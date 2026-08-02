use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use crate::error::{Error, Result};

pub(super) const FILENAME: &str = "model.safetensors.index.json";
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct SafeTensorsIndex {
    weight_map: BTreeMap<String, String>,
}

pub(super) fn validate_size(size: usize) -> Result<()> {
    if size == 0 {
        return Err(Error::Config(format!("`{FILENAME}` is empty")));
    }
    if size > MAX_BYTES {
        return Err(Error::Config(format!(
            "`{FILENAME}` exceeds the {MAX_BYTES} byte preflight limit"
        )));
    }
    Ok(())
}

pub(super) fn shard_files(bytes: &[u8], available: &[String]) -> Result<Vec<String>> {
    validate_size(bytes.len())?;
    let index: SafeTensorsIndex = serde_json::from_slice(bytes)?;
    if index.weight_map.is_empty() {
        return Err(Error::Config(format!("`{FILENAME}` has an empty weight map")));
    }
    let available = available.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let files = index.weight_map.into_values().collect::<BTreeSet<_>>();
    if let Some(missing) = files.iter().find(|file| !available.contains(file.as_str())) {
        return Err(Error::Config(format!("`{FILENAME}` references missing shard `{missing}`")));
    }
    Ok(files.into_iter().collect())
}
