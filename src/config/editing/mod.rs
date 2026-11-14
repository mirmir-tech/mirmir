use std::fs;

use libmir::KvCacheDType;
use toml_edit::{DocumentMut, value};

mod server;

use super::{AppConfig, Store, file::write_text, store::config_writes};
use crate::error::{Error, Result};

/// Changes only together with the listener, through remote access.
const REMOTE_BIND: &str = "server.remote_bind";

impl Store {
    /// Validates and saves one setting. `server.remote_bind` is refused: it
    /// changes only with `mirmir remote enable` or `disable`, which start and
    /// stop the listener in the same transition.
    pub(crate) fn set_config_value(&self, key: &str, input: &str) -> Result<()> {
        if key == REMOTE_BIND {
            return Err(Error::Config(format!(
                "`{REMOTE_BIND}` changes together with the listener; use `mirmir remote enable \
                 [--bind ADDRESS]` or `mirmir remote disable`"
            )));
        }
        self.edit(key, input)
    }

    /// Persists the remote listener address, or that remote access is off.
    /// Only remote access calls this, inside its serialized transition.
    pub(crate) fn set_remote_bind(&self, bind: Option<&str>) -> Result<()> {
        self.edit(REMOTE_BIND, bind.unwrap_or("off"))
    }

    fn edit(&self, key: &str, input: &str) -> Result<()> {
        let _writes = config_writes();
        let raw = if self.paths.config_file.exists() {
            fs::read_to_string(&self.paths.config_file)?
        } else {
            toml::to_string_pretty(&AppConfig::default())?
        };
        let mut document = match raw.parse::<DocumentMut>() {
            Ok(document) => document,
            Err(error) => {
                return Err(Error::Config(format!(
                    "cannot edit {}: {error}",
                    self.paths.config_file.display()
                )));
            },
        };
        patch(&mut document, key, input.trim())?;
        let rendered = document.to_string();
        let config: AppConfig = toml::from_str(&rendered)?;
        config.validate()?;
        self.paths.ensure_config_dirs()?;
        write_text(&self.paths.config_file, &rendered, false)
    }
}

fn patch(document: &mut DocumentMut, key: &str, input: &str) -> Result<()> {
    match key {
        "default_model" => optional_string(document, "default_model", input),
        key if key.starts_with("server.") => server::patch(document, key, input)?,
        "runtime.kv_block_size" => optional_integer(document, "kv_block_size", key, input)?,
        "runtime.kv_blocks" => optional_integer(document, "kv_blocks", key, input)?,
        "runtime.max_batch_requests" => {
            optional_integer(document, "max_batch_requests", key, input)?;
        },
        "runtime.max_batch_tokens" => optional_integer(document, "max_batch_tokens", key, input)?,
        "runtime.prefill_batch_wait_us" => {
            optional_integer(document, "prefill_batch_wait_us", key, input)?;
        },
        "runtime.decode_batch_wait_us" => {
            optional_integer(document, "decode_batch_wait_us", key, input)?;
        },
        "runtime.decode_priority_burst" => {
            optional_integer(document, "decode_priority_burst", key, input)?;
        },
        "runtime.speculative_draft_tokens" => {
            optional_integer(document, "speculative_draft_tokens", key, input)?;
        },
        "runtime.memory_reserve_percent" => {
            optional_integer(document, "memory_reserve_percent", key, input)?;
        },
        "runtime.memory_reserve_bytes" => {
            optional_integer(document, "memory_reserve_bytes", key, input)?;
        },
        "runtime.vision_max_pixels" => {
            optional_integer(document, "vision_max_pixels", key, input)?;
        },
        "runtime.vision_attention_budget_bytes" => {
            optional_integer(document, "vision_attention_budget_bytes", key, input)?;
        },
        "runtime.vision_memory_percent" => {
            optional_integer(document, "vision_memory_percent", key, input)?;
        },
        "runtime.prefill_decode_policy" => {
            optional_runtime_string(document, "prefill_decode_policy", input);
        },
        "runtime.prefill_refill_policy" => {
            optional_runtime_string(document, "prefill_refill_policy", input);
        },
        "runtime.cached_prefill_policy" => {
            optional_runtime_string(document, "cached_prefill_policy", input);
        },
        #[cfg(target_os = "macos")]
        "runtime.metal_decode_reservation" => {
            optional_runtime_string(document, "metal_decode_reservation", input);
        },
        "runtime.kv_cache_dtype" => {
            if automatic(input) {
                if let Some(runtime) = document["runtime"].as_table_like_mut() {
                    runtime.remove("kv_cache_dtype");
                }
            } else {
                let dtype = parse::<KvCacheDType>(key, input)?;
                document["runtime"]["kv_cache_dtype"] = value(dtype.as_str());
            }
        },
        _ => return Err(Error::Config(format!("unknown configuration key `{key}`"))),
    }
    Ok(())
}

fn optional_runtime_string(document: &mut DocumentMut, field: &str, input: &str) {
    if automatic(input) {
        if let Some(runtime) = document["runtime"].as_table_like_mut() {
            runtime.remove(field);
        }
    } else {
        // AppConfig deserialization validates the typed value before any write.
        document["runtime"][field] = value(input);
    }
}

fn optional_string(document: &mut DocumentMut, key: &str, input: &str) {
    if automatic(input) {
        document.as_table_mut().remove(key);
    } else {
        document[key] = value(input);
    }
}

fn optional_integer(document: &mut DocumentMut, field: &str, key: &str, input: &str) -> Result<()> {
    if automatic(input) {
        if let Some(runtime) = document["runtime"].as_table_like_mut() {
            runtime.remove(field);
        }
    } else {
        document["runtime"][field] = value(parse::<i64>(key, input)?);
    }
    Ok(())
}

fn parse<T>(key: &str, input: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match input.parse() {
        Ok(value) => Ok(value),
        Err(error) => Err(Error::Config(format!("invalid value for `{key}`: {error}"))),
    }
}

/// Whether `input` clears an optional setting instead of choosing a value.
pub const fn automatic(input: &str) -> bool {
    input.is_empty() || input.eq_ignore_ascii_case("auto") || input.eq_ignore_ascii_case("none")
}

#[cfg(test)]
mod tests;
