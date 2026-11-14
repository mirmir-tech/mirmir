//! The K/V capacity, in tokens, a model is given when it loads.

use std::num::{NonZeroU32, NonZeroU64};

use super::{Error, Result};

/// The context a model loads with: sized automatically from free memory, or
/// a fixed token count, stored as whole K/V blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextChoice {
    Automatic,
    Tokens(NonZeroU64),
}

/// The context a model can be given and the one it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelContext {
    pub limits: ContextLimits,
    /// The choice saved for the model, which every load uses.
    pub saved: ContextChoice,
    /// The context automatic sizing would give the model now.
    pub automatic_tokens: u64,
    /// The context the model serves with while it is loaded.
    pub resident_tokens: Option<u64>,
}

/// What a chosen context may be for one model: whole blocks, from one block
/// up to the model's own context length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextLimits {
    pub block_tokens: NonZeroU64,
    pub max_tokens: u64,
}

impl ContextLimits {
    #[must_use]
    pub const fn min_tokens(self) -> u64 {
        self.block_tokens.get()
    }

    /// The K/V blocks a load with `choice` uses; `None` sizes automatically.
    pub fn blocks(self, choice: ContextChoice) -> Result<Option<NonZeroU32>> {
        let ContextChoice::Tokens(tokens) = choice else {
            return Ok(None);
        };
        let maximum = self.max_tokens.max(self.min_tokens());
        if tokens.get() > maximum {
            return Err(Error::InvalidContext(format!(
                "{tokens} tokens exceed the model's context of {maximum} tokens"
            )));
        }
        let blocks = tokens.get().div_ceil(self.block_tokens.get());
        NonZeroU32::new(u32::try_from(blocks).unwrap_or(u32::MAX))
            .map(Some)
            .ok_or_else(|| Error::InvalidContext("a context needs at least one block".to_owned()))
    }

    /// The choice that `blocks` saved for a model stand for.
    #[must_use]
    pub fn choice(self, blocks: Option<NonZeroU32>) -> ContextChoice {
        blocks.map_or(ContextChoice::Automatic, |blocks| {
            ContextChoice::Tokens(NonZeroU64::from(blocks).saturating_mul(self.block_tokens))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: ContextLimits = ContextLimits {
        block_tokens: NonZeroU64::new(16).unwrap(),
        max_tokens: 32_768,
    };

    fn tokens(value: u64) -> ContextChoice {
        ContextChoice::Tokens(NonZeroU64::new(value).unwrap())
    }

    #[test]
    fn automatic_has_no_blocks() -> Result<()> {
        assert_eq!(LIMITS.blocks(ContextChoice::Automatic)?, None);
        assert_eq!(LIMITS.choice(None), ContextChoice::Automatic);
        Ok(())
    }

    #[test]
    fn tokens_round_up_to_whole_blocks() -> Result<()> {
        assert_eq!(LIMITS.blocks(tokens(1))?.map(NonZeroU32::get), Some(1));
        assert_eq!(LIMITS.blocks(tokens(17))?.map(NonZeroU32::get), Some(2));
        assert_eq!(LIMITS.blocks(tokens(32_768))?.map(NonZeroU32::get), Some(2_048));
        assert_eq!(LIMITS.choice(NonZeroU32::new(2_048)), tokens(32_768));
        Ok(())
    }

    #[test]
    fn a_context_beyond_the_model_is_refused() {
        assert!(matches!(LIMITS.blocks(tokens(32_769)), Err(Error::InvalidContext(_))));
    }
}
