use toml_edit::{Array, DocumentMut, Item, value};

use super::{automatic, parse};
use crate::error::{Error, Result};

pub(super) fn patch(document: &mut DocumentMut, key: &str, input: &str) -> Result<()> {
    let field = key.strip_prefix("server.").unwrap_or(key);
    match field {
        "http_bind" => document["server"]["http_bind"] = value(input),
        "allow_remote" | "web_enabled" => {
            document["server"][field] = value(parse::<bool>(key, input)?);
        },
        "cors_origins" => document["server"]["cors_origins"] = origins(input),
        "body_limit_bytes" | "request_timeout_seconds" | "max_concurrency" => {
            document["server"][field] = value(parse::<i64>(key, input)?);
        },
        // `none`, `auto` or an empty value removes the setting: remote access
        // off, or the generated self-signed certificate.
        "remote_bind" | "remote_tls_cert" | "remote_tls_key" => {
            if automatic(input) || input.eq_ignore_ascii_case("off") {
                if let Some(server) = document["server"].as_table_like_mut() {
                    server.remove(field);
                }
            } else {
                document["server"][field] = value(input);
            }
        },
        _ => return Err(Error::Config(format!("unknown configuration key `{key}`"))),
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
