mod markdown;
mod stream;

use gloo_file::{Blob, futures::read_as_data_url};
use leptos::prelude::*;
use serde::Serialize;
use wasm_bindgen_futures::spawn_local;
use web_sys::{DragEvent, File};
use self::{markdown::Markdown, stream::run_generation};
use crate::{
    api,
    components::Icon,
    state::{RuntimeState, number},
    types::ChatMessage,
};

#[derive(Clone, Default)]
pub(super) struct UiMessage {
    role: String,
    content: String,
    reasoning: String,
    thinking: bool,
}

#[derive(Clone, Default)]
pub(super) struct Attachment {
    name: String,
    size: f64,
    data_url: String,
}
#[derive(Clone, Copy)]
pub(super) struct ChatState {
    messages: RwSignal<Vec<UiMessage>>,
    running: RwSignal<bool>,
    operation_id: RwSignal<Option<String>>,
    attachment: RwSignal<Option<Attachment>>,
    status: RwSignal<String>,
    ttft: RwSignal<Option<f64>>,
    prefill: RwSignal<Option<f64>>,
    decode: RwSignal<Option<f64>>,
    throughput: RwSignal<Option<f64>>,
}

#[derive(Serialize)]
pub(super) struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: Option<u64>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<u64>,
    repetition_penalty: Option<f32>,
    seed: Option<u64>,
    image: Option<String>,
}

#[component]
pub fn ChatPage() -> impl IntoView {
    let chat = ChatState {
        messages: RwSignal::new(Vec::new()),
        running: RwSignal::new(false),
        operation_id: RwSignal::new(None),
        attachment: RwSignal::new(None),
        status: RwSignal::new("idle".to_owned()),
        ttft: RwSignal::new(None),
        prefill: RwSignal::new(None),
        decode: RwSignal::new(None),
        throughput: RwSignal::new(None),
    };
    provide_context(chat);
    let selected_model = RwSignal::new(String::new());
    view! { <section class="view active" id="chat">
        <Conversation />
        <Composer selected_model />
    </section> }
}

#[component]
fn Conversation() -> impl IntoView {
    let chat = expect_context::<ChatState>();
    let log = NodeRef::<leptos::html::Div>::new();
    let following = RwSignal::new(true);
    Effect::new(move || {
        chat.messages.track();
        if following.get_untracked()
            && let Some(log) = log.get()
        {
            log.set_scroll_top(log.scroll_height());
        }
    });
    let indices = move || (0..chat.messages.get().len()).collect::<Vec<_>>();
    view! { <div class="chat-conversation" node_ref=log on:scroll=move |event| { let element = event_target::<web_sys::HtmlElement>(&event); following.set(element.scroll_height() - element.scroll_top() - element.client_height() < 72); }>
        <Show when=move || !chat.messages.get().is_empty() fallback=|| view! { <p class="chat-empty"><strong>"Start a conversation"</strong><span>"Choose a loaded model below and send a message."</span></p> }>
            <For each=indices key=|index| *index children=move |index| view! { <MessageView index /> } />
        </Show>
    </div> }
}

#[component]
fn MessageView(index: usize) -> impl IntoView {
    let chat = expect_context::<ChatState>();
    let message = move || chat.messages.get().get(index).cloned().unwrap_or_default();
    let reasoning = Signal::derive(move || message().reasoning);
    let content = Signal::derive(move || message().content);
    view! { <article class=move || { let value = message(); format!("chat-message {}{}", value.role, if value.thinking { " is-thinking" } else { "" }) }><span class="role">{move || message().role}</span><div class="chat-bubble"><Show when=move || { let value = message(); !value.reasoning.is_empty() || value.thinking }><details class="reasoning" open=false><summary><span class="thinking-label">{move || if message().thinking { "Thinking…" } else { "Thinking" }}</span></summary><div class="reasoning-content"><Markdown source=reasoning /></div></details></Show><div class="message-content"><Markdown source=content /></div></div></article> }
}

#[component]
fn Composer(selected_model: RwSignal<String>) -> impl IntoView {
    let runtime = expect_context::<RuntimeState>();
    let chat = expect_context::<ChatState>();
    let prompt = RwSignal::new(String::new());
    let max_tokens = RwSignal::new(String::new());
    let temperature = RwSignal::new(String::new());
    let top_p = RwSignal::new(String::new());
    let top_k = RwSignal::new(String::new());
    let repetition = RwSignal::new(String::new());
    let seed = RwSignal::new(String::new());
    let submit = move || {
        let text = prompt.get_untracked().trim().to_owned();
        if text.is_empty() || chat.running.get_untracked() {
            return;
        }
        prompt.set(String::new());
        run_generation(
            runtime,
            chat,
            selected_model.get_untracked(),
            text,
            Parameters {
                max_tokens,
                temperature,
                top_p,
                top_k,
                repetition,
                seed,
            },
        );
    };
    let loaded_models = move || {
        runtime
            .models
            .get()
            .into_iter()
            .filter(|model| matches!(model.state.as_str(), "ready" | "active"))
            .collect::<Vec<_>>()
    };
    view! { <div class="composer-dock"><form class="chat-composer" on:submit=move |event| { event.prevent_default(); submit(); } on:dragover=move |event: DragEvent| event.prevent_default() on:drop=move |event: DragEvent| { event.prevent_default(); if let Some(file) = event.data_transfer().and_then(|data| data.files()).and_then(|files| files.get(0)) { attach(runtime, chat, file); } }>
        <AttachmentChip />
        <div class="prompt-row"><label class="composer-button attach-button" data-tooltip="Add attachment" aria-label="Add attachment"><Icon name="add" /><input type="file" accept="image/png,image/jpeg,image/webp,image/gif" hidden on:change=move |event| { if let Some(file) = event_target::<web_sys::HtmlInputElement>(&event).files().and_then(|files| files.get(0)) { attach(runtime, chat, file); } } /></label><textarea rows="2" placeholder="Message MiRMiR…" prop:value=move || prompt.get() on:input=move |event| prompt.set(event_target_value(&event)) on:keydown=move |event| { if event.key() == "Enter" && !event.shift_key() { event.prevent_default(); submit(); } } disabled=move || chat.running.get()></textarea><Show when=move || chat.running.get()><button class="composer-button cancel-button" type="button" data-tooltip="Stop generation" aria-label="Stop generation" on:click=move |_| cancel_generation(runtime, chat)>"■"</button></Show><button class="composer-button send-button" type="submit" data-tooltip="Send message" aria-label="Send message" disabled=move || chat.running.get()>"↑"</button></div>
        <div class="composer-footer"><label class="model-picker"><span>"Model"</span><select on:change=move |event| selected_model.set(event_target_value(&event))><option value="">"Select model"</option><For each=loaded_models key=|model| model.selector.clone() children=move |model| { let selector = model.selector.clone(); view! { <option value=selector>{model.id}</option> } } /></select></label><details class="chat-settings"><summary>"Parameters"</summary><div class="chat-parameters"><Parameter label="Max tokens" value=max_tokens /><Parameter label="Temperature" value=temperature /><Parameter label="Top P" value=top_p /><Parameter label="Top K" value=top_k /><Parameter label="Repetition" value=repetition /><Parameter label="Seed" value=seed /></div></details><div class="composer-metrics"><Metric label="TTFT" value=chat.ttft unit="ms" /><Metric label="PREFILL" value=chat.prefill unit="tok/s" /><Metric label="DECODE" value=chat.decode unit="tok/s" /><Metric label="E2E" value=chat.throughput unit="tok/s" /></div><span class="stage">{move || chat.status.get()}</span></div>
        <p class="chat-hint">"Enter sends · Shift+Enter adds a line"</p>
    </form></div> }
}

#[derive(Clone, Copy)]
pub(super) struct Parameters {
    max_tokens: RwSignal<String>,
    temperature: RwSignal<String>,
    top_p: RwSignal<String>,
    top_k: RwSignal<String>,
    repetition: RwSignal<String>,
    seed: RwSignal<String>,
}
impl Parameters {
    pub fn request(self) -> ParsedParameters {
        ParsedParameters {
            max_tokens: parse(self.max_tokens),
            temperature: parse(self.temperature),
            top_p: parse(self.top_p),
            top_k: parse(self.top_k),
            repetition_penalty: parse(self.repetition),
            seed: parse(self.seed),
        }
    }
}
pub(super) struct ParsedParameters {
    max_tokens: Option<u64>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<u64>,
    repetition_penalty: Option<f32>,
    seed: Option<u64>,
}
fn parse<T: std::str::FromStr>(signal: RwSignal<String>) -> Option<T> {
    signal.get_untracked().parse().ok()
}

#[component]
fn Parameter(label: &'static str, value: RwSignal<String>) -> impl IntoView {
    view! { <label>{label}<input type="number" prop:value=move || value.get() on:input=move |event| value.set(event_target_value(&event)) /></label> }
}
#[component]
fn Metric(label: &'static str, value: RwSignal<Option<f64>>, unit: &'static str) -> impl IntoView {
    view! { <span>{label}<b>{move || number(value.get())}</b><i>{unit}</i></span> }
}
#[component]
fn AttachmentChip() -> impl IntoView {
    let chat = expect_context::<ChatState>();
    view! { <Show when=move || chat.attachment.get().is_some()>{move || chat.attachment.get().map(|item| view! { <div class="chat-attachment"><span class="file-icon"></span><div><strong>{item.name}</strong><span>{format_file_size(item.size)}</span></div><button class="attachment-remove" type="button" data-tooltip="Remove attachment" aria-label="Remove attachment" on:click=move |_| chat.attachment.set(None)>"×"</button></div> })}</Show> }
}

fn attach(runtime: RuntimeState, chat: ChatState, file: File) {
    if file.size() > 20.0 * 1024.0 * 1024.0 {
        runtime.notify("Image exceeds the 20 MiB limit", true);
        return;
    }
    if !matches!(file.type_().as_str(), "image/png" | "image/jpeg" | "image/webp" | "image/gif") {
        runtime.notify("Unsupported image type", true);
        return;
    }
    spawn_local(async move {
        let name = file.name();
        let size = file.size();
        match read_as_data_url(&Blob::from(file)).await {
            Ok(data_url) => chat.attachment.set(Some(Attachment { name, size, data_url })),
            Err(error) => runtime.notify(error.to_string(), true),
        }
    });
}

fn format_file_size(mut value: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.precision$} {}", UNITS[unit], precision = usize::from(unit > 1))
}

fn cancel_generation(runtime: RuntimeState, chat: ChatState) {
    let Some(operation_id) = chat.operation_id.get_untracked() else {
        return;
    };
    spawn_local(async move {
        let body = serde_json::json!({ "operation_id": operation_id });
        match api::post::<serde_json::Value, _>(runtime, "/activity/cancel", &body).await {
            Ok(response) => {
                let status = if response["accepted"].as_bool().unwrap_or(false) {
                    "cancelling"
                } else {
                    response["state"].as_str().unwrap_or("running")
                };
                chat.status.set(status.to_owned());
            },
            Err(error) => runtime.notify(error, true),
        }
    });
}
