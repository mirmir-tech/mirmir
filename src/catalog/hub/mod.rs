mod facts;
mod gguf;
mod weights;

use std::collections::BTreeMap;

pub use facts::{Features, HubFacts, encoding_named};
pub use gguf::{GgufRepository, GgufVariant, RepoFile};
pub use weights::is_base_safetensors;

impl HubModel {
    /// The GGUF variants its files offer; empty for other repositories.
    #[must_use]
    pub fn gguf_variants(&self) -> GgufRepository {
        let files = self
            .siblings
            .iter()
            .map(|sibling| RepoFile {
                path: sibling.rfilename.clone(),
                size: None,
            })
            .collect::<Vec<_>>();
        GgufRepository::from_files(&files)
    }
}
use serde::Deserialize;

use crate::{error::Result, model::class};

const MODELS_ENDPOINT: &str = "https://huggingface.co/api/models";
const WHOAMI_ENDPOINT: &str = "https://huggingface.co/api/whoami-v2";

#[derive(Debug, Deserialize)]
pub struct HubModel {
    pub id: String,
    #[serde(default)]
    pub library_name: Option<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub likes: u64,
    #[serde(default)]
    pub gated: serde_json::Value,
    #[serde(default)]
    pub config: HubConfig,
    #[serde(default)]
    pub safetensors: Option<SafeTensors>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// GGUF metadata the Hub reads from a GGUF repository's files.
    #[serde(default)]
    pub gguf: Option<HubGguf>,
    /// The repository's files, without sizes.
    #[serde(default)]
    pub siblings: Vec<Sibling>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HubGguf {
    /// Parameters of the model.
    #[serde(default)]
    pub total: Option<u64>,
    /// `general.architecture`.
    #[serde(default)]
    pub architecture: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Sibling {
    pub rfilename: String,
}

pub struct HubPage {
    pub models: Vec<HubModel>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HubConfig {
    #[serde(default)]
    pub architectures: Vec<String>,
    #[serde(default)]
    pub model_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SafeTensors {
    #[serde(default)]
    pub parameters: BTreeMap<String, u64>,
}

pub async fn search(
    client: &reqwest::Client,
    token: Option<&str>,
    query: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<HubPage> {
    let limit = limit.clamp(1, 50).to_string();
    let query = search_parameters(query, &limit);
    let mut request = client
        .get(MODELS_ENDPOINT)
        .query(&query)
        .header(reqwest::header::USER_AGENT, concat!("mirmir/", env!("CARGO_PKG_VERSION")));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(cursor) = cursor {
        request = request.query(&[("cursor", cursor)]);
    }
    let response = request.send().await?.error_for_status()?;
    let next_cursor = response.headers().get(reqwest::header::LINK).and_then(parse_next_cursor);
    Ok(HubPage {
        models: response.json().await?,
        next_cursor,
    })
}

const fn search_parameters<'a>(query: &'a str, limit: &'a str) -> [(&'static str, &'a str); 13] {
    [
        ("search", query),
        ("sort", "downloads"),
        ("direction", "-1"),
        ("limit", limit),
        ("expand[]", "safetensors"),
        ("expand[]", "config"),
        ("expand[]", "downloads"),
        ("expand[]", "likes"),
        ("expand[]", "gated"),
        ("expand[]", "tags"),
        ("expand[]", "library_name"),
        ("expand[]", "gguf"),
        ("expand[]", "siblings"),
    ]
}

fn parse_next_cursor(header: &reqwest::header::HeaderValue) -> Option<String> {
    let link = header.to_str().ok()?.split(',').find(|link| link.contains("rel=\"next\""))?;
    let url = link.split_once('<')?.1.split_once('>')?.0;
    reqwest::Url::parse(url)
        .ok()?
        .query_pairs()
        .find_map(|(key, value)| (key == "cursor").then(|| value.into_owned()))
}

pub async fn whoami(client: &reqwest::Client, token: &str) -> Result<String> {
    let response: serde_json::Value = client
        .get(WHOAMI_ENDPOINT)
        .header(reqwest::header::USER_AGENT, concat!("mirmir/", env!("CARGO_PKG_VERSION")))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("authenticated user")
        .to_owned())
}

impl HubModel {
    /// The configuration's `architectures`, else its `model_type`, else a
    /// GGUF repository's `general.architecture`: GGUF repositories rarely
    /// carry a `config.json`.
    #[must_use]
    pub fn model_class(&self) -> String {
        let architecture = self.gguf.as_ref().and_then(|gguf| gguf.architecture.as_deref());
        class::name(
            self.config.architectures.iter().map(String::as_str),
            [self.config.model_type.as_deref(), architecture],
        )
    }

    #[must_use]
    pub const fn is_gated(&self) -> bool {
        !matches!(self.gated, serde_json::Value::Null | serde_json::Value::Bool(false))
    }
}

#[cfg(test)]
mod tests;
