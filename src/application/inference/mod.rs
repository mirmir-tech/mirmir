mod generation;

pub use generation::{GenerationEvent, GenerationResult, GenerationSession};

use super::{Application, Result};

impl Application {
    pub fn embed(
        &self,
        selector: &str,
        request: libmir::EmbeddingRequest,
    ) -> Result<libmir::EmbeddingOutput> {
        self.runtime.embed(selector, request)
    }

    pub fn rerank(
        &self,
        selector: &str,
        request: libmir::RerankRequest,
    ) -> Result<libmir::RerankOutput> {
        self.runtime.rerank(selector, request)
    }
}
