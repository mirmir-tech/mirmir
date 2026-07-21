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
            if *in_thought {
                spans.push(styled(content, true));
            } else {
                spans.extend(markdown_spans(content));
            }
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

fn markdown_spans(content: &str) -> Vec<Span<'static>> {
    let heading = Style::new().fg(theme::GLACIER).add_modifier(Modifier::BOLD);
    let block = content
        .strip_prefix("### ")
        .map(|body| ("", body, heading))
        .or_else(|| content.strip_prefix("## ").map(|body| ("", body, heading)))
        .or_else(|| {
            content
                .strip_prefix("# ")
                .map(|body| ("", body, Style::new().fg(theme::SIGNAL).add_modifier(Modifier::BOLD)))
        })
        .or_else(|| {
            content
                .strip_prefix("- ")
                .or_else(|| content.strip_prefix("* "))
                .map(|body| ("• ", body, Style::new().fg(theme::INK)))
        })
        .or_else(|| {
            content.strip_prefix("> ").map(|body| {
                ("│ ", body, Style::new().fg(theme::MUTED).add_modifier(Modifier::ITALIC))
            })
        })
        .or_else(|| {
            content
                .strip_prefix("```")
                .map(|body| ("┄ ", body, Style::new().fg(theme::SIGNAL)))
        });
    let (prefix, body, block_style) =
        block.unwrap_or_else(|| ("", content, Style::new().fg(theme::INK)));
    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::styled(prefix.to_owned(), block_style));
    }
    spans.extend(inline_spans(body, block_style));
    spans
}

fn inline_spans(mut content: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    while !content.is_empty() {
        let markers = [("**", Modifier::BOLD), ("`", Modifier::DIM)];
        let Some((index, marker, modifier)) = markers
            .iter()
            .filter_map(|(marker, modifier)| {
                content.find(marker).map(|index| (index, *marker, *modifier))
            })
            .min_by_key(|(index, _, _)| *index)
        else {
            spans.push(Span::styled(content.to_owned(), base));
            break;
        };
        if index > 0 {
            spans.push(Span::styled(content[..index].to_owned(), base));
        }
        let rest = &content[index + marker.len()..];
        let Some(end) = rest.find(marker) else {
            spans.push(Span::styled(content[index..].to_owned(), base));
            break;
        };
        spans.push(Span::styled(
            rest[..end].to_owned(),
            base.add_modifier(modifier).fg(if marker == "`" {
                theme::SIGNAL
            } else {
                theme::INK
            }),
        ));
        content = &rest[end + marker.len()..];
    }
    spans
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
