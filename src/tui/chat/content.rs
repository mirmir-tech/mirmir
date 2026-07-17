use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::theme;

pub fn message_lines(content: &str) -> Vec<Line<'static>> {
    let starts_in_thought = !content.contains("<think>") && content.contains("</think>");
    let mut in_thought = starts_in_thought;
    content.split('\n').map(|line| styled_line(line, &mut in_thought)).collect()
}

pub fn thought_lines(content: &str) -> Vec<Line<'static>> {
    content.split('\n').map(|line| Line::from(styled(line, true))).collect()
}

fn styled_line(mut content: &str, in_thought: &mut bool) -> Line<'static> {
    let mut spans = Vec::new();
    while !content.is_empty() {
        let marker = if *in_thought {
            "</think>"
        } else {
            "<think>"
        };
        let Some(index) = content.find(marker) else {
            spans.push(styled(content, *in_thought));
            break;
        };
        if index > 0 {
            spans.push(styled(&content[..index], *in_thought));
        }
        content = &content[index + marker.len()..];
        *in_thought = !*in_thought;
    }
    Line::from(spans)
}

fn styled(content: &str, thought: bool) -> Span<'static> {
    let style = if thought {
        Style::new().fg(theme::THOUGHT).add_modifier(Modifier::ITALIC)
    } else {
        Style::new().fg(theme::INK)
    };
    Span::styled(content.to_owned(), style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_think_markers_and_dims_reasoning() {
        let lines = message_lines("<think>draft\nstill thinking</think>answer");
        assert_eq!(lines[0].spans[0].content, "draft");
        assert_eq!(lines[0].spans[0].style.fg, Some(theme::THOUGHT));
        assert_eq!(lines[1].spans[0].style.fg, Some(theme::THOUGHT));
        assert_eq!(lines[1].spans[1].content, "answer");
        assert_eq!(lines[1].spans[1].style.fg, Some(theme::INK));
        assert_eq!(thought_lines("semantic")[0].spans[0].style.fg, Some(theme::THOUGHT));
    }
}
