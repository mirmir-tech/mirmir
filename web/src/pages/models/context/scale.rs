//! The context slider's logarithmic token scale.

use crate::types::catalog::{ContextChoice, ContextInfo};

/// Slider positions between the smallest and largest context.
pub const STEPS: u32 = 1_000;

/// A model's selectable contexts: whole K/V blocks from `min` to `max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale {
    block: u64,
    min: u64,
    max: u64,
}

impl Scale {
    pub const fn new(block: u64, min: u64, max: u64) -> Self {
        let block = if block == 0 {
            1
        } else {
            block
        };
        let min = if min < block {
            block
        } else {
            min
        };
        Self {
            block,
            min,
            max: if max < min {
                min
            } else {
                max
            },
        }
    }

    /// `tokens` rounded to the nearest whole block, within the limits.
    pub fn clamp(self, tokens: u64) -> u64 {
        let snapped = tokens.saturating_add(self.block / 2) / self.block * self.block;
        if snapped > self.max {
            let floor = self.max / self.block * self.block;
            if floor >= self.min {
                floor
            } else {
                self.max
            }
        } else {
            snapped.max(self.min)
        }
    }

    /// The context at slider `position` (`0..=STEPS`).
    pub fn tokens_at(self, position: u32) -> u64 {
        let ratio = f64::from(position.min(STEPS)) / f64::from(STEPS);
        let (low, high) = (self.min as f64, self.max as f64);
        let tokens = ratio.mul_add(high.ln() - low.ln(), low.ln()).exp();
        self.clamp(rounded(tokens))
    }

    /// The slider position closest to `tokens`.
    pub fn position_of(self, tokens: u64) -> u32 {
        if self.max == self.min {
            return 0;
        }
        let tokens = tokens.clamp(self.min, self.max) as f64;
        let (low, high) = (self.min as f64, self.max as f64);
        let ratio = (tokens.ln() - low.ln()) / (high.ln() - low.ln());
        u32::try_from(rounded(ratio * f64::from(STEPS))).unwrap_or(STEPS).min(STEPS)
    }
}

impl From<&ContextInfo> for Scale {
    fn from(info: &ContextInfo) -> Self {
        Self::new(info.block_tokens, info.min_tokens, info.max_tokens)
    }
}

/// The token count a choice stands for now.
pub const fn effective(choice: ContextChoice, info: &ContextInfo) -> u64 {
    match choice {
        ContextChoice::Automatic => info.automatic_tokens,
        ContextChoice::Tokens(tokens) => tokens,
    }
}

/// `value` with thousands separators: `194,128`.
pub fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn rounded(value: f64) -> u64 {
    value.round().max(0.0) as u64
}
