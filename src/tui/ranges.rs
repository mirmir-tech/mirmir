use crate::rpc::proto;

pub const GENERATION_FIELDS: usize = 5;
const DEFAULT_TOKEN_LIMIT: u64 = 131_072;
const BAR_WIDTH: usize = 12;

/// Bounds and stepping for one editable generation setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldRange {
    pub label: &'static str,
    pub minimum: f64,
    pub maximum: f64,
    pub step: f64,
    pub precision: usize,
}

/// The one table every generation-settings editor uses, in field order.
pub const GENERATION_RANGES: [FieldRange; GENERATION_FIELDS] = [
    FieldRange::new("max tokens", 1.0, 131_072.0, 256.0, 0),
    FieldRange::new("temperature", 0.0, 2.0, 0.05, 2),
    FieldRange::new("top p", 0.0, 1.0, 0.01, 2),
    FieldRange::new("top k", 0.0, 200.0, 1.0, 0),
    FieldRange::new("repetition penalty", 0.5, 2.0, 0.01, 2),
];

/// The generation ranges with max tokens bounded by `token_limit`.
pub fn generation_ranges(token_limit: Option<u64>) -> [FieldRange; GENERATION_FIELDS] {
    let limit = token_limit.unwrap_or(DEFAULT_TOKEN_LIMIT).max(1);
    let mut ranges = GENERATION_RANGES;
    ranges[0].maximum = f64::from(u32::try_from(limit).unwrap_or(u32::MAX));
    ranges
}

/// The smallest known bound on generated tokens: the context and the input
/// limit.
pub fn token_limit(
    context_tokens: Option<u64>,
    capabilities: Option<&proto::ModelTaskCapabilities>,
) -> Option<u64> {
    let input = capabilities.map(|capabilities| capabilities.max_input_tokens);
    [context_tokens, input].into_iter().flatten().filter(|tokens| *tokens > 0).min()
}

impl FieldRange {
    const fn new(
        label: &'static str,
        minimum: f64,
        maximum: f64,
        step: f64,
        precision: usize,
    ) -> Self {
        Self { label, minimum, maximum, step, precision }
    }

    /// Moves `value` one step along the step grid, clamped to the range.
    pub fn adjust(&self, value: &str, direction: i8) -> String {
        let current =
            value.parse::<f64>().unwrap_or(self.minimum).clamp(self.minimum, self.maximum);
        let next = (current / self.step).round() + f64::from(direction);
        self.format((next * self.step).clamp(self.minimum, self.maximum))
    }

    /// Parses `value`, replacing an out-of-range number with the nearest bound.
    pub fn clamped<T: std::str::FromStr>(&self, value: &str) -> Result<T, String> {
        let invalid = || format!("invalid {}: `{value}`", self.label);
        let number = value.parse::<f64>().ok().filter(|number| number.is_finite());
        let text = match number {
            None => return Err(invalid()),
            Some(number) if number < self.minimum => self.format(self.minimum),
            Some(number) if number > self.maximum => self.format(self.maximum),
            Some(_) => value.to_owned(),
        };
        text.parse().map_or_else(|_| Err(invalid()), Ok)
    }

    /// A fixed-width bar showing where `value` sits in the range.
    pub fn bar(&self, value: &str) -> String {
        let current =
            value.parse::<f64>().unwrap_or(self.minimum).clamp(self.minimum, self.maximum);
        let span = self.maximum - self.minimum;
        bar(if span > 0.0 {
            (current - self.minimum) / span
        } else {
            1.0
        })
    }

    fn format(&self, value: f64) -> String {
        let precision = self.precision;
        format!("{value:.precision$}")
    }
}

/// A fixed-width bar filled to `ratio` (0–1).
pub fn bar(ratio: f64) -> String {
    let filled = format!("{:.0}", ratio.clamp(0.0, 1.0) * 12.0)
        .parse::<usize>()
        .unwrap_or(0)
        .min(BAR_WIDTH);
    format!("{}{}", "━".repeat(filled), "─".repeat(BAR_WIDTH - filled))
}

/// A token count with thousands separators, e.g. `194,128`.
pub fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjusts_along_the_step_grid_within_bounds() {
        let [tokens, temperature, top_p, top_k, penalty] = generation_ranges(Some(8192));
        assert_eq!(tokens.adjust("1", 1), "256");
        assert_eq!(tokens.adjust("256", -1), "1");
        assert_eq!(tokens.adjust("8192", 1), "8192");
        assert_eq!(temperature.adjust("0.7", 1), "0.75");
        assert_eq!(temperature.adjust("2", 1), "2.00");
        assert_eq!(top_p.adjust("0", -1), "0.00");
        assert_eq!(top_k.adjust("", 1), "1");
        assert_eq!(penalty.adjust("1.1", -1), "1.09");
    }

    #[test]
    fn clamps_typed_values_on_confirm() {
        let [tokens, temperature, top_p, top_k, penalty] = generation_ranges(Some(4096));
        assert_eq!(tokens.clamped::<u64>("99999"), Ok(4096));
        assert_eq!(tokens.clamped::<u64>("0"), Ok(1));
        assert_eq!(temperature.clamped::<f32>("5").map(|value| value.to_string()), Ok("2".into()));
        assert_eq!(top_p.clamped::<f32>("0.25").map(|value| value.to_string()), Ok("0.25".into()));
        assert_eq!(top_k.clamped::<u64>("500"), Ok(200));
        assert_eq!(penalty.clamped::<f32>("0.1").map(|value| value.to_string()), Ok("0.5".into()));
        assert!(tokens.clamped::<u64>("1.2.3").is_err());
    }

    #[test]
    fn bounds_max_tokens_by_context_and_input_limit() {
        let capabilities = proto::ModelTaskCapabilities {
            max_input_tokens: 32_768,
            ..Default::default()
        };
        assert_eq!(token_limit(Some(8192), Some(&capabilities)), Some(8192));
        assert_eq!(token_limit(Some(65_536), Some(&capabilities)), Some(32_768));
        assert_eq!(token_limit(None, None), None);
        assert_eq!(grouped(194_128), "194,128");
        assert_eq!(grouped(512), "512");
    }
}
