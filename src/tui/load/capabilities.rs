use std::fmt::Write as _;

use ratatui::{Frame, layout::Rect, style::Style, widgets::Paragraph};

use super::super::theme;
use crate::tui::app::LoadDialog;

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, dialog: &LoadDialog) -> bool {
    if dialog.task == "generation" {
        return false;
    }
    frame.render_widget(Paragraph::new(summary(dialog)).style(Style::new().fg(theme::MUTED)), area);
    true
}

pub(super) fn summary(dialog: &LoadDialog) -> String {
    let Some(capabilities) = dialog.capabilities.as_ref() else {
        return format!("task {:<14} no editable load settings", dialog.task);
    };
    let mut lines = vec![format!("task {:<14} detected from checkpoint", dialog.task)];
    let limit = if dialog.task == "rerank" {
        "maximum pair"
    } else {
        "maximum input"
    };
    lines.push(format!("{limit:<19} {} tokens", capabilities.max_input_tokens));
    if let Some(embedding) = capabilities.embedding.as_ref() {
        lines.push(format!(
            "output              {} dimensions · {} · {} · prompt {}",
            embedding.native_dimensions,
            embedding.pooling.replace('_', " "),
            if embedding.normalized {
                "normalized"
            } else {
                "not normalized"
            },
            if embedding.includes_prompt {
                "included"
            } else {
                "excluded"
            }
        ));
        lines.push(format!("prompt presets      {}", prompts(embedding)));
    }
    if let Some(rerank) = capabilities.rerank.as_ref() {
        let labels = if rerank.labels == 1 {
            "label"
        } else {
            "labels"
        };
        lines.push(format!(
            "classifier          {} {labels} · {} pooling · {}",
            rerank.labels,
            rerank.pooling,
            if rerank.raw_scores {
                "raw logits available"
            } else {
                "relevance scores"
            }
        ));
    }
    lines.push("load settings       none required".to_owned());
    lines.join("\n")
}

fn prompts(capabilities: &crate::rpc::proto::EmbeddingCapabilities) -> String {
    if capabilities.prompt_names.is_empty() {
        return "none".to_owned();
    }
    let mut prompts = capabilities.prompt_names.join(", ");
    if let Some(default) = capabilities.default_prompt.as_deref() {
        let _ignored = write!(prompts, " · default {default}");
    }
    prompts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_task_contracts_without_load_settings() {
        let embedding = crate::rpc::proto::EmbeddingCapabilities {
            native_dimensions: 1024,
            pooling: "last_token".to_owned(),
            normalized: true,
            prompt_names: vec!["query".to_owned(), "passage".to_owned()],
            default_prompt: Some("query".to_owned()),
            includes_prompt: false,
        };
        let mut dialog = LoadDialog {
            target: super::super::super::app::LoadTarget {
                selector: String::new(),
                config_id: String::new(),
                repo_id: String::new(),
                revision: String::new(),
                commit: String::new(),
            },
            task: "embedding".to_owned(),
            capabilities: Some(crate::rpc::proto::ModelTaskCapabilities {
                max_input_tokens: 32_768,
                embedding: Some(embedding),
                rerank: None,
            }),
            status: super::super::super::app::LoadStatus::Editing,
            fields: std::array::from_fn(|_| String::new()),
            selected: 0,
            has_mirmir_overrides: false,
            memory: None,
            force: false,
            progress: None,
            error: None,
            restore: None,
        };

        let embedding_summary = summary(&dialog);
        assert!(embedding_summary.contains("1024 dimensions · last token · normalized"));
        assert!(embedding_summary.contains("query, passage · default query"));
        assert!(embedding_summary.contains("none required"));

        dialog.task = "rerank".to_owned();
        dialog.capabilities = Some(crate::rpc::proto::ModelTaskCapabilities {
            max_input_tokens: 8192,
            embedding: None,
            rerank: Some(crate::rpc::proto::RerankCapabilities {
                labels: 1,
                pooling: "cls".to_owned(),
                raw_scores: true,
            }),
        });
        let rerank_summary = summary(&dialog);
        assert!(rerank_summary.contains("maximum pair"));
        assert!(rerank_summary.contains("8192 tokens"));
        assert!(rerank_summary.contains("1 label · cls pooling · raw logits available"));
    }
}
