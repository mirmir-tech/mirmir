use std::collections::HashMap;

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::types::{Activity, Configuration, Model, Overview, TelemetryPoint};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Models,
    Chat,
    Configuration,
}

impl Page {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Overview => "Dashboard",
            Self::Models => "Models",
            Self::Chat => "Chat",
            Self::Configuration => "Settings",
        }
    }
}

#[derive(Clone)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub error: bool,
}

#[derive(Clone, Copy)]
pub struct RuntimeState {
    pub page: RwSignal<Page>,
    pub connection: RwSignal<String>,
    pub server_version: RwSignal<String>,
    pub protocol_version: RwSignal<String>,
    pub csrf: RwSignal<String>,
    pub overview: RwSignal<Option<Overview>>,
    pub models: RwSignal<Vec<Model>>,
    pub models_loaded: RwSignal<bool>,
    pub configuration: RwSignal<Option<Configuration>>,
    pub activities: RwSignal<Vec<Activity>>,
    pub telemetry: RwSignal<Vec<TelemetryPoint>>,
    pub busy: RwSignal<HashMap<String, String>>,
    pub toasts: RwSignal<Vec<Toast>>,
    next_toast: RwSignal<u64>,
}

impl RuntimeState {
    pub fn new() -> Self {
        Self {
            page: RwSignal::new(Page::Overview),
            connection: RwSignal::new("connecting".to_owned()),
            server_version: RwSignal::new(String::new()),
            protocol_version: RwSignal::new(String::new()),
            csrf: RwSignal::new(String::new()),
            overview: RwSignal::new(None),
            models: RwSignal::new(Vec::new()),
            models_loaded: RwSignal::new(false),
            configuration: RwSignal::new(None),
            activities: RwSignal::new(Vec::new()),
            telemetry: RwSignal::new(Vec::new()),
            busy: RwSignal::new(HashMap::new()),
            toasts: RwSignal::new(Vec::new()),
            next_toast: RwSignal::new(0),
        }
    }

    pub fn notify(self, message: impl Into<String>, error: bool) {
        let id = self.next_toast.get_untracked().saturating_add(1);
        self.next_toast.set(id);
        self.toasts.update(|items| {
            items.push(Toast { id, message: message.into(), error });
            if items.len() > 4 {
                items.remove(0);
            }
        });
        spawn_local(async move {
            TimeoutFuture::new(if error {
                8_000
            } else {
                5_000
            })
            .await;
            self.dismiss(id);
        });
    }

    pub fn dismiss(self, id: u64) {
        self.toasts.update(|items| items.retain(|toast| toast.id != id));
    }

    pub fn set_busy(self, target: &str, operation: Option<&str>) {
        self.busy.update(|busy| {
            if let Some(operation) = operation {
                busy.insert(target.to_owned(), operation.to_owned());
            } else {
                busy.remove(target);
            }
        });
    }

    pub fn apply_activity(self, activity: &Activity) {
        self.activities.update(|items| {
            items.retain(|item| item.operation_id != activity.operation_id);
            items.push(activity.clone());
            items.sort_by_key(|item| std::cmp::Reverse(item.updated_at_unix_ms));
            items.truncate(80);
        });
        let live = matches!(activity.state.as_str(), "queued" | "running" | "cancelling");
        self.set_busy(&activity.target, live.then_some(activity.kind.as_str()));
        if activity.state == "failed" {
            self.notify(format!("{}: {}", activity.target, activity.detail), true);
        }
    }
}

pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.precision$} {}", UNITS[unit], precision = usize::from(unit > 1))
}

pub fn number(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.1}"))
}
