use super::*;

#[test]
fn gaps_are_not_drawn_as_measurements_and_time_controls_spacing() {
    let sample = |time, rate| TelemetryPoint {
        sampled_at_unix_ms: time,
        prefill_tokens_per_second: rate,
        ..Default::default()
    };
    let result = plot(
        &[sample(0, Some(20.0)), sample(1000, None), sample(3000, Some(30.0))],
        Metric::Prefill,
    );
    assert_eq!(result.line.matches('M').count(), 2);
    assert_eq!(result.area.matches('Z').count(), 2);
    assert!(result.line.contains("M640.0,"));
    assert_eq!(result.seconds, 3);
}

#[test]
fn unavailable_and_nonfinite_measurements_have_no_fill() {
    let result = plot(
        &[TelemetryPoint {
            decode_tokens_per_second: Some(f64::NAN),
            ..Default::default()
        }],
        Metric::Decode,
    );
    assert!(result.line.is_empty());
    assert!(result.area.is_empty());
}
