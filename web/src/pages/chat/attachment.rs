use gloo_file::{Blob, futures::read_as_data_url};
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::File;

use super::ChatState;
use crate::state::RuntimeState;

#[derive(Clone, Default)]
pub(super) struct Attachment {
    pub(super) name: String,
    pub(super) size: f64,
    pub(super) data_url: String,
}

#[component]
pub(super) fn AttachmentChip() -> impl IntoView {
    let chat = expect_context::<ChatState>();
    view! { <Show when=move || chat.attachment.get().is_some()>{move || chat.attachment.get().map(|item| view! { <div class="chat-attachment"><span class="file-icon"></span><div><strong>{item.name}</strong><span>{format_file_size(item.size)}</span></div><button class="attachment-remove" type="button" data-tooltip="Remove attachment" aria-label="Remove attachment" on:click=move |_| chat.attachment.set(None)>"×"</button></div> })}</Show> }
}

pub(super) fn attach(runtime: RuntimeState, chat: ChatState, file: File) {
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
