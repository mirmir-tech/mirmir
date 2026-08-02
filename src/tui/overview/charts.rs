use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Block, Chart, Dataset, GraphType, Paragraph},
};

use crate::tui::{app::App, theme};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let rows = Layout::vertical([Constraint::Ratio(1, 2); 2]).split(area);
    let top = Layout::horizontal([Constraint::Ratio(1, 2); 2]).split(rows[0]);
    let bottom = Layout::horizontal([Constraint::Ratio(1, 2); 2]).split(rows[1]);
    let memory = values(app, |point| point.memory_percent);
    let gpu = values(app, |point| point.gpu_percent);
    let temperature = values(app, |point| point.temperature_celsius);
    chart(frame, top[0], "MEMORY", theme::SIGNAL, &memory, 100.0, memory_label(app));
    chart(frame, top[1], "GPU", theme::GLACIER, &gpu, 100.0, latest(&gpu, "%"));
    chart(
        frame,
        bottom[0],
        "TEMPERATURE",
        theme::DANGER,
        &temperature,
        100.0,
        latest(&temperature, "°C"),
    );
    let power = values(app, |point| point.power_watts);
    let limit = app
        .telemetry_history
        .iter()
        .filter_map(|point| point.power_limit_watts)
        .fold(0.0, f64::max);
    chart(
        frame,
        bottom[1],
        "POWER",
        theme::SUCCESS,
        &power,
        limit,
        power_label(&power, limit),
    );
}

fn values(
    app: &App,
    field: impl Fn(&crate::tui::app::TelemetryPoint) -> Option<f64>,
) -> Vec<(f64, f64)> {
    app.telemetry_history
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            let index = u32::try_from(index).ok().map(f64::from)?;
            field(point).map(|value| (index, value))
        })
        .collect()
}

fn chart(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    color: Color,
    values: &[(f64, f64)],
    expected_maximum: f64,
    value_label: String,
) {
    let block = Block::bordered()
        .title(Line::from(vec![
            Span::styled(
                format!(" {label} "),
                Style::new().fg(theme::INK).add_modifier(Modifier::BOLD),
            ),
            Span::styled(value_label, Style::new().fg(color)),
        ]))
        .border_style(Style::new().fg(theme::BORDER))
        .style(Style::new().bg(theme::SURFACE));
    if values.is_empty() {
        frame.render_widget(
            Paragraph::new("telemetry unavailable")
                .centered()
                .style(Style::new().fg(theme::MUTED))
                .block(block),
            area,
        );
        return;
    }
    let measured_maximum = values.iter().map(|(_, value)| *value).fold(0.0, f64::max);
    let maximum = expected_maximum.max(measured_maximum * 1.1).max(1.0);
    let x_maximum = values.last().map_or(1.0, |(index, _)| *index).max(1.0);
    let datasets = vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Area)
            .fill_to_y(0.0)
            .style(Style::new().fg(color))
            .data(values),
    ];
    frame.render_widget(
        Chart::new(datasets)
            .block(block)
            .x_axis(Axis::default().bounds([0.0, x_maximum]))
            .y_axis(Axis::default().bounds([0.0, maximum])),
        area,
    );
}

fn latest(values: &[(f64, f64)], unit: &str) -> String {
    values
        .last()
        .map_or_else(|| "—".to_owned(), |(_, value)| format!("{value:.1}{unit} "))
}

fn memory_label(app: &App) -> String {
    app.telemetry_history
        .iter()
        .rev()
        .find_map(|point| {
            let used = point.memory_used_bytes?;
            let total = point.memory_total_bytes?;
            let percent = point.memory_percent?;
            let (used, total, unit) = byte_values(used, total);
            Some(format!("{used:.1}/{total:.1} {unit} · {percent:.1}% "))
        })
        .unwrap_or_else(|| "—".to_owned())
}

fn byte_values(used: u64, total: u64) -> (f64, f64, &'static str) {
    const GIB: u64 = 1_073_741_824;
    const MIB: u64 = 1_048_576;
    let divisor = if total >= GIB {
        GIB
    } else {
        MIB
    };
    let unit = if total >= GIB {
        "GiB"
    } else {
        "MiB"
    };
    (decimal_tenths(used, divisor), decimal_tenths(total, divisor), unit)
}

fn decimal_tenths(bytes: u64, divisor: u64) -> f64 {
    let tenths = u32::try_from(u128::from(bytes) * 10 / u128::from(divisor)).unwrap_or(u32::MAX);
    f64::from(tenths) / 10.0
}

fn power_label(values: &[(f64, f64)], limit: f64) -> String {
    values.last().map_or_else(
        || "—".to_owned(),
        |(_, value)| {
            if limit > 0.0 {
                format!("{value:.1}/{limit:.1} W ")
            } else {
                format!("{value:.1} W ")
            }
        },
    )
}
