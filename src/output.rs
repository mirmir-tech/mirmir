use std::io::{self, Read, Write};

use serde::Serialize;

use crate::error::Result;

pub fn line(value: impl AsRef<str>) -> Result<()> {
    lines([value.as_ref().to_owned()])
}

pub fn lines(values: impl IntoIterator<Item = String>) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for value in values {
        writeln!(handle, "{value}")?;
    }
    Ok(())
}

pub fn stream(value: &str) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    write!(handle, "{value}")?;
    handle.flush()?;
    Ok(())
}

pub fn diagnostic(value: impl AsRef<str>) -> Result<()> {
    let stderr = io::stderr();
    let mut handle = stderr.lock();
    writeln!(handle, "{}", value.as_ref())?;
    Ok(())
}

pub fn json(value: &impl Serialize) -> Result<()> {
    line(serde_json::to_string_pretty(value)?)
}

pub fn read_secret() -> Result<String> {
    let mut value = String::new();
    io::stdin().lock().read_to_string(&mut value)?;
    Ok(value)
}
