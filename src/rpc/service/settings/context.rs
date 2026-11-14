//! A model's context between the protocol and the application.

use std::num::NonZeroU64;

use tonic::Status;

use crate::{
    application::{ContextChoice, ModelContext},
    rpc::proto::{self, model_context_choice::Choice},
};

pub(super) fn choice(value: &proto::ModelContextChoice) -> Result<ContextChoice, Status> {
    match value.choice {
        Some(Choice::Automatic(true)) => Ok(ContextChoice::Automatic),
        Some(Choice::Tokens(tokens)) => NonZeroU64::new(tokens)
            .map(ContextChoice::Tokens)
            .ok_or_else(|| Status::invalid_argument("a context needs at least one token")),
        Some(Choice::Automatic(false)) | None => {
            Err(Status::invalid_argument("a context is automatic or a token count"))
        },
    }
}

pub(super) const fn wire(value: ContextChoice) -> proto::ModelContextChoice {
    proto::ModelContextChoice {
        choice: Some(match value {
            ContextChoice::Automatic => Choice::Automatic(true),
            ContextChoice::Tokens(tokens) => Choice::Tokens(tokens.get()),
        }),
    }
}

pub(super) const fn model_context(value: ModelContext) -> proto::ModelContext {
    proto::ModelContext {
        block_tokens: value.limits.block_tokens.get(),
        min_tokens: value.limits.min_tokens(),
        max_tokens: value.limits.max_tokens,
        saved: Some(wire(value.saved)),
        automatic_tokens: value.automatic_tokens,
        resident_tokens: value.resident_tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_round_trips_and_an_empty_one_is_refused() -> Result<(), Status> {
        let tokens = ContextChoice::Tokens(NonZeroU64::MIN.saturating_add(4_095));
        assert_eq!(choice(&wire(tokens))?, tokens);
        assert_eq!(choice(&wire(ContextChoice::Automatic))?, ContextChoice::Automatic);
        assert!(choice(&proto::ModelContextChoice { choice: None }).is_err());
        assert!(choice(&proto::ModelContextChoice { choice: Some(Choice::Tokens(0)) }).is_err());
        Ok(())
    }
}
