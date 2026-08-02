use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Gauge, Paragraph},
};

use super::super::{app::App, theme};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(repo) = app.transfer_repo.as_deref() else {
        let selected_error = app
            .selected_local_model()
            .filter(|model| !model.loadable)
            .map(|model| model.load_unavailable_reason.as_str());
        let text = app
            .action_message
            .as_deref()
            .or(selected_error)
            .unwrap_or("Select a model to inspect its available actions");
        frame.render_widget(Paragraph::new(text).style(Style::new().fg(theme::MUTED)), area);
        return;
    };
    let phase = app.transfer_phase.as_deref().unwrap_or("starting");
    let color = phase_color(phase);
    let inner = area.inner(Margin { horizontal: 1, vertical: 1 });
    frame.render_widget(
        Block::bordered()
            .title(format!(" DOWNLOAD · {} ", phase.to_ascii_uppercase()))
            .border_style(Style::new().fg(color))
            .style(Style::new().bg(theme::SURFACE)),
        area,
    );
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(repo, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("  ·  {}", app.action_message.as_deref().unwrap_or("starting")),
                Style::new().fg(theme::MUTED),
            ),
        ])),
        rows[0],
    );
    let (ratio, label) = progress(app);
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::new().fg(color).bg(theme::RAISED))
            .ratio(ratio)
            .label(Span::styled(label, Style::new().fg(theme::INK).add_modifier(Modifier::BOLD))),
        rows[1],
    );
}

fn progress(app: &App) -> (f64, String) {
    let Some(total) = app.transfer_total_bytes.filter(|total| *total > 0) else {
        let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
            [usize::try_from(app.animation_tick % 10).unwrap_or(0)];
        return (
            0.0,
            format!(
                "{spinner} waiting for total size · {} received",
                human_bytes(app.transfer_downloaded_bytes)
            ),
        );
    };
    let current = app.transfer_downloaded_bytes.min(total);
    let tenths = u16::try_from(u128::from(current) * 1_000 / u128::from(total)).unwrap_or(1_000);
    (
        f64::from(tenths) / 1_000.0,
        format!(
            "{}.{:01}%  ·  {} / {}",
            tenths / 10,
            tenths % 10,
            human_bytes(current),
            human_bytes(total)
        ),
    )
}

fn phase_color(phase: &str) -> ratatui::style::Color {
    match phase {
        "available" => theme::SUCCESS,
        "failed" => theme::DANGER,
        _ => theme::SIGNAL,
    }
}

fn human_bytes(value: u64) -> String {
    const KIB: u64 = 1_024;
    const MIB: u64 = KIB * 1_024;
    const GIB: u64 = MIB * 1_024;
    if value >= GIB {
        scaled(value, GIB, "GiB")
    } else if value >= MIB {
        scaled(value, MIB, "MiB")
    } else if value >= KIB {
        scaled(value, KIB, "KiB")
    } else {
        format!("{value} B")
    }
}

fn scaled(value: u64, unit: u64, suffix: &str) -> String {
    let whole = value / unit;
    let hundredths = value % unit * 100 / unit;
    format!("{whole}.{hundredths:02} {suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_binary_download_sizes() {
        assert_eq!(human_bytes(512 * 1_024 * 1_024), "512.00 MiB");
        assert_eq!(human_bytes(1_024 * 1_024 * 1_024), "1.00 GiB");
    }
}
