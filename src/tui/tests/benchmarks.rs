use super::{App, Screen, rendered};
use crate::{
    prompt::benchmark::{Aggregate, SampleReport},
    rpc::proto::ModelInfo,
};

#[test]
fn renders_benchmark_distributions_and_prompt() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Benchmarks;
    app.models.push(ModelInfo {
        id: "Qwen--Test".to_owned(),
        path: "/models/test".to_owned(),
        ..Default::default()
    });
    app.benchmark.prompt = "Explain local inference.".to_owned();
    app.benchmark.results = vec![sample(20.0, 40.0), sample(30.0, 60.0)];
    app.benchmark.aggregate = Some(Aggregate::from_samples(&app.benchmark.results));

    let text = rendered(&mut app, 100, 28)?;
    assert!(text.contains("BENCHMARK MODEL"));
    assert!(text.contains("DISTRIBUTIONS"));
    assert!(text.contains("decode"));
    assert!(text.contains("p95"));
    assert!(text.contains("Explain local inference."));
    Ok(())
}

fn sample(decode: f64, ttft: f64) -> SampleReport {
    SampleReport {
        text: "ok".to_owned(),
        reasoning: String::new(),
        prompt_tokens: 4,
        completion_tokens: 8,
        finish_reason: "stop".to_owned(),
        elapsed_ms: 500.0,
        ttft_ms: Some(ttft),
        tokens_per_second: Some(16.0),
        prefill_tokens_per_second: Some(120.0),
        decode_tokens_per_second: Some(decode),
        prefill_ms: Some(20.0),
        decode_ms: Some(400.0),
    }
}
