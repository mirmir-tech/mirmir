mod csv;
mod report;
mod statistics;

use std::{fs, path::Path};

pub use report::{Aggregate, BenchmarkReport, SampleReport};
pub use statistics::Distribution;

use crate::{
    cli::PromptArgs,
    error::{Error, Result},
    output,
    rpc::{self, PROTOCOL_VERSION, proto},
};

pub async fn execute(
    client: &mut rpc::Client,
    args: &PromptArgs,
    model: String,
    prompt: String,
    server_reused: bool,
) -> Result<()> {
    check_protocol(client).await?;
    let request = request(args, model.clone(), prompt)?;
    for _ in 0..args.warmup {
        drop(sample(client, request.clone(), false).await?);
    }
    let mut samples = Vec::with_capacity(args.samples);
    for index in 0..args.samples {
        if args.samples > 1 && !args.json {
            output::diagnostic(format!("sample {}/{}", index + 1, args.samples))?;
        }
        let completion = sample(client, request.clone(), !args.json && !args.no_stream).await?;
        if !args.json {
            present_sample(args, &completion)?;
        }
        samples.push(SampleReport::from(completion));
    }
    let report = BenchmarkReport {
        model,
        warmup: args.warmup,
        server_reused,
        aggregate: Aggregate::from_samples(&samples),
        samples,
    };
    if let Some(path) = &args.csv {
        write_csv(path, &report)?;
        if !args.json {
            output::diagnostic(format!("CSV written to {}", path.display()))?;
        }
    }
    if args.json {
        output::json(&report)
    } else {
        present_aggregate(&report.aggregate)
    }
}

pub fn write_csv(path: &Path, report: &BenchmarkReport) -> Result<()> {
    csv::write(path, report)
}

pub fn write_json(path: &Path, report: &BenchmarkReport) -> Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(report)?)?;
    Ok(())
}

async fn check_protocol(client: &mut rpc::Client) -> Result<()> {
    let health = client.health(proto::HealthRequest {}).await?.into_inner();
    if health.protocol_version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(Error::Config(format!(
            "server protocol {} is incompatible with client protocol {PROTOCOL_VERSION}",
            health.protocol_version
        )))
    }
}

fn request(args: &PromptArgs, model: String, prompt: String) -> Result<proto::GenerateRequest> {
    Ok(proto::GenerateRequest {
        model,
        prompt,
        max_tokens: args.max_tokens.map(u64::try_from).transpose()?,
        temperature: args.temperature,
        top_p: args.top_p,
        top_k: args.top_k.map(u64::try_from).transpose()?,
        repetition_penalty: args.repetition_penalty,
        seed: args.seed,
        messages: Vec::new(),
    })
}

async fn sample(
    client: &mut rpc::Client,
    request: proto::GenerateRequest,
    stream_tokens: bool,
) -> Result<proto::Completion> {
    let mut stream = client.generate(request).await?.into_inner();
    let mut cancellation_client = client.clone();
    let mut completion = None;
    let mut operation_id = None;
    loop {
        let message = tokio::select! {
            message = stream.message() => message?,
            result = tokio::signal::ctrl_c() => {
                result?;
                if let Some(operation_id) = operation_id {
                    drop(cancellation_client.cancel_operation(proto::CancelOperationRequest {
                        operation_id,
                    }).await);
                }
                return Err(Error::Config("generation cancelled".to_owned()));
            },
        };
        let Some(message) = message else {
            break;
        };
        match message.event {
            Some(proto::generate_event::Event::Started(started)) => {
                operation_id = Some(started.operation_id);
            },
            Some(proto::generate_event::Event::Token(token)) if stream_tokens => {
                output::stream(&token.text)?;
            },
            Some(proto::generate_event::Event::Completion(value)) => completion = Some(value),
            _ => {},
        }
    }
    completion.ok_or_else(|| Error::Config("server stream ended without completion".to_owned()))
}

fn present_sample(args: &PromptArgs, completion: &proto::Completion) -> Result<()> {
    if args.no_stream {
        output::line(&completion.text)?;
    } else {
        output::line("")?;
    }
    output::diagnostic(format!(
        "prompt_tokens={} completion_tokens={} elapsed_ms={:.3} ttft_ms={} rate={}",
        completion.prompt_tokens,
        completion.completion_tokens,
        completion.elapsed_ms,
        metric(completion.ttft_ms, "ms"),
        metric(completion.tokens_per_second, "tok/s")
    ))
}

fn present_aggregate(aggregate: &Aggregate) -> Result<()> {
    output::diagnostic(format!("aggregate samples={}", aggregate.samples))?;
    for (name, distribution, unit) in aggregate.metrics() {
        if let Some(value) = distribution {
            output::diagnostic(format!(
                "{name} mean={:.3} median={:.3} p95={:.3} stddev={:.3} {unit}",
                value.mean, value.median, value.p95, value.stddev,
            ))?;
        }
    }
    Ok(())
}

fn metric(value: Option<f64>, unit: &str) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3} {unit}"))
}

impl From<proto::Completion> for SampleReport {
    fn from(value: proto::Completion) -> Self {
        Self {
            text: value.text,
            reasoning: value.reasoning,
            prompt_tokens: value.prompt_tokens,
            completion_tokens: value.completion_tokens,
            finish_reason: value.finish_reason,
            elapsed_ms: value.elapsed_ms,
            ttft_ms: value.ttft_ms,
            tokens_per_second: value.tokens_per_second,
            prefill_tokens_per_second: value.prefill_tokens_per_second,
            decode_tokens_per_second: value.decode_tokens_per_second,
            prefill_ms: value.prefill_ms,
            decode_ms: value.decode_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_rates_and_ttft() {
        let samples = [sample_report(10.0, 2.0, 100.0), sample_report(20.0, 4.0, 200.0)];
        let aggregate = Aggregate::from_samples(&samples);
        assert_eq!(aggregate.samples, 2);
        assert_eq!(aggregate.e2e_tokens_per_second.as_ref().map(|value| value.mean), Some(15.0));
        assert_eq!(aggregate.ttft_ms.as_ref().map(|value| value.mean), Some(3.0));
        assert!((aggregate.elapsed_ms.mean - 150.0).abs() < f64::EPSILON);
    }

    fn sample_report(rate: f64, ttft: f64, elapsed: f64) -> SampleReport {
        SampleReport {
            text: String::new(),
            reasoning: String::new(),
            prompt_tokens: 1,
            completion_tokens: 1,
            finish_reason: "stop".to_owned(),
            elapsed_ms: elapsed,
            ttft_ms: Some(ttft),
            tokens_per_second: Some(rate),
            prefill_tokens_per_second: Some(rate * 2.0),
            decode_tokens_per_second: Some(rate),
            prefill_ms: Some(10.0),
            decode_ms: Some(90.0),
        }
    }
}
