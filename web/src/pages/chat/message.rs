use leptos::prelude::*;

use super::{ChatState, markdown::Markdown};
use crate::state::number;

#[component]
pub(super) fn MessageView(index: usize) -> impl IntoView {
    let chat = expect_context::<ChatState>();
    let message = move || chat.messages.get().get(index).cloned().unwrap_or_default();
    let reasoning = Signal::derive(move || message().reasoning);
    let content = Signal::derive(move || message().content);
    let assistant = move || message().role == "assistant";
    let latest = move || chat.messages.get().len() == index + 1;
    let show_metrics = move || {
        assistant()
            && latest()
            && !message().thinking
            && !chat.running.get()
            && chat.decode.get().is_some()
    };
    view! {
        <article class=move || { let value = message(); format!("chat-message {}{}", value.role, if value.thinking { " is-thinking" } else { "" }) }>
            <Show when=assistant fallback=|| view! { <span class="visually-hidden">"You"</span> }>
                <header class="message-author">
                    <span class="message-mark" aria-hidden="true"><svg viewBox="0 0 60 64"><path d="M8 52V12L42 46" /><path d="M52 52V12L18 46" /></svg></span>
                    <strong>{move || model_name(&message().model)}</strong>
                </header>
            </Show>
            <div class="chat-bubble">
                <Show when=move || { let value = message(); !value.reasoning.is_empty() || value.thinking }>
                    <details class="reasoning" open=false><summary><span class="thinking-label">{move || if message().thinking { "Thinking…" } else { "Thinking" }}</span></summary><div class="reasoning-content"><Markdown source=reasoning /></div></details>
                </Show>
                <div class="message-content"><Markdown source=content /></div>
            </div>
            <Show when=show_metrics>
                <footer class="message-metrics" aria-label="Generation metrics">
                    <span>{move || format!("TTFT {} ms", number(chat.ttft.get()))}</span>
                    <span>{move || format!("Prefill {} tok/s", number(chat.prefill.get()))}</span>
                    <span>{move || format!("Decode {} tok/s", number(chat.decode.get()))}</span>
                    <span>{move || format!("End to end {} tok/s", number(chat.throughput.get()))}</span>
                </footer>
            </Show>
        </article>
    }
}

/// The last path segment of a model selector, such as `Qwen3-4B` for
/// `Qwen/Qwen3-4B`.
fn model_name(selector: &str) -> String {
    selector
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("Assistant")
        .to_owned()
}
