mod geometry;

use leptos::prelude::*;

use crate::{state::RuntimeState, types::TelemetryPoint};

#[derive(Clone, Copy)]
pub enum Metric {
    Memory,
    Gpu,
    Temperature,
    Power,
    Prefill,
    Decode,
}

impl Metric {
    const fn value(self, point: &TelemetryPoint) -> Option<f64> {
        match self {
            Self::Memory => point.memory_percent,
            Self::Gpu => point.gpu_percent,
            Self::Temperature => point.temperature_celsius,
            Self::Power => point.power_watts,
            Self::Prefill => point.prefill_tokens_per_second,
            Self::Decode => point.decode_tokens_per_second,
        }
    }

    const fn gradient(self) -> &'static str {
        match self {
            Self::Memory => "memory-fill",
            Self::Gpu => "gpu-fill",
            Self::Temperature => "temperature-fill",
            Self::Power => "power-fill",
            Self::Prefill => "prefill-fill",
            Self::Decode => "decode-fill",
        }
    }
}

#[component]
pub fn Chart(metric: Metric, label: &'static str) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let plot = Memo::new(move |_| geometry::plot(&state.telemetry.get(), metric));
    let fill = format!("url(#{})", metric.gradient());
    view! { <div class="chart-frame device-chart" data-metric=metric.gradient()>
        <div class="chart-scale"><span>{move || format!("{:.0}", plot.get().maximum)}</span><span>"0"</span></div>
        <svg class="telemetry-chart" viewBox="0 0 640 220" preserveAspectRatio="none" role="img" aria-label=label>
            <defs>
                <linearGradient id=metric.gradient() x1="0" y1="0" x2="0" y2="1"
                    inner_html=r#"<stop offset="0%" stop-color="currentColor" stop-opacity=".24"/><stop offset="100%" stop-color="currentColor" stop-opacity="0"/>"# />
            </defs>
            <path class="chart-grid-line" d="M0 10H640M0 110H640M0 210H640M0 10V210M320 10V210M640 10V210" />
            <path class="chart-area" fill=fill d=move || plot.get().area />
            <path class="chart-line" stroke="currentColor" d=move || plot.get().line />
        </svg>
        <div class="chart-time"><span>{move || format!("{}s ago", plot.get().seconds)}</span><span>"Latest sample"</span></div>
        <Show when=move || plot.get().line.is_empty()><p class="chart-empty">"Waiting for measurements"</p></Show>
    </div> }
}
