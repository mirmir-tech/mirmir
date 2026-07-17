use libmir::runtime::metrics::GenerationMetrics;

mod bench;

pub use bench::production_bench_report;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ITALIC: &str = "\x1b[3m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const BLUE: &str = "\x1b[34m";
const THOUGHT: &str = "\x1b[90m";

pub struct ChatReport<'a> {
    pub answer: &'a str,
    pub thoughts: &'a [String],
    pub header: &'a [String],
    pub trace: &'a [String],
    pub finish_reason: &'a str,
    pub token_ids: &'a str,
    pub metrics: &'a GenerationMetrics,
}

#[must_use]
pub fn chat_report(report: &ChatReport<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(section("model", CYAN, report.header));
    if !report.trace.is_empty() {
        lines.push(String::new());
        lines.extend(section("trace", YELLOW, report.trace));
    }
    if !report.thoughts.is_empty() {
        lines.push(String::new());
        lines.extend(section("thinking", THOUGHT, &thought_lines(report.thoughts)));
    }
    if !report.answer.trim().is_empty() {
        lines.push(String::new());
        lines.push(format!("{GREEN}{BOLD}assistant{RESET}"));
        lines.push(report.answer.to_owned());
    }
    lines.push(String::new());
    lines.extend(section(
        "runtime",
        BLUE,
        &runtime_lines(report.finish_reason, report.token_ids, report.metrics),
    ));
    lines.push(String::new());
    lines.extend(section("profile", GREEN, &bench_lines(report.metrics)));
    lines
}

#[must_use]
pub fn bench_report(metrics: &GenerationMetrics) -> Vec<String> {
    section("bench", GREEN, &bench_lines(metrics))
}

#[must_use]
#[cfg(target_os = "macos")]
pub fn trace_report(lines: &[String]) -> Vec<String> {
    let mut output = Vec::new();
    output.extend(section("trace-metal", CYAN, lines));
    output
}

fn section(title: &str, color: &str, lines: &[String]) -> Vec<String> {
    let mut output = Vec::with_capacity(lines.len() + 2);
    output.push(format!("{color}{BOLD}+-- {title} {RESET}"));
    output.extend(lines.iter().map(|line| format!("{color}|{RESET} {}", highlight(line))));
    output.push(format!("{color}+--{RESET}"));
    output
}

fn bench_lines(metrics: &GenerationMetrics) -> Vec<String> {
    vec![
        kv("inspect", &format!("{} ms", ms(metrics.durations_ms.inspect))),
        kv(
            "prompt",
            &format!("{} tokens | {} ms", metrics.tokens.prompt, ms(metrics.durations_ms.prompt)),
        ),
        kv("load", &format!("{} ms", ms(metrics.durations_ms.load))),
        kv(
            "prefill",
            &format!(
                "{} tokens | {} ms | {}",
                metrics.tokens.prefill,
                ms(metrics.durations_ms.prefill),
                metrics.throughput.prefill
            ),
        ),
        kv(
            "decode",
            &format!(
                "{} tokens | {} ms | {}",
                metrics.tokens.decode_steps,
                ms(metrics.durations_ms.decode),
                metrics.throughput.decode
            ),
        ),
        kv("sampling", &format!("{} ms", ms(metrics.durations_ms.sampling))),
        kv(
            "generated",
            &format!(
                "{} tokens | total {} | active {}",
                metrics.tokens.generated,
                metrics.throughput.generated,
                metrics.throughput.generated_active
            ),
        ),
    ]
}

fn runtime_lines(reason: &str, token_ids: &str, metrics: &GenerationMetrics) -> Vec<String> {
    vec![
        kv("finish", reason),
        kv("generated_token_ids", token_ids),
        kv(
            "time",
            &format!(
                "total {} ms | active {} ms | prefill {} ms | decode {} ms",
                ms(metrics.durations_ms.total),
                ms(metrics.durations_ms.active),
                ms(metrics.durations_ms.prefill),
                ms(metrics.durations_ms.decode)
            ),
        ),
        kv(
            "tokens",
            &format!(
                "prompt {} | prefill {} | generated {} | decode steps {}",
                metrics.tokens.prompt,
                metrics.tokens.prefill,
                metrics.tokens.generated,
                metrics.tokens.decode_steps
            ),
        ),
        kv(
            "throughput",
            &format!(
                "prefill {} | decode {} | generated {} | active {}",
                metrics.throughput.prefill,
                metrics.throughput.decode,
                metrics.throughput.generated,
                metrics.throughput.generated_active
            ),
        ),
        kv(
            "kv_cache",
            &format!(
                "used {}/{} | block {} | dtype {} | quant {:?}",
                metrics.kv_cache.used_blocks,
                metrics.kv_cache.total_blocks,
                metrics.kv_cache.block_size,
                metrics.kv_cache.dtype,
                metrics.kv_cache.quant_mode
            ),
        ),
        kv(
            "prefix_cache",
            &format!(
                "probes {} | hits {} | misses {} | hit tokens {} | miss tokens {}",
                metrics.kv_cache.counters.probes,
                metrics.kv_cache.counters.hits,
                metrics.kv_cache.counters.misses,
                metrics.kv_cache.counters.hit_tokens,
                metrics.kv_cache.counters.miss_tokens
            ),
        ),
    ]
}

fn thought_lines(thoughts: &[String]) -> Vec<String> {
    thoughts
        .iter()
        .flat_map(|thought| thought.lines())
        .map(|line| format!("{THOUGHT}{ITALIC}{line}{RESET}"))
        .collect()
}

fn kv(key: &str, value: &str) -> String {
    format!("{DIM}{key:<20}{RESET} {value}")
}

fn ms(value: f64) -> String {
    format!("{value:.3}")
}

fn highlight(line: &str) -> String {
    if line.contains("status: pass") {
        return paint(line, GREEN);
    }
    if line.contains("status: fail") || line.starts_with("warning:") {
        return paint(line, RED);
    }
    if line.starts_with("trace.") || line.contains(".trace:") {
        return paint(line, YELLOW);
    }
    line.to_owned()
}

fn paint(text: &str, color: &str) -> String {
    format!("{color}{text}{RESET}")
}
