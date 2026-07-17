use serde::Serialize;

use super::Distribution;

#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkReport {
    pub model: String,
    pub warmup: usize,
    pub server_reused: bool,
    pub samples: Vec<SampleReport>,
    pub aggregate: Aggregate,
}

#[derive(Debug, Clone, Serialize)]
pub struct SampleReport {
    pub text: String,
    pub reasoning: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub finish_reason: String,
    pub elapsed_ms: f64,
    pub ttft_ms: Option<f64>,
    pub tokens_per_second: Option<f64>,
    pub prefill_tokens_per_second: Option<f64>,
    pub decode_tokens_per_second: Option<f64>,
    pub prefill_ms: Option<f64>,
    pub decode_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Aggregate {
    pub samples: usize,
    pub elapsed_ms: Distribution,
    pub ttft_ms: Option<Distribution>,
    pub e2e_tokens_per_second: Option<Distribution>,
    pub prefill_tokens_per_second: Option<Distribution>,
    pub decode_tokens_per_second: Option<Distribution>,
    pub prefill_ms: Option<Distribution>,
    pub decode_ms: Option<Distribution>,
}

impl Aggregate {
    pub fn from_samples(samples: &[SampleReport]) -> Self {
        Self {
            samples: samples.len(),
            elapsed_ms: Distribution::new(samples.iter().map(|sample| sample.elapsed_ms)),
            ttft_ms: optional(samples.iter().filter_map(|sample| sample.ttft_ms)),
            e2e_tokens_per_second: optional(
                samples.iter().filter_map(|sample| sample.tokens_per_second),
            ),
            prefill_tokens_per_second: optional(
                samples.iter().filter_map(|sample| sample.prefill_tokens_per_second),
            ),
            decode_tokens_per_second: optional(
                samples.iter().filter_map(|sample| sample.decode_tokens_per_second),
            ),
            prefill_ms: optional(samples.iter().filter_map(|sample| sample.prefill_ms)),
            decode_ms: optional(samples.iter().filter_map(|sample| sample.decode_ms)),
        }
    }

    pub const fn metrics(&self) -> [(&'static str, Option<&Distribution>, &'static str); 7] {
        [
            ("elapsed", Some(&self.elapsed_ms), "ms"),
            ("ttft", self.ttft_ms.as_ref(), "ms"),
            ("e2e", self.e2e_tokens_per_second.as_ref(), "tok/s"),
            ("prefill", self.prefill_tokens_per_second.as_ref(), "tok/s"),
            ("decode", self.decode_tokens_per_second.as_ref(), "tok/s"),
            ("prefill_time", self.prefill_ms.as_ref(), "ms"),
            ("decode_time", self.decode_ms.as_ref(), "ms"),
        ]
    }
}

fn optional(values: impl Iterator<Item = f64>) -> Option<Distribution> {
    let values = values.collect::<Vec<_>>();
    (!values.is_empty()).then(|| Distribution::new(values.into_iter()))
}
