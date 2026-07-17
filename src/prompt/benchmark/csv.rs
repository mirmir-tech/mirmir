use std::{fmt::Write as _, fs, path::Path};

use super::BenchmarkReport;
use crate::error::{Error, Result};

pub(super) fn write(path: &Path, report: &BenchmarkReport) -> Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let mut output = String::from(
        "sample,model,prompt_tokens,completion_tokens,finish_reason,elapsed_ms,ttft_ms,e2e_tokens_per_second,prefill_tokens_per_second,decode_tokens_per_second,prefill_ms,decode_ms\n",
    );
    for (index, sample) in report.samples.iter().enumerate() {
        write!(
            output,
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            index + 1,
            field(&report.model),
            sample.prompt_tokens,
            sample.completion_tokens,
            field(&sample.finish_reason),
            sample.elapsed_ms,
            optional(sample.ttft_ms),
            optional(sample.tokens_per_second),
            optional(sample.prefill_tokens_per_second),
            optional(sample.decode_tokens_per_second),
            optional(sample.prefill_ms),
            optional(sample.decode_ms),
        )
        .map_err(|error| Error::Config(format!("cannot format CSV output: {error}")))?;
        output.push('\n');
    }
    fs::write(path, output)?;
    Ok(())
}

fn optional(value: Option<f64>) -> String {
    value.map_or_else(String::new, |value| value.to_string())
}

fn field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_csv_text_fields() {
        assert_eq!(field("A, \"model\""), "\"A, \"\"model\"\"\"");
    }
}
