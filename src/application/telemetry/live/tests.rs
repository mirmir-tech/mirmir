use super::*;

#[test]
fn snapshots_active_prefill_and_decode_without_waiting_for_completion() {
    let registry = Registry::new();
    let request = registry.begin();
    request.progress(&ProgressEvent::prefill_tokens(3, 8));
    let prefill = registry.snapshot();
    assert_eq!(prefill.requests, 1);
    assert_eq!(prefill.prompt_tokens, 8);
    assert!(prefill.prefill_rate.is_some());
    request.progress(&ProgressEvent::decode_tokens(2, 8));
    request.token_emitted();
    let decode = registry.snapshot();
    assert_eq!(decode.completion_tokens, 2);
    assert!(decode.prefill_rate.is_some());
    assert!(decode.decode_rate.is_some());
    assert_eq!(decode.stage.map(Stage::as_str), Some("decode"));
    request.finish();
    let finished = registry.snapshot();
    assert_eq!(finished.requests, 0);
    assert!(finished.rate.is_none());
}
