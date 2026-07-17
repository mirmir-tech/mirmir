use libmir::{
    Engine,
    runtime::backend::{Backend, ModelHandle},
};

use super::output::{HeaderInput, header_lines};
use crate::{args::ChatArgs, error::CliError};

pub(super) async fn collect_report_context(
    args: &ChatArgs,
    backend: &Engine,
    handle: &ModelHandle,
    header: &HeaderInput<'_>,
) -> Result<(Vec<String>, Vec<String>), CliError> {
    let trace = if args.verbose || args.trace {
        Some(backend.model_trace(handle).await?)
    } else {
        None
    };
    let header_lines = if args.verbose || args.trace {
        let mut lines = header_lines(header);
        if let Some(trace) = trace.as_ref() {
            lines.push(format!("acceleration: {}", trace.acceleration.join("; ")));
            lines.push(format!("decode_attention: {}", trace.kv_cache.decode_attention));
        }
        lines
    } else {
        Vec::new()
    };
    let mut trace_lines = Vec::new();
    if args.trace
        && let Some(trace) = trace
    {
        trace_lines.extend(trace.summary_lines());
    }
    Ok((header_lines, trace_lines))
}
