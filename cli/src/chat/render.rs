use libmir::runtime::{kv::CacheStats, metrics::GenerationMetricsRecorder};

use super::{flow::GeneratedTokens, response::ChatResponse};
use crate::{args::ChatArgs, ui};

pub(super) struct RenderInput<'a> {
    pub args: &'a ChatArgs,
    pub did_stream: bool,
    pub generated: &'a GeneratedTokens,
    pub response: &'a ChatResponse,
    pub metrics: &'a GenerationMetricsRecorder,
    pub stats: CacheStats,
    pub header: &'a [String],
    pub trace: &'a [String],
}

pub(super) fn render(input: RenderInput<'_>) -> Vec<String> {
    if !(input.args.verbose || input.args.trace) {
        return render_plain(
            input.args, input.did_stream, input.response, input.metrics, input.stats,
        );
    }
    render_report(input)
}

fn render_plain(
    args: &ChatArgs,
    did_stream: bool,
    response: &ChatResponse,
    metrics: &GenerationMetricsRecorder,
    stats: CacheStats,
) -> Vec<String> {
    if did_stream {
        return if args.bench {
            ui::bench_report(&metrics.snapshot(stats))
        } else {
            Vec::new()
        };
    }
    let text = response.display_text(args.review).to_owned();
    if !args.bench {
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![text]
        };
    }
    let mut lines = Vec::new();
    if !text.is_empty() {
        lines.push(text);
        lines.push(String::new());
    }
    lines.extend(ui::bench_report(&metrics.snapshot(stats)));
    lines
}

fn render_report(input: RenderInput<'_>) -> Vec<String> {
    let RenderInput {
        args,
        did_stream,
        generated,
        response,
        metrics,
        stats,
        header,
        trace,
    } = input;
    let token_ids = token_ids(&generated.tokens);
    let metrics = metrics.snapshot(stats);
    let thoughts = if did_stream {
        Vec::new()
    } else {
        thoughts(args, response)
    };
    let answer = if did_stream {
        ""
    } else {
        response.final_text().unwrap_or("")
    };
    ui::chat_report(&ui::ChatReport {
        answer,
        thoughts: &thoughts,
        header,
        trace,
        finish_reason: generated.finish_reason,
        token_ids: &token_ids,
        metrics: &metrics,
    })
}

fn thoughts(args: &ChatArgs, response: &ChatResponse) -> Vec<String> {
    if args.review {
        response.thoughts().map(str::to_owned).collect()
    } else {
        Vec::new()
    }
}

fn token_ids(tokens: &[u32]) -> String {
    if tokens.is_empty() {
        return "none".into();
    }
    tokens.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
}
