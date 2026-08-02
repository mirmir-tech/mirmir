use futures_util::StreamExt;
use gloo_net::websocket::{Message, futures::WebSocket};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::{
    api,
    state::RuntimeState,
    types::{
        Activity, Bootstrap, History, HistorySample, Models, Overview, ServerMessage,
        TelemetryPoint,
    },
};

const SCHEMA_VERSION: u32 = 7;

pub fn connect(state: RuntimeState) {
    spawn_local(async move {
        let mut connected_once = false;
        loop {
            state.connection.set("connecting".to_owned());
            match establish(state).await {
                Ok(socket) => {
                    state.connection.set("connected".to_owned());
                    if connected_once {
                        state.notify("Connection restored", false);
                    }
                    connected_once = true;
                    consume(state, socket).await;
                    state.connection.set("disconnected".to_owned());
                    state.notify("Connection lost. Reconnecting…", true);
                },
                Err(error) => {
                    state.connection.set("disconnected".to_owned());
                    if connected_once {
                        state.notify(error, true);
                    }
                },
            }
            TimeoutFuture::new(1_000).await;
        }
    });
}

async fn establish(state: RuntimeState) -> Result<WebSocket, String> {
    let bootstrap: Bootstrap = api::get("/bootstrap").await?;
    if bootstrap.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "Dashboard/server mismatch: UI schema {SCHEMA_VERSION}, server schema {}",
            bootstrap.schema_version
        ));
    }
    state.server_version.set(bootstrap.server_version);
    state.protocol_version.set(bootstrap.protocol_version);
    let session: serde_json::Value = api::post_empty("/session").await?;
    let csrf = session["csrf_token"]
        .as_str()
        .ok_or_else(|| "session response omitted CSRF token".to_owned())?;
    state.csrf.set(csrf.to_owned());
    if let Ok(history) = api::get::<History>("/telemetry/history?limit=900").await {
        state.telemetry.set(history.samples.iter().map(TelemetryPoint::from).collect());
    }
    crate::result::string(WebSocket::open(&socket_url()?))
}

async fn consume(state: RuntimeState, mut socket: WebSocket) {
    while let Some(message) = socket.next().await {
        let Ok(Message::Text(text)) = message else {
            continue;
        };
        match serde_json::from_str::<ServerMessage>(&text) {
            Ok(message) => apply(state, message),
            Err(error) => state.notify(format!("Invalid runtime update: {error}"), true),
        }
    }
}

fn apply(state: RuntimeState, message: ServerMessage) {
    match message {
        ServerMessage::Startup { startup } if startup.phase == "failed" => {
            state.notify(startup.detail, true);
        },
        ServerMessage::Startup { .. } => {},
        ServerMessage::Overview { overview } => {
            state.telemetry.update(|points| {
                points.push(TelemetryPoint::from(overview.as_ref()));
                points.sort_by_key(|point| point.sampled_at_unix_ms);
                points.dedup_by_key(|point| point.sampled_at_unix_ms);
                if points.len() > 900 {
                    points.drain(..points.len() - 900);
                }
            });
            state.overview.set(Some(*overview));
        },
        ServerMessage::Models { models } => {
            apply_models(state, models);
        },
        ServerMessage::Configuration { configuration } => {
            state.configuration.set(Some(configuration));
        },
        ServerMessage::Activity { activity } => {
            let refresh = model_activity(&activity) && terminal(&activity.state);
            state.apply_activity(&activity);
            if refresh {
                spawn_local(refresh_models(state));
            }
        },
        ServerMessage::Error { message } => state.notify(message, true),
    }
}

fn apply_models(state: RuntimeState, models: Models) {
    state.models.set(models.models);
    state.models_loaded.set(true);
}

async fn refresh_models(state: RuntimeState) {
    match api::get::<Models>("/models").await {
        Ok(models) => apply_models(state, models),
        Err(error) => state.notify(format!("Could not refresh models: {error}"), true),
    }
}

fn model_activity(activity: &Activity) -> bool {
    matches!(activity.kind.as_str(), "load" | "restore" | "unload" | "pull" | "remove")
}

fn terminal(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}

fn socket_url() -> Result<String, String> {
    let location = web_sys::window()
        .ok_or_else(|| "browser window unavailable".to_owned())?
        .location();
    let Ok(protocol) = location.protocol() else {
        return Err("location protocol unavailable".to_owned());
    };
    let Ok(host) = location.host() else {
        return Err("location host unavailable".to_owned());
    };
    let scheme = if protocol == "https:" {
        "wss"
    } else {
        "ws"
    };
    Ok(format!("{scheme}://{host}/api/mirmir/v1/ws"))
}

impl From<&HistorySample> for TelemetryPoint {
    fn from(sample: &HistorySample) -> Self {
        let memory_percent =
            usage_percent(sample.memory_total_bytes, sample.memory_available_bytes);
        Self {
            sampled_at_unix_ms: sample.sampled_at_unix_ms,
            memory_percent,
            gpu_percent: sample.gpu_utilization_percent,
            temperature_celsius: sample.device_temperature_celsius,
            power_watts: sample.device_power_watts,
            power_limit_watts: sample.device_power_limit_watts,
        }
    }
}

impl From<&Overview> for TelemetryPoint {
    fn from(overview: &Overview) -> Self {
        Self {
            sampled_at_unix_ms: overview.sampled_at_unix_ms,
            memory_percent: usage_percent(
                overview.host_total_memory_bytes,
                overview.host_available_memory_bytes,
            ),
            gpu_percent: overview.gpu_utilization_percent,
            temperature_celsius: overview.device_temperature_celsius,
            power_watts: overview.device_power_watts,
            power_limit_watts: overview.device_power_limit_watts,
        }
    }
}

fn usage_percent(total: Option<u64>, available: Option<u64>) -> Option<f64> {
    total
        .zip(available)
        .and_then(|(total, available)| percent(total.saturating_sub(available), total))
}

fn percent(used: u64, total: u64) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some(100.0 * used as f64 / total as f64)
    }
}
