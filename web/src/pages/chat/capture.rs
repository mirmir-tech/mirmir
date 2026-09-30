use leptos::prelude::*;

use super::{ChatState, UiMessage, history::SavedConversation};

/// Deterministic chat state for documentation captures.
pub(super) fn state() -> ChatState {
    ChatState {
        messages: RwSignal::new(conversation()),
        history: RwSignal::new(vec![
            SavedConversation::new(1, conversation()),
            saved(2, "Tool schema for a weather lookup"),
            saved(3, "Rerank support tickets by urgency"),
            saved(4, "Summarize the 0.4.0 release notes"),
        ]),
        conversation: RwSignal::new(1),
        running: RwSignal::new(false),
        operation_id: RwSignal::new(None),
        attachment: RwSignal::new(None),
        status: RwSignal::new("complete".to_owned()),
        ttft: RwSignal::new(Some(184.0)),
        prefill: RwSignal::new(Some(611.7)),
        decode: RwSignal::new(Some(42.6)),
        throughput: RwSignal::new(Some(39.8)),
    }
}

fn saved(id: u64, prompt: &str) -> SavedConversation {
    SavedConversation::new(
        id,
        vec![UiMessage {
            role: "user".to_owned(),
            content: prompt.to_owned(),
            ..Default::default()
        }],
    )
}

fn conversation() -> Vec<UiMessage> {
    vec![
            UiMessage {
                role: "user".to_owned(),
                content: "Summarize why accelerator-resident K/V cache matters for a local inference server.".to_owned(),
                ..Default::default()
            },
            UiMessage {
                role: "assistant".to_owned(),
                model: "Qwen/Qwen3-4B".to_owned(),
                reasoning: "We need to connect latency, memory bandwidth, and concurrent reuse without assuming a particular model family.".to_owned(),
                content: "Keeping the K/V cache on the accelerator avoids repeated host transfers during decode. That reduces per-token latency, preserves memory bandwidth for model execution, and lets concurrent requests reuse cached prefixes without synchronizing through the CPU.".to_owned(),
                ..Default::default()
            },
    ]
}
