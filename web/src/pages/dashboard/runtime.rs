use leptos::prelude::*;

use super::{
    chart::{Chart, Metric},
    format::{memory_label, uptime},
};
use crate::state::{RuntimeState, number};

#[component]
pub fn RuntimeInspector() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! { <aside class="runtime-inspector" aria-label="Runtime details">
        <h2>"Runtime"</h2>
        <dl>
            <div><dt>"Platform"</dt><dd>{move || platform(state)}</dd></div>
            <div><dt>"Device"</dt><dd>{move || state.overview.get().map_or_else(|| "Waiting for runtime".to_owned(), |v| v.device_name)}</dd></div>
            <div><dt>"Backend support"</dt><dd>{move || state.server.get().backend_support}</dd></div>
            <div><dt>"Server"</dt><dd>{move || state.server.get().server_version}</dd></div>
            <div><dt>"Connection"</dt><dd>{move || state.connection.get()}</dd></div>
        </dl>
        <div class="runtime-memory"><span>"Host memory"</span><strong>{move || memory_label(state)}</strong>
            <Show when=move || memory_percent(state).is_some()>
                <progress max="100" value=move || memory_percent(state).unwrap_or_default() aria-label="Host memory used" />
            </Show>
            <small>{move || state.overview.get().map_or_else(String::new, |v| v.memory_source)}</small>
        </div>
        <details><summary>"Device details"</summary><dl>
            <div><dt>"Protocol"</dt><dd>{move || state.server.get().protocol_version}</dd></div>
            <div><dt>"GPU usage"</dt><dd>{move || number(state.overview.get().and_then(|v| v.gpu_utilization_percent))}" %"</dd></div>
            <div><dt>"Temperature"</dt><dd>{move || number(state.overview.get().and_then(|v| v.device_temperature_celsius))}" °C"</dd></div>
            <div><dt>"Power"</dt><dd>{move || number(state.overview.get().and_then(|v| v.device_power_watts))}" W"</dd></div>
            <div><dt>"Uptime"</dt><dd>{move || uptime(state)}</dd></div>
        </dl></details>
    </aside> }
}

fn platform(state: RuntimeState) -> String {
    let platform = state.server.get().platform;
    if platform.is_empty() {
        return "Not reported by server".to_owned();
    }
    format!("{} · {}", platform, state.server.get().architecture)
}

fn memory_percent(state: RuntimeState) -> Option<f64> {
    let overview = state.overview.get()?;
    let total = overview.host_total_memory_bytes.filter(|total| *total > 0)?;
    Some(100.0 * total.saturating_sub(overview.host_available_memory_bytes?) as f64 / total as f64)
}

#[component]
pub fn RuntimeActivity() -> impl IntoView {
    view! { <section class="runtime-activity" aria-label="Runtime activity">
        <header><h3>"Runtime activity"</h3><span>"Recent measurements · tok/s"</span></header>
        <div class="rate-charts"><div><h4>"Prefill"</h4><Chart metric=Metric::Prefill label="Prefill tokens per second" /></div>
        <div><h4>"Decode"</h4><Chart metric=Metric::Decode label="Decode tokens per second" /></div></div>
    </section> }
}
