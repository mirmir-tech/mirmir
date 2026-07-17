use libmir::runtime::metrics::GenerationMetrics;

use super::{GREEN, kv, section};

#[must_use]
pub fn production_bench_report(
    model: &str,
    prompt_tokens: usize,
    max_tokens: usize,
    warmup: usize,
    sampling: &str,
    diagnostics: &str,
    samples: &[GenerationMetrics],
) -> Vec<String> {
    let generated = samples.iter().map(|sample| sample.tokens.generated).collect::<Vec<_>>();
    let lines = vec![
        kv("model", model),
        kv("path", "production Engine chat prefill/decode"),
        kv("prefix_cache", "cleared before every sample"),
        kv("runs", &format!("{} measured after {warmup} warmups", samples.len())),
        kv("prompt", &format!("{prompt_tokens} tokens | max {max_tokens} generated")),
        kv("sampling", sampling),
        kv("diagnostics", diagnostics),
        kv(
            "prefill",
            &rate_summary(samples.iter().map(|sample| sample.throughput.prefill.per_second)),
        ),
        kv(
            "decode",
            &rate_summary(samples.iter().map(|sample| sample.throughput.decode.per_second)),
        ),
        kv("generated", &range_summary(&generated)),
    ];
    section("bench", GREEN, &lines)
}

fn rate_summary(rates: impl Iterator<Item = Option<f64>>) -> String {
    let mut rates = rates.flatten().collect::<Vec<_>>();
    if rates.is_empty() {
        return "n/a".into();
    }
    rates.sort_by(f64::total_cmp);
    let median = rates[rates.len() / 2];
    let best = *rates.last().unwrap_or(&median);
    format!("median {median:.2} tok/s | best {best:.2} tok/s")
}

fn range_summary(values: &[usize]) -> String {
    let Some(minimum) = values.iter().min() else {
        return "none".into();
    };
    let maximum = values.iter().max().unwrap_or(minimum);
    format!("{minimum}..{maximum} tokens")
}
