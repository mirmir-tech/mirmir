use std::fmt::Write;

use super::{Metric, TelemetryPoint};

#[derive(Clone, PartialEq)]
pub(super) struct Plot {
    pub line: String,
    pub area: String,
    pub maximum: f64,
    pub seconds: u64,
}

pub(super) fn plot(points: &[TelemetryPoint], metric: Metric) -> Plot {
    let visible = &points[points.len().saturating_sub(300)..];
    let first = visible.first().map_or(0, |point| point.sampled_at_unix_ms);
    let duration = visible.last().map_or(0, |point| point.sampled_at_unix_ms.saturating_sub(first));
    let measured = visible
        .iter()
        .filter_map(|point| metric.value(point))
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max);
    let maximum = match metric {
        Metric::Memory | Metric::Gpu => 100.0,
        Metric::Power => visible
            .iter()
            .filter_map(|point| point.power_limit_watts)
            .filter(|value| value.is_finite())
            .fold((measured * 1.15).ceil().max(1.0), f64::max),
        _ => (measured * 1.15).ceil().max(1.0),
    };
    let mut plot = Plot {
        line: String::new(),
        area: String::new(),
        maximum,
        seconds: duration / 1000,
    };
    let mut start = None;
    let mut end = 0.0;
    for point in visible {
        let Some(value) = metric.value(point).filter(|value| value.is_finite()) else {
            close_area(&mut plot.area, &mut start, end);
            continue;
        };
        let x =
            640.0 * point.sampled_at_unix_ms.saturating_sub(first) as f64 / duration.max(1) as f64;
        let y = 210.0 - 200.0 * value.clamp(0.0, maximum) / maximum;
        let command = if start.is_none() {
            "M"
        } else {
            "L"
        };
        if start.is_none() {
            start = Some(x);
            let result = write!(plot.area, "M{x:.1},210");
            debug_assert!(result.is_ok());
        }
        let result = write!(plot.line, "{command}{x:.1},{y:.1}");
        debug_assert!(result.is_ok());
        let result = write!(plot.area, "L{x:.1},{y:.1}");
        debug_assert!(result.is_ok());
        end = x;
    }
    close_area(&mut plot.area, &mut start, end);
    plot
}

fn close_area(area: &mut String, start: &mut Option<f64>, end: f64) {
    if start.take().is_some() {
        let result = write!(area, "L{end:.1},210Z");
        debug_assert!(result.is_ok());
    }
}

#[cfg(test)]
mod tests;
