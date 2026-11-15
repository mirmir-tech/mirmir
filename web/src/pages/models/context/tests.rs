use super::scale::{STEPS, Scale, effective, grouped};
use crate::types::catalog::{ContextChoice, ContextInfo, Fit};

const QWEN: Scale = Scale::new(16, 16, 40_960);

#[test]
fn contexts_snap_to_whole_blocks_within_the_limits() {
    assert_eq!(QWEN.clamp(8_190), 8_192);
    assert_eq!(QWEN.clamp(8_199), 8_192);
    assert_eq!(QWEN.clamp(0), 16);
    assert_eq!(QWEN.clamp(194_128), 40_960);
    assert_eq!(Scale::new(16, 16, 40_000).clamp(40_000), 40_000);
    assert_eq!(Scale::new(16, 16, 40_005).clamp(50_000), 40_000);
    assert_eq!(Scale::new(0, 0, 0).clamp(7), 1);
}

#[test]
fn slider_ends_reach_the_limits_and_positions_round_trip() {
    assert_eq!(QWEN.tokens_at(0), 16);
    assert_eq!(QWEN.tokens_at(STEPS), 40_960);
    assert_eq!(QWEN.tokens_at(STEPS + 5), 40_960);
    assert_eq!(QWEN.position_of(16), 0);
    assert_eq!(QWEN.position_of(1_000_000), STEPS);
    for tokens in [16, 512, 4_096, 8_192, 32_768] {
        let back = QWEN.tokens_at(QWEN.position_of(tokens));
        assert!(back.abs_diff(tokens) * 100 <= tokens, "{tokens} came back as {back}");
        assert_eq!(back % 16, 0);
    }
    assert_eq!(Scale::new(16, 64, 64).position_of(64), 0);
}

#[test]
fn the_scale_is_logarithmic() {
    let middle = QWEN.tokens_at(STEPS / 2);
    assert!(middle.abs_diff(810) <= 16, "midpoint {middle}");
}

#[test]
fn choices_resolve_and_counts_are_grouped() {
    let info = ContextInfo {
        block_tokens: 16,
        min_tokens: 16,
        max_tokens: 40_960,
        automatic_tokens: 194_128,
        ..ContextInfo::default()
    };
    assert_eq!(effective(ContextChoice::Automatic, &info), 194_128);
    assert_eq!(effective(ContextChoice::Tokens(8_192), &info), 8_192);
    assert_eq!(Scale::from(&info), QWEN);
    assert_eq!(grouped(194_128), "194,128");
    assert_eq!(grouped(1_234_567), "1,234,567");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(0), "0");
}

#[test]
fn inspection_json_decodes_the_context_and_fit() -> Result<(), serde_json::Error> {
    let tokens: ContextChoice = serde_json::from_str(r#"{"tokens":8192}"#)?;
    assert_eq!(tokens, ContextChoice::Tokens(8_192));
    assert_eq!(tokens.query(), "8192");
    let automatic: ContextChoice = serde_json::from_str(r#""automatic""#)?;
    assert_eq!(automatic.query(), "automatic");
    assert_eq!(serde_json::to_string(&tokens)?, r#"{"tokens":8192}"#);
    let fit: Fit = serde_json::from_str(r#""does_not_fit""#)?;
    assert_eq!(fit, Fit::Exceeds);
    let later: Fit = serde_json::from_str(r#""borderline""#)?;
    assert_eq!(later, Fit::Unknown);
    Ok(())
}
