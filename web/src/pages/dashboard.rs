mod chart;

use leptos::prelude::*;

use self::chart::{Chart, Metric};
use crate::{
    components::StatePill,
    state::{RuntimeState, number},
};

#[component]
pub fn DashboardPage() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let active = move || state.overview.get().is_some_and(|item| item.active_requests > 0);
    let rate = move |current: fn(&crate::types::Overview) -> Option<f64>,
                     last: fn(&crate::types::Overview) -> Option<f64>| {
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
                <p class="eyebrow">"Live runtime telemetry"</p>
                <span>{move || state.overview.get().map_or_else(|| "waiting for telemetry".to_owned(), |item| format!("sample {}", item.sampled_at_unix_ms))}</span>
            </div>
            <div class="metric-cards">
                <MetricCard label="Throughput" value=move || number(rate(|v| v.current_tokens_per_second, |v| v.last_tokens_per_second)) unit="tok/s" class="live" />
                <MetricCard label="Prefill" value=move || number(rate(|v| v.current_prefill_tokens_per_second, |v| v.last_prefill_tokens_per_second)) unit="tok/s" class="prefill" />
                <MetricCard label="Decode" value=move || number(rate(|v| v.current_decode_tokens_per_second, |v| v.last_decode_tokens_per_second)) unit="tok/s" class="decode" />
                <MetricCard label="TTFT" value=move || number(state.overview.get().and_then(|v| v.current_ttft_ms.or(v.last_ttft_ms))) unit="ms" class="ttft" />
                <MetricCard label="Memory" value=move || memory_used(state) unit="GiB" class="memory" />
                <MetricCard label="K/V cache" value=move || state.overview.get().map_or_else(|| "—".to_owned(), |v| v.kv_used_blocks.to_string()) unit="blocks" class="kv" />
            </div>
            <div class="dashboard-grid">
                <Panel title="Generation speed" kicker="Tokens per second" class="throughput-panel">
                    <Chart metrics=vec![Metric::E2e, Metric::Prefill, Metric::Decode] />
                </Panel>
                <Panel title="Host memory" kicker="Unified memory occupancy" class="memory-panel">
                    <Chart metrics=vec![Metric::Memory] />
                    <p class="panel-footnote">{move || state.overview.get().map_or_else(|| "Waiting for telemetry".to_owned(), |v| v.memory_source)}</p>
                </Panel>
                <article class="dashboard-panel activity-panel">
                    <header class="dashboard-panel-heading">
                        <div><p>"Runtime log"</p><h3>"Recent activity"</h3></div>
                        <StatePill state=move || if active() { "active".to_owned() } else { "idle".to_owned() } detail=String::new() progress=None />
                    </header>
                    <ActivityList />
                </article>
                <Panel title="K/V cache occupancy" kicker="Attention state" class="kv-panel">
                    <Chart metrics=vec![Metric::Kv] />
                    <p class="panel-footnote">{move || kv_detail(state)}</p>
                </Panel>
                <RuntimeStrip />
            </div>
        </section>
    }
}

#[component]
fn MetricCard<F>(
    label: &'static str,
    value: F,
    unit: &'static str,
    class: &'static str,
) -> impl IntoView
where
    F: Fn() -> String + Send + Sync + 'static,
{
    view! { <article><div><p>{label}</p><span class=format!("metric-signal {class}")></span></div><strong>{value}</strong><small>{unit}</small></article> }
}

#[component]
fn Panel(
    title: &'static str,
    kicker: &'static str,
    class: &'static str,
    children: Children,
) -> impl IntoView {
    view! { <article class=format!("dashboard-panel {class}")><header class="dashboard-panel-heading"><div><p>{kicker}</p><h3>{title}</h3></div></header>{children()}</article> }
}

#[component]
fn ActivityList() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! { <div class="activity-timeline">
        <Show when=move || !state.activities.get().is_empty() fallback=|| view! { <p class="empty">"No operations recorded in this server session."</p> }>
            <For each=move || state.activities.get() key=|item| item.operation_id.clone() children=move |item| {
                let progress = item.current.zip(item.total).map(|(current, total)| 100.0 * current as f64 / total.max(1) as f64);
                view! { <article class="activity-entry"><div class="activity-heading"><strong>{item.kind.clone()}</strong><StatePill state=item.state.clone() detail=item.detail.clone() progress /></div><p>{item.target}</p><small>{item.detail}</small></article> }
            } />
        </Show>
    </div> }
}

#[component]
fn RuntimeStrip() -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! { <article class="dashboard-panel runtime-strip">
        <Stat label="Loaded models" value=move || state.overview.get().map_or(0, |v| v.loaded_models).to_string() />
        <Stat label="Active requests" value=move || state.overview.get().map_or(0, |v| v.active_requests).to_string() />
        <Stat label="Requests" value=move || state.overview.get().map_or(0, |v| v.total_requests).to_string() />
        <Stat label="Failures" value=move || state.overview.get().map_or(0, |v| v.failed_requests).to_string() />
        <Stat label="Tokens" value=move || state.overview.get().map_or_else(|| "—".to_owned(), |v| format!("{} / {}", v.prompt_tokens, v.completion_tokens)) />
        <Stat label="Uptime" value=move || state.overview.get().map_or_else(|| "—".to_owned(), |v| format!("{}h {}m", v.uptime_ms / 3_600_000, v.uptime_ms / 60_000 % 60)) />
    </article> }
}

#[component]
fn Stat<F>(label: &'static str, value: F) -> impl IntoView
where
    F: Fn() -> String + Send + Sync + 'static,
{
    view! { <div><span>{label}</span><strong>{value}</strong></div> }
}

fn memory_used(state: RuntimeState) -> String {
    state
        .overview
        .get()
        .and_then(|v| v.host_total_memory_bytes.zip(v.host_available_memory_bytes))
        .map_or_else(
            || "—".to_owned(),
            |(total, available)| {
                format!("{:.1}", total.saturating_sub(available) as f64 / 1024_f64.powi(3))
            },
        )
}

fn kv_detail(state: RuntimeState) -> String {
    state.overview.get().map_or_else(
        || "No cache activity yet".to_owned(),
        |v| {
            let lookups = v.kv_hit_tokens + v.kv_miss_tokens;
            if lookups == 0 {
                format!("{} cached prefixes", v.kv_cached_prefixes)
            } else {
                format!(
                    "{:.1}% token hit rate · {} cached prefixes",
                    100.0 * v.kv_hit_tokens as f64 / lookups as f64,
                    v.kv_cached_prefixes
                )
            }
        },
    )
}
