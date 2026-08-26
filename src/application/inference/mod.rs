mod generation;

pub use generation::{GenerationResult, GenerationSession};

use super::{Result, RuntimeCoordinator};

impl RuntimeCoordinator {
    pub fn embed(
        &self,
        selector: &str,
        request: libmir::EmbeddingRequest,
    ) -> Result<libmir::EmbeddingOutput> {
        let model = self.inference_model(selector)?;
        Ok(model.embed(request)?)
    }

    pub fn rerank(
        &self,
        selector: &str,
        request: libmir::RerankRequest,
    ) -> Result<libmir::RerankOutput> {
        let model = self.inference_model(selector)?;
        Ok(model.rerank(request)?)
    }

    pub(super) fn inference_model(&self, selector: &str) -> Result<libmir::Model> {
        let mut ignored = |_progress| {};
        Ok(self.load_model(selector, false, &mut ignored)?.model)
    }
}
