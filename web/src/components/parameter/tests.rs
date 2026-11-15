use super::range::{
    MAX_TOKENS_FALLBACK, REPETITION_PENALTY, Range, TEMPERATURE, TOP_K, max_tokens_limit, whole,
};

#[test]
fn values_snap_to_the_step_and_stay_in_bounds() {
    assert!((TEMPERATURE.clamp(0.72) - 0.7).abs() < 1e-12);
    assert!((TEMPERATURE.clamp(9.0) - 2.0).abs() < 1e-12);
    assert!((REPETITION_PENALTY.clamp(0.0) - 0.5).abs() < 1e-12);
    assert!((TOP_K.clamp(40.4) - 40.0).abs() < 1e-12);
    assert!((TOP_K.clamp(f64::NAN)).abs() < 1e-12);
    assert!((TEMPERATURE.clamp(f64::from(0.7_f32)) - 0.7).abs() < 1e-12);
}

#[test]
fn entries_parse_clamped_and_format_with_the_step_precision() {
    assert_eq!(TEMPERATURE.parse(" 3 "), Some(2.0));
    assert_eq!(TEMPERATURE.parse(""), None);
    assert_eq!(TEMPERATURE.parse("warm"), None);
    assert_eq!(TEMPERATURE.parse("inf"), None);
    assert_eq!(TEMPERATURE.format(0.7), "0.70");
    assert_eq!(TOP_K.format(40.0), "40");
    assert_eq!(Range::max_tokens(0), Range::new(1.0, 1.0, 1.0));
    assert_eq!(whole(Range::max_tokens(8192).clamp(9000.0)), 8192);
}

#[test]
fn max_tokens_follow_the_tighter_known_limit() {
    assert_eq!(max_tokens_limit(Some(8192), Some(40_960)), 8192);
    assert_eq!(max_tokens_limit(Some(194_128), Some(40_960)), 40_960);
    assert_eq!(max_tokens_limit(None, Some(40_960)), 40_960);
    assert_eq!(max_tokens_limit(Some(4096), None), 4096);
    assert_eq!(max_tokens_limit(None, None), MAX_TOKENS_FALLBACK);
    assert_eq!(max_tokens_limit(Some(0), None), 1);
}
