use leptos::prelude::*;

use crate::{state::RuntimeState, types::TelemetryPoint};

#[derive(Clone, Copy)]
pub enum Metric {
    E2e,
    Prefill,
    Decode,
    Memory,
    Kv,
}

impl Metric {
    const fn value(self, point: &TelemetryPoint) -> f64 {
        match self {
            Self::E2e => point.e2e,
            Self::Prefill => point.prefill,
            Self::Decode => point.decode,
            Self::Memory => point.memory_percent,
            Self::Kv => point.kv_percent,
        }
    }

    const fn color(self) -> &'static str {
        match self {
            Self::E2e => "#79d7ff",
            Self::Prefill => "#4f8ef7",
            Self::Decode => "#61d6a3",
            Self::Memory => "#e8b15a",
            Self::Kv => "#c48b3a",
        }
    }
}

#[component]
pub fn Chart(metrics: Vec<Metric>) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    view! { <div class="chart-frame compact">
        <svg class="telemetry-chart" viewBox="0 0 640 220" preserveAspectRatio="none" role="img" aria-label="Runtime telemetry over time">
            <path class="chart-grid-line" d="M0 55H640M0 110H640M0 165H640" />
            <For each=move || metrics.clone() key=|metric| *metric as u8 children=move |metric| {
                let points = move || chart_points(&state.telemetry.get(), metric);
                view! { <polyline class="chart-line" stroke=metric.color() points=points /> }
            } />
        </svg>
        <Show when=move || state.telemetry.get().len() < 2><p class="chart-empty">"Collecting telemetry samples…"</p></Show>
    </div> }
}

fn chart_points(points: &[TelemetryPoint], metric: Metric) -> String {
    let visible = &points[points.len().saturating_sub(300)..];
    let maximum = visible.iter().map(|point| metric.value(point)).fold(1.0_f64, f64::max);
    let denominator = visible.len().saturating_sub(1).max(1) as f64;
    visible
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let x = 640.0 * index as f64 / denominator;
            let y = 210.0 - 200.0 * metric.value(point) / maximum;
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}
