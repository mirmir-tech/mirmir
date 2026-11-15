//! Bounds shared by every generation-parameter control.

/// Used when neither the context nor the model bounds max tokens.
pub const MAX_TOKENS_FALLBACK: u64 = 32_768;

pub const TEMPERATURE: Range = Range::new(0.0, 2.0, 0.05);
pub const TOP_P: Range = Range::new(0.0, 1.0, 0.01);
pub const TOP_K: Range = Range::new(0.0, 200.0, 1.0);
pub const REPETITION_PENALTY: Range = Range::new(0.5, 2.0, 0.01);

/// An inclusive, stepped range of a parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl Range {
    pub const fn new(min: f64, max: f64, step: f64) -> Self {
        Self { min, max, step }
    }

    /// Max tokens from one up to `limit`.
    pub fn max_tokens(limit: u64) -> Self {
        Self::new(1.0, limit.max(1) as f64, 1.0)
    }

    /// `value` snapped to the step grid and bounded by the range.
    pub fn clamp(self, value: f64) -> f64 {
        if !value.is_finite() {
            return self.min;
        }
        let snapped = ((value - self.min) / self.step).round().mul_add(self.step, self.min);
        let scale = 10_f64.powi(i32::try_from(self.decimals()).unwrap_or(0));
        ((snapped * scale).round() / scale).clamp(self.min, self.max)
    }

    /// A typed number, clamped; `None` for an empty or invalid entry.
    pub fn parse(self, text: &str) -> Option<f64> {
        text.trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| self.clamp(value))
    }

    /// The value with as many decimals as the step needs.
    pub fn format(self, value: f64) -> String {
        format!("{value:.precision$}", precision = self.decimals())
    }

    /// Where an unset value rests on the slider.
    pub fn middle(self) -> f64 {
        self.clamp(f64::midpoint(self.min, self.max))
    }

    fn decimals(self) -> usize {
        let mut step = self.step;
        let mut decimals = 0;
        while decimals < 6 && (step - step.round()).abs() > 1e-9 {
            step *= 10.0;
            decimals += 1;
        }
        decimals
    }
}

/// The max-tokens ceiling: the smaller of the context and the model's input
/// limit, whichever are known.
pub fn max_tokens_limit(context: Option<u64>, max_input: Option<u64>) -> u64 {
    match (context, max_input) {
        (Some(context), Some(input)) => context.min(input),
        (Some(limit), None) | (None, Some(limit)) => limit,
        (None, None) => MAX_TOKENS_FALLBACK,
    }
    .max(1)
}

/// A clamped slider value as a whole count.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub const fn whole(value: f64) -> u64 {
    value.round() as u64
}

/// A clamped slider value as a sampling parameter.
#[allow(clippy::cast_possible_truncation)]
pub const fn single(value: f64) -> f32 {
    value as f32
}
