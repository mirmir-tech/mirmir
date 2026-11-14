use libmir::{ModelDescriptor, ModelTask, PoolingMode, TaskExecutionPlan};

use crate::application::{EmbeddingCapabilities, ModelTaskCapabilities, RerankCapabilities};

pub(super) fn capabilities(descriptor: &ModelDescriptor) -> ModelTaskCapabilities {
    let max_input_tokens = descriptor
        .tokenizer()
        .default_max_length()
        .unwrap_or_else(|| descriptor.metadata().context_len)
        .min(descriptor.metadata().context_len);
    let (embedding, rerank) = match descriptor.task_plan() {
        TaskExecutionPlan::Embedding { task, .. } => (
            Some(EmbeddingCapabilities {
                native_dimensions: u64::try_from(task.native_dimensions).unwrap_or(u64::MAX),
                pooling: pooling(task.pooling),
                normalized: task.normalize,
                prompt_names: task.prompts.keys().cloned().collect(),
                default_prompt: task.default_prompt.clone(),
                includes_prompt: task.include_prompt,
            }),
            None,
        ),
        TaskExecutionPlan::SequenceScoring { task, .. } => (
            None,
            Some(RerankCapabilities {
                labels: u64::try_from(task.labels).unwrap_or(u64::MAX),
                pooling: pooling(task.pooling),
                raw_scores: true,
            }),
        ),
        TaskExecutionPlan::CausalScoring { .. } => (
            None,
            Some(RerankCapabilities {
                labels: 1,
                pooling: "last_token",
                raw_scores: true,
            }),
        ),
        TaskExecutionPlan::Generation { .. } => (None, None),
    };
    ModelTaskCapabilities {
        max_input_tokens: u64::try_from(max_input_tokens).unwrap_or(u64::MAX),
        embedding,
        rerank,
    }
}

const fn pooling(mode: PoolingMode) -> &'static str {
    match mode {
        PoolingMode::Cls => "cls",
        PoolingMode::LastToken => "last_token",
        PoolingMode::Mean => "mean",
    }
}

pub(super) const fn task_name(task: &ModelTask) -> &'static str {
    match task {
        ModelTask::Generation => "generation",
        ModelTask::CausalScoring(_) | ModelTask::SequenceScoring(_) => "rerank",
        ModelTask::Embedding(_) => "embedding",
    }
}
