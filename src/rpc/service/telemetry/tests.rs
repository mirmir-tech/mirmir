use super::*;

#[test]
fn records_completed_and_failed_requests() {
    let telemetry = Telemetry::new(
        std::env::temp_dir().join(format!("mirmir-telemetry-counters-{}.toml", std::process::id())),
    );
    let mut completed = telemetry.begin();
    completed.complete(&proto::Completion {
        reasoning: String::new(),
        text: String::new(),
        prompt_tokens: 3,
        completion_tokens: 5,
        finish_reason: "stop".to_owned(),
        elapsed_ms: 10.0,
        tokens_per_second: Some(50.0),
        ttft_ms: Some(2.0),
        ..Default::default()
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
}
