//! Per-request generation parameters; unset ones use the model's defaults.

use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    api,
    components::{
        RangeParameter,
        parameter::{
            Range,
            range::{
                REPETITION_PENALTY, TEMPERATURE, TOP_K, TOP_P, max_tokens_limit, single, whole,
            },
        },
    },
    types::catalog::Inspection,
};

#[derive(Clone, Copy)]
pub(super) struct Parameters {
    max_tokens: RwSignal<Option<f64>>,
    temperature: RwSignal<Option<f64>>,
    top_p: RwSignal<Option<f64>>,
    top_k: RwSignal<Option<f64>>,
    repetition: RwSignal<Option<f64>>,
    seed: RwSignal<String>,
    /// The selected model's input limit, once inspected.
    max_input: RwSignal<Option<u64>>,
}

pub(super) struct ParsedParameters {
    pub max_tokens: Option<u64>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u64>,
    pub repetition_penalty: Option<f32>,
    pub seed: Option<u64>,
}

impl Parameters {
    pub fn new() -> Self {
        Self {
            max_tokens: RwSignal::new(None),
            temperature: RwSignal::new(None),
            top_p: RwSignal::new(None),
            top_k: RwSignal::new(None),
            repetition: RwSignal::new(None),
            seed: RwSignal::new(String::new()),
            max_input: RwSignal::new(None),
        }
    }

    pub fn request(self) -> ParsedParameters {
        let value = |signal: RwSignal<Option<f64>>, range: Range| {
            signal.get_untracked().map(|value| range.clamp(value))
        };
        ParsedParameters {
            max_tokens: value(self.max_tokens, self.token_range()).map(whole),
            temperature: value(self.temperature, TEMPERATURE).map(single),
            top_p: value(self.top_p, TOP_P).map(single),
            top_k: value(self.top_k, TOP_K).map(whole),
            repetition_penalty: value(self.repetition, REPETITION_PENALTY).map(single),
            seed: self.seed.get_untracked().trim().parse().ok(),
        }
    }

    fn token_range(self) -> Range {
        Range::max_tokens(max_tokens_limit(None, self.max_input.get()))
    }

    /// Follows the selected model's input limit.
    pub fn track_model(self, model: RwSignal<String>) {
        Effect::new(move || {
            let selector = model.get();
            self.max_input.set(None);
            if selector.is_empty() {
                return;
            }
            spawn_local(async move {
                let path = format!("/models/inspect?selector={}", urlencoding::encode(&selector));
                let limit = api::get::<Inspection>(&path).await.ok().and_then(|item| {
                    item.capabilities.map(|capabilities| capabilities.max_input_tokens)
                });
                if model.get_untracked() == selector {
                    self.max_input.set(limit);
                }
            });
        });
    }
}

#[component]
pub(super) fn ParameterPanel(parameters: Parameters) -> impl IntoView {
    let set = |signal: RwSignal<Option<f64>>| move |value| signal.set(value);
    let tokens = Signal::derive(move || parameters.token_range());
    view! { <details class="chat-settings"><summary>"Parameters"</summary><div class="chat-parameters">
        <RangeParameter label="Max tokens" range=tokens value=parameters.max_tokens on_change=set(parameters.max_tokens) />
        <RangeParameter label="Temperature" range=TEMPERATURE value=parameters.temperature on_change=set(parameters.temperature) />
        <RangeParameter label="Top P" range=TOP_P value=parameters.top_p on_change=set(parameters.top_p) />
        <RangeParameter label="Top K" range=TOP_K value=parameters.top_k on_change=set(parameters.top_k) />
        <RangeParameter label="Repetition" range=REPETITION_PENALTY value=parameters.repetition on_change=set(parameters.repetition) />
        <label class="chat-seed">"Seed"<input type="number" prop:value=move || parameters.seed.get() on:input=move |event| parameters.seed.set(event_target_value(&event)) /></label>
    </div></details> }
}
