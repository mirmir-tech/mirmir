use std::fmt::Write;

use leptos::prelude::*;

use crate::{state::RuntimeState, types::TelemetryPoint};

const WIDTH: f64 = 640.0;
const BASELINE: f64 = 210.0;

#[derive(Clone, Copy)]
pub enum Metric {
    Memory,
    Gpu,
    Temperature,
    Power,
}

impl Metric {
    const fn value(self, point: &TelemetryPoint) -> Option<f64> {
        match self {
            Self::Memory => point.memory_percent,
            Self::Gpu => point.gpu_percent,
            Self::Temperature => point.temperature_celsius,
            Self::Power => point.power_watts,
        }
    }

    const fn color(self) -> &'static str {
        match self {
            Self::Memory => "#4f8ef7",
            Self::Gpu => "#79d7ff",
            Self::Temperature => "#f07178",
            Self::Power => "#61d6a3",
        }
    }

    const fn gradient(self) -> &'static str {
        match self {
            Self::Memory => "memory-fill",
            Self::Gpu => "gpu-fill",
            Self::Temperature => "temperature-fill",
            Self::Power => "power-fill",
        }
    }
}

#[component]
pub fn Chart(metric: Metric, label: &'static str) -> impl IntoView {
    let state = expect_context::<RuntimeState>();
    let line = move || paths(&state.telemetry.get(), metric).0;
    let area = move || paths(&state.telemetry.get(), metric).1;
    let available = move || state.telemetry.get().iter().any(|point| metric.value(point).is_some());
    let fill = format!("url(#{})", metric.gradient());
    view! { <div class="chart-frame device-chart">
        <svg class="telemetry-chart" viewBox="0 0 640 220" preserveAspectRatio="none" role="img" aria-label=label>
            <defs>
                <linearGradient id=metric.gradient() x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stop-color=metric.color() stop-opacity=".34" />
                    <stop offset="72%" stop-color=metric.color() stop-opacity=".08" />
                    <stop offset="100%" stop-color=metric.color() stop-opacity="0" />
                </linearGradient>
            </defs>
            <path class="chart-grid-line" d="M0 52.5H640M0 105H640M0 157.5H640" />
            <path class="chart-area" fill=fill d=area />
            <path class="chart-line" stroke=metric.color() d=line />
        </svg>
        <Show when=move || !available()><p class="chart-empty">"Telemetry unavailable"</p></Show>
    </div> }
}

fn paths(points: &[TelemetryPoint], metric: Metric) -> (String, String) {
    let visible = &points[points.len().saturating_sub(300)..];
    let maximum = maximum(visible, metric);
    let denominator = visible.len().saturating_sub(1).max(1) as f64;
    let coordinates = visible
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            let value = metric.value(point)?.max(0.0);
            let x = WIDTH * index as f64 / denominator;
            let y = BASELINE - 200.0 * value / maximum;
            Some((x, y))
        })
        .collect::<Vec<_>>();
    let mut line = String::new();
    let mut area_line = String::new();
    for (index, (x, y)) in coordinates.iter().enumerate() {
        let command = if index == 0 {
            "M"
        } else {
            "L"
        };
        let line_result = write!(&mut line, "{command}{x:.1},{y:.1}");
        let area_result = write!(&mut area_line, "L{x:.1},{y:.1}");
        debug_assert!(line_result.is_ok() && area_result.is_ok());
    }
    let area =
        coordinates
            .first()
            .zip(coordinates.last())
            .map_or_else(String::new, |(first, last)| {
                format!("M{:.1},{BASELINE:.1}{area_line}L{:.1},{BASELINE:.1}Z", first.0, last.0)
            });
    (line, area)
}

fn maximum(points: &[TelemetryPoint], metric: Metric) -> f64 {
    let measured = points.iter().filter_map(|point| metric.value(point)).fold(0.0, f64::max);
    let expected = match metric {
        Metric::Memory | Metric::Gpu | Metric::Temperature => 100.0,
        Metric::Power => {
            points.iter().filter_map(|point| point.power_limit_watts).fold(0.0, f64::max)
        },
    };
    expected.max(measured * 1.1).max(1.0)
}
