use super::*;

#[test]
fn records_completed_and_failed_requests() {
    let telemetry = Telemetry::new(
        std::env::temp_dir().join(format!("mirmir-telemetry-counters-{}.toml", std::process::id())),
    );
    let mut completed = telemetry.begin();
    completed.complete(&crate::application::CompletionMetrics {
        prompt_tokens: 3,
        completion_tokens: 5,
        tokens_per_second: Some(50.0),
        ttft_ms: Some(2.0),
        prefill_tokens_per_second: Some(120.0),
        decode_tokens_per_second: Some(42.0),
    });
    drop(completed);
    let mut failed = telemetry.begin();
    failed.fail();
    drop(failed);
    assert_eq!(telemetry.0.active.load(Ordering::Relaxed), 0);
    assert_eq!(telemetry.0.total.load(Ordering::Relaxed), 2);
    assert_eq!(telemetry.0.completed.load(Ordering::Relaxed), 1);
    assert_eq!(telemetry.0.failed.load(Ordering::Relaxed), 1);
    assert_eq!(telemetry.0.prompt_tokens.load(Ordering::Relaxed), 3);
    assert_eq!(telemetry.0.completion_tokens.load(Ordering::Relaxed), 5);
    assert!(telemetry.0.live.snapshot().rate.is_none());
    let rates = telemetry
        .0
        .rates
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .snapshot();
    assert_eq!(rates.last_prefill_rate, Some(120.0));
    assert_eq!(rates.last_decode_rate, Some(42.0));
}
