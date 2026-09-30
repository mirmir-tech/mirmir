use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{ChatState, UiMessage};
use crate::components::Icon;

const STORAGE_KEY: &str = "mirmir.chat.conversations.v1";
const HISTORY_LIMIT: usize = 30;
const TITLE_LENGTH: usize = 48;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct SavedConversation {
    id: u64,
    title: String,
    messages: Vec<UiMessage>,
}

impl SavedConversation {
    /// Keeps only settled messages: a reply that never produced text is
    /// dropped.
    pub(super) fn new(id: u64, messages: Vec<UiMessage>) -> Self {
        let messages = messages
            .into_iter()
            .filter(|message| {
                message.role != "assistant"
                    || !message.content.is_empty()
                    || !message.reasoning.is_empty()
            })
            .map(|mut message| {
                message.thinking = false;
                message
            })
            .collect::<Vec<_>>();
        Self { id, title: title(&messages), messages }
    }
}

/// The saved history, the conversation to reopen, and its messages.
///
/// History is kept in this browser's local storage; a server-backed store can
/// replace `restore` and `store` without changing the chat view.
#[cfg_attr(feature = "capture", allow(dead_code))]
pub(super) fn restore() -> (Vec<SavedConversation>, u64, Vec<UiMessage>) {
    let history: Vec<SavedConversation> = storage()
        .and_then(|storage| storage.get_item(STORAGE_KEY).ok().flatten())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    let (conversation, messages) = history
        .first()
        .map_or_else(|| (1, Vec::new()), |latest| (latest.id, latest.messages.clone()));
    (history, conversation, messages)
}

fn store(history: &[SavedConversation]) {
    if cfg!(feature = "capture") {
        return;
    }
    if let (Some(storage), Ok(raw)) = (storage(), serde_json::to_string(history)) {
        storage.set_item(STORAGE_KEY, &raw).ok();
    }
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn title(messages: &[UiMessage]) -> String {
    let Some(prompt) = messages.iter().find(|message| message.role == "user") else {
        return "New chat".to_owned();
    };
    let text = prompt.content.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > TITLE_LENGTH {
        format!("{}…", text.chars().take(TITLE_LENGTH).collect::<String>().trim_end())
    } else {
        text
    }
}

/// Records the open conversation whenever a generation settles.
pub(super) fn persist(chat: ChatState) {
    Effect::new(move || {
        if chat.running.get() {
            return;
        }
        let messages = chat.messages.get();
        if messages.is_empty() {
            return;
        }
        let id = chat.conversation.get_untracked();
        let saved = SavedConversation::new(id, messages);
        let unchanged = chat.history.with_untracked(|history| {
            history.iter().any(|item| item.id == id && item.messages == saved.messages)
        });
        if unchanged {
            return;
        }
        chat.history.update(|history| {
            history.retain(|item| item.id != id);
            history.insert(0, saved);
            history.truncate(HISTORY_LIMIT);
            store(history);
        });
    });
}

fn start_new(chat: ChatState) {
    let next = chat
        .history
        .get_untracked()
        .iter()
        .map(|item| item.id)
        .chain([chat.conversation.get_untracked()])
        .max()
        .map_or(1, |id| id + 1);
    chat.conversation.set(next);
    show(chat, Vec::new());
}

fn open(chat: ChatState, id: u64) {
    let messages = chat
        .history
        .get_untracked()
        .into_iter()
        .find(|item| item.id == id)
        .map(|item| item.messages)
        .unwrap_or_default();
    chat.conversation.set(id);
    show(chat, messages);
}

fn show(chat: ChatState, messages: Vec<UiMessage>) {
    chat.messages.set(messages);
    chat.status.set("idle".to_owned());
    chat.ttft.set(None);
    chat.prefill.set(None);
    chat.decode.set(None);
    chat.throughput.set(None);
}

fn remove(chat: ChatState, id: u64) {
    chat.history.update(|history| {
        history.retain(|item| item.id != id);
        store(history);
    });
    if chat.conversation.get_untracked() == id {
        start_new(chat);
    }
}

#[component]
pub(super) fn HistoryPanel() -> impl IntoView {
    let chat = expect_context::<ChatState>();
    view! {
        <aside class="chat-history" aria-label="Conversations">
            <button type="button" class="chat-new" disabled=move || chat.running.get() on:click=move |_| start_new(chat)>
                <Icon name="add" /><span>"New chat"</span>
            </button>
            <p class="chat-history-label">"Recent"</p>
            <ul>
                <For
                    each=move || chat.history.get()
                    key=|item| (item.id, item.title.clone(), item.messages.len())
                    children=move |item| {
                        let id = item.id;
                        let label = format!("Delete conversation: {}", item.title);
                        view! {
                            <li class:current=move || chat.conversation.get() == id>
                                <button type="button" class="chat-history-item" disabled=move || chat.running.get() on:click=move |_| open(chat, id)>{item.title}</button>
                                <button type="button" class="chat-history-remove" aria-label=label disabled=move || chat.running.get() on:click=move |_| remove(chat, id)><Icon name="cancel" /></button>
                            </li>
                        }
                    }
                />
            </ul>
            <Show when=move || chat.history.get().is_empty()>
                <p class="chat-history-empty">"Conversations are kept in this browser."</p>
            </Show>
        </aside>
    }
}
