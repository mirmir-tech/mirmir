use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Paragraph},
};

use super::super::{
    app::{App, ChatStatus},
    theme,
};

pub fn draw(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if let Some(error) = &app.chat_error {
        frame.render_widget(
            Paragraph::new(error.as_str()).style(Style::new().fg(theme::DANGER)),
            area,
        );
        return;
    }
    let columns = Layout::horizontal([
        Constraint::Percentage(22),
        Constraint::Percentage(26),
        Constraint::Percentage(26),
        Constraint::Percentage(26),
    ])
    .split(area);
    let values = Values::from_app(app);
    card(frame, columns[0], "TTFT", &values.ttft, values.ttft_active);
    card(frame, columns[1], "PREFILL", &values.prefill, values.stage == "prefill");
    card(frame, columns[2], "DECODE", &values.decode, values.stage == "decode");
    card(frame, columns[3], "TOKENS", &values.tokens, false);
}

struct Values {
    stage: String,
    ttft: String,
    prefill: String,
    decode: String,
    tokens: String,
    ttft_active: bool,
}

impl Values {
    fn from_app(app: &App) -> Self {
        if app.chat_status == ChatStatus::Generating {
            return app.chat_live_metrics.as_ref().map_or_else(Self::empty, |metrics| Self {
                stage: metrics.stage.clone(),
                ttft: metrics.ttft_ms.map_or_else(
                    || format!("waiting {}", milliseconds(metrics.ttft_pending_ms)),
                    |value| format!("{value:.2} ms"),
                ),
                prefill: rate(metrics.prefill_tokens_per_second),
                decode: rate(metrics.decode_tokens_per_second),
                tokens: format!("{}p / {}c", metrics.prompt_tokens, metrics.completion_tokens),
                ttft_active: metrics.ttft_ms.is_none(),
            });
        }
        app.chat_metrics.as_ref().map_or_else(Self::empty, |metrics| Self {
            stage: "done".to_owned(),
            ttft: milliseconds(metrics.ttft_ms),
            prefill: rate(metrics.prefill_tokens_per_second),
            decode: rate(metrics.decode_tokens_per_second),
            tokens: format!("{}p / {}c", metrics.prompt_tokens, metrics.completion_tokens),
            ttft_active: false,
        })
    }

    fn empty() -> Self {
        Self {
            stage: "idle".to_owned(),
            ttft: "—".to_owned(),
            prefill: "—".to_owned(),
            decode: "—".to_owned(),
            tokens: "—".to_owned(),
            ttft_active: false,
        }
    }
}

fn card(frame: &mut Frame<'_>, area: Rect, title: &str, value: &str, active: bool) {
    let color = if active {
        theme::SIGNAL
    } else {
        theme::INK
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            value.to_owned(),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ))
        .block(Block::bordered().title(format!(" {title} ")).border_style(Style::new().fg(
            if active {
                theme::SIGNAL
            } else {
                theme::BORDER
            },
        )))
        .style(Style::new().bg(theme::SURFACE)),
        area,
    );
}

fn rate(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2} tok/s"))
}

fn milliseconds(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.2} ms"))
}
