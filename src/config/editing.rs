use std::fs;

use libmir::KvCacheDType;
use toml_edit::{Array, DocumentMut, Item, value};

use super::{AppConfig, Store, file::write_text};
use crate::error::{Error, Result};

impl Store {
    pub(crate) fn set_config_value(&self, key: &str, input: &str) -> Result<()> {
        let raw = if self.paths.config_file.exists() {
            fs::read_to_string(&self.paths.config_file)?
        } else {
            toml::to_string_pretty(&AppConfig::default())?
        };
        let mut document = raw.parse::<DocumentMut>().map_err(|error| {
            Error::Config(format!("cannot edit {}: {error}", self.paths.config_file.display()))
        })?;
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
        "server.http_bind" => document["server"]["http_bind"] = value(input),
        "server.allow_remote" => {
            document["server"]["allow_remote"] = value(parse::<bool>(key, input)?);
        },
        "server.web_enabled" => {
            document["server"]["web_enabled"] = value(parse::<bool>(key, input)?);
        },
        "server.cors_origins" => document["server"]["cors_origins"] = origins(input),
        "server.body_limit_bytes" => {
            document["server"]["body_limit_bytes"] = value(parse::<i64>(key, input)?);
        },
        "server.request_timeout_seconds" => {
            document["server"]["request_timeout_seconds"] = value(parse::<i64>(key, input)?);
        },
        "server.max_concurrency" => {
            document["server"]["max_concurrency"] = value(parse::<i64>(key, input)?);
        },
        "runtime.kv_block_size" => optional_integer(document, "kv_block_size", key, input)?,
        "runtime.kv_blocks" => optional_integer(document, "kv_blocks", key, input)?,
        "runtime.max_batch_requests" => {
            optional_integer(document, "max_batch_requests", key, input)?;
        },
        "runtime.max_batch_tokens" => optional_integer(document, "max_batch_tokens", key, input)?,
        "runtime.vision_max_pixels" => {
            optional_integer(document, "vision_max_pixels", key, input)?;
        },
        "runtime.vision_attention_budget_bytes" => {
            optional_integer(document, "vision_attention_budget_bytes", key, input)?;
        },
        "runtime.vision_memory_percent" => {
            optional_integer(document, "vision_memory_percent", key, input)?;
        },
        "runtime.kv_cache_dtype" => {
            if automatic(input) {
                if let Some(runtime) = document["runtime"].as_table_mut() {
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

fn optional_string(document: &mut DocumentMut, key: &str, input: &str) {
    if automatic(input) {
        document.as_table_mut().remove(key);
    } else {
        document[key] = value(input);
    }
}

fn optional_integer(document: &mut DocumentMut, field: &str, key: &str, input: &str) -> Result<()> {
    if automatic(input) {
        if let Some(runtime) = document["runtime"].as_table_mut() {
            runtime.remove(field);
        }
    } else {
        document["runtime"][field] = value(parse::<i64>(key, input)?);
    }
    Ok(())
}

fn origins(input: &str) -> Item {
    let mut origins = Array::new();
    for origin in input.split(',').map(str::trim).filter(|origin| !origin.is_empty()) {
        origins.push(origin);
    }
    value(origins)
}

fn parse<T>(key: &str, input: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    input
        .parse()
        .map_err(|error| Error::Config(format!("invalid value for `{key}`: {error}")))
}

const fn automatic(input: &str) -> bool {
    input.is_empty() || input.eq_ignore_ascii_case("auto") || input.eq_ignore_ascii_case("none")
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;
    use crate::config::Paths;

    static NEXT_EDIT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn patches_one_value_without_losing_comments() -> Result<()> {
        let store = store();
        store.paths.ensure_config_dirs()?;
        fs::write(
            &store.paths.config_file,
            "# keep this comment\nschema_version = 1\ndefault_model = \"old\"\n",
        )?;
        store.set_config_value("default_model", "new")?;
        let rendered = fs::read_to_string(&store.paths.config_file)?;
        assert!(rendered.contains("# keep this comment"));
        assert!(rendered.contains("default_model = \"new\""));
        assert_eq!(store.load()?.default_model.as_deref(), Some("new"));
        Ok(())
    }

    #[test]
    fn validates_before_replacing_the_file() -> Result<()> {
        let store = store();
        store.initialize()?;
        let before = fs::read_to_string(&store.paths.config_file)?;
        assert!(store.set_config_value("server.allow_remote", "perhaps").is_err());
        assert_eq!(fs::read_to_string(&store.paths.config_file)?, before);
        Ok(())
    }

    #[test]
    fn enables_the_embedded_web_foundation() -> Result<()> {
        let store = store();
        store.initialize()?;
        store.set_config_value("server.web_enabled", "true")?;
        assert!(store.load()?.server.web_enabled);
        Ok(())
    }

    #[test]
    fn selects_int8_kv_storage() -> Result<()> {
        let store = store();
        store.initialize()?;
        store.set_config_value("runtime.kv_cache_dtype", "int8_per_token_head")?;
        assert_eq!(store.load()?.runtime.kv_cache_dtype, Some(KvCacheDType::Int8PerTokenHead));
        store.set_config_value("runtime.kv_cache_dtype", "auto")?;
        assert_eq!(store.load()?.runtime.kv_cache_dtype, None);
        Ok(())
    }

    fn store() -> Store {
        let id = NEXT_EDIT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("mirmir-edit-{}-{id}", std::process::id()));
        Store::new(Paths::from_roots(
            root.join("config"),
            root.join("state"),
            &root.join("runtime"),
        ))
    }
}
