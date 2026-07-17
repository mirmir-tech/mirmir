use std::io::{self, Write};

use libmir::runtime::metrics::GenerationMetrics;

use crate::error::CliError;

pub(super) fn write(metrics: &GenerationMetrics) -> Result<(), CliError> {
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    write_summary(
        &mut handle,
        &metrics.throughput.generated_active.to_string(),
        &metrics.throughput.prefill.to_string(),
        &metrics.throughput.decode.to_string(),
    )?;
    handle.flush()?;
    Ok(())
}

fn write_summary(
    handle: &mut impl Write,
    response: &str,
    prefill: &str,
    decode: &str,
) -> Result<(), CliError> {
    writeln!(handle)?;
    writeln!(handle, "+----------+----------------+")?;
    writeln!(handle, "| stage    | throughput     |")?;
    writeln!(handle, "+----------+----------------+")?;
    row(handle, "response", response)?;
    row(handle, "prefill", prefill)?;
    row(handle, "decode", decode)?;
    writeln!(handle, "+----------+----------------+")?;
    Ok(())
}

fn row(handle: &mut impl Write, stage: &str, throughput: &str) -> Result<(), CliError> {
    writeln!(handle, "| {stage:<8} | {throughput:<14} |")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_summary;

    #[test]
    fn summary_always_contains_prefill_and_decode() -> Result<(), Box<dyn std::error::Error>> {
        let mut output = Vec::new();
        write_summary(&mut output, "12.000 tok/s", "350.000 tok/s", "31.850 tok/s")?;
        let output = String::from_utf8(output)?;
        assert!(output.contains("| prefill  | 350.000 tok/s"));
        assert!(output.contains("| decode   | 31.850 tok/s"));
        Ok(())
    }
}
