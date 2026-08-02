use leptos::prelude::Get;

use crate::{
    state::{RuntimeState, bytes},
    types::Overview,
};

pub(super) fn active(state: RuntimeState) -> bool {
    state.overview.get().is_some_and(|item| item.active_requests > 0)
}

pub(super) fn mean(state: RuntimeState, value: fn(&Overview) -> Option<f64>, unit: &str) -> String {
    state
        .overview
        .get()
        .and_then(|overview| value(&overview))
        .map_or_else(|| "mean —".to_owned(), |value| format!("mean {value:.1} {unit}"))
}

pub(super) fn device_caption(state: RuntimeState) -> String {
    state.overview.get().map_or_else(
        || "Waiting for telemetry".to_owned(),
        |overview| {
            if overview.device_name.is_empty() {
                overview.memory_source
            } else {
                overview.device_name
            }
        },
    )
}

pub(super) fn memory_label(state: RuntimeState) -> String {
    state
        .overview
        .get()
        .and_then(|value| value.host_total_memory_bytes.zip(value.host_available_memory_bytes))
        .map_or_else(
            || "—".to_owned(),
            |(total, available)| {
                let used = total.saturating_sub(available);
                let percent = if total == 0 {
                    0.0
                } else {
                    100.0 * used as f64 / total as f64
                };
                format!("{} / {} · {percent:.1}%", bytes(used), bytes(total))
            },
        )
}

pub(super) fn percent_label(state: RuntimeState, value: fn(&Overview) -> Option<f64>) -> String {
    state
        .overview
        .get()
        .and_then(|overview| value(&overview))
        .map_or_else(|| "—".to_owned(), |value| format!("{value:.1}%"))
}

pub(super) fn temperature_label(state: RuntimeState) -> String {
    state
        .overview
        .get()
        .and_then(|value| value.device_temperature_celsius)
        .map_or_else(|| "—".to_owned(), |value| format!("{value:.1}°C"))
}

pub(super) fn power_label(state: RuntimeState) -> String {
    state
        .overview
        .get()
        .and_then(|overview| {
            overview
                .device_power_watts
                .map(|power| (power, overview.device_power_limit_watts))
        })
        .map_or_else(
            || "—".to_owned(),
            |(power, limit)| {
                limit.map_or_else(
                    || format!("{power:.1} W"),
                    |limit| format!("{power:.1} / {limit:.1} W"),
                )
            },
        )
}

pub(super) fn stage(state: RuntimeState) -> String {
    state.overview.get().map_or_else(
        || "—".to_owned(),
        |overview| {
            if overview.active_requests == 0 {
                "idle".to_owned()
            } else {
                overview.active_stage
            }
        },
    )
}

pub(super) fn uptime(state: RuntimeState) -> String {
    state.overview.get().map_or_else(
        || "—".to_owned(),
        |value| format!("{}h {}m", value.uptime_ms / 3_600_000, value.uptime_ms / 60_000 % 60),
    )
}
