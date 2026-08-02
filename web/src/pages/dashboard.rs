mod chart;
mod format;

use leptos::prelude::*;

use self::{
    chart::{Chart, Metric},
    format::{
        active, device_caption, mean, memory_label, percent_label, power_label, stage,
        temperature_label, uptime,
    },
};
use crate::{
    state::{RuntimeState, number},
    types::Overview,
};

#[component]
pub fn DashboardPage() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let rate = move |current: fn(&Overview) -> Option<f64>, last: fn(&Overview) -> Option<f64>| {
        state.overview.get().and_then(|item| {
            if item.active_requests > 0 {
                current(&item)
            } else {
                last(&item)
            }
        })
    };
    view! {
        <section class="view active" id="overview">
            <div class="dashboard-toolbar">
                <div>
                    <p class="eyebrow">"Runtime overview"</p>
                    <span class="device-caption">{move || device_caption(state)}</span>
                </div>
                <span class="overview-live" data-active=move || active(state).to_string()>
                    {move || if active(state) { "Generating" } else { "Live" }}
                </span>
            </div>
            <div class="metric-cards overview-metrics">
                <MetricCard
                    label="Prefill"
                    value=move || number(rate(|v| v.current_prefill_tokens_per_second, |v| v.last_prefill_tokens_per_second))
                    detail=move || mean(state, |v| v.mean_prefill_tokens_per_second, "tok/s")
                    unit="tok/s"
                    class="prefill"
                />
                <MetricCard
                    label="Decode"
                    value=move || number(rate(|v| v.current_decode_tokens_per_second, |v| v.last_decode_tokens_per_second))
                    detail=move || mean(state, |v| v.mean_decode_tokens_per_second, "tok/s")
                    unit="tok/s"
                    class="decode"
                />
                <MetricCard
                    label="TTFT"
                    value=move || number(state.overview.get().and_then(|v| v.current_ttft_ms.or(v.last_ttft_ms)))
                    detail=move || mean(state, |v| v.mean_ttft_ms, "ms")
                    unit="ms"
                    class="ttft"
                />
            </div>
            <div class="device-grid">
                <TelemetryPanel title="Memory" value=move || memory_label(state) class="memory-panel">
                    <Chart metric=Metric::Memory label="Memory occupancy over time" />
                </TelemetryPanel>
                <TelemetryPanel title="GPU" value=move || percent_label(state, |v| v.gpu_utilization_percent) class="gpu-panel">
                    <Chart metric=Metric::Gpu label="GPU utilization over time" />
                </TelemetryPanel>
                <TelemetryPanel title="Temperature" value=move || temperature_label(state) class="temperature-panel">
                    <Chart metric=Metric::Temperature label="Device temperature over time" />
                </TelemetryPanel>
                <TelemetryPanel title="Power" value=move || power_label(state) class="power-panel">
                    <Chart metric=Metric::Power label="Device power draw over time" />
                </TelemetryPanel>
            </div>
            <div class="overview-activity-grid">
                <article class="dashboard-panel activity-panel">
                    <header class="dashboard-panel-heading">
                        <div><p>"Operations"</p><h3>"Recent activity"</h3></div>
                        <span>{move || format!("{} events", state.activities.get().len())}</span>
                    </header>
                    <ActivityList />
                </article>
                <RuntimeSummary />
            </div>
        </section>
    }
}

#[component]
fn MetricCard<V, D>(
    label: &'static str,
    value: V,
    detail: D,
    unit: &'static str,
    class: &'static str,
) -> impl IntoView
where
    V: Fn() -> String + Send + Sync + 'static,
    D: Fn() -> String + Send + Sync + 'static,
{
    view! { <article><div><p>{label}</p><span class=format!("metric-signal {class}")></span></div><strong>{value}</strong><small>{unit}</small><em>{detail}</em></article> }
}

#[component]
fn TelemetryPanel<V>(
    title: &'static str,
    value: V,
    class: &'static str,
    children: Children,
) -> impl IntoView
where
    V: Fn() -> String + Send + Sync + 'static,
{
    view! { <article class=format!("dashboard-panel telemetry-panel {class}")>
        <header class="dashboard-panel-heading"><div><p>"Device telemetry"</p><h3>{title}</h3></div><strong>{value}</strong></header>
        {children()}
    </article> }
}

#[component]
fn ActivityList() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! { <div class="activity-timeline compact">
        <Show when=move || !state.activities.get().is_empty() fallback=|| view! { <p class="empty">"No operations recorded in this server session."</p> }>
            <For each=move || state.activities.get() key=|item| item.operation_id.clone() children=move |item| {
                let class = format!("activity-entry {}", item.state);
                view! { <article class=class>
                    <span class="activity-marker"></span>
                    <strong>{item.kind.to_uppercase()}</strong>
                    <span>{item.target}</span>
                    <small>{item.detail}</small>
                </article> }
            } />
        </Show>
    </div> }
}

#[component]
fn RuntimeSummary() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! { <article class="dashboard-panel runtime-summary">
        <header class="dashboard-panel-heading"><div><p>"Current state"</p><h3>"Runtime"</h3></div></header>
        <Stat label="Models" value=move || state.overview.get().map_or(0, |v| v.loaded_models).to_string() />
        <Stat label="Requests" value=move || state.overview.get().map_or_else(|| "—".to_owned(), |v| format!("{}/{} ok", v.completed_requests, v.total_requests)) />
        <Stat label="Stage" value=move || stage(state) />
        <Stat label="Uptime" value=move || uptime(state) />
    </article> }
}

#[component]
fn Stat<V>(label: &'static str, value: V) -> impl IntoView
where
    V: Fn() -> String + Send + Sync + 'static,
{
    view! { <div class="runtime-stat"><span>{label}</span><strong>{value}</strong></div> }
}
