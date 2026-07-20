use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::Result;

const MODELS_ENDPOINT: &str = "https://huggingface.co/api/models";
const WHOAMI_ENDPOINT: &str = "https://huggingface.co/api/whoami-v2";

#[derive(Debug, Deserialize)]
pub struct HubModel {
    pub id: String,
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
}

pub struct HubPage {
    pub models: Vec<HubModel>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct HubConfig {
    #[serde(default)]
    pub architectures: Vec<String>,
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

const fn search_parameters<'a>(query: &'a str, limit: &'a str) -> [(&'static str, &'a str); 10] {
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
    #[must_use]
    pub fn model_class(&self) -> String {
        if self.config.architectures.is_empty() {
            return "unknown".to_owned();
        }
        self.config.architectures.join(", ")
    }

    #[must_use]
    pub const fn is_gated(&self) -> bool {
        !matches!(self.gated, serde_json::Value::Null | serde_json::Value::Bool(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_search_metadata_used_by_fit_estimator() -> Result<()> {
        let json = r#"[{
            "id":"Qwen/Qwen2.5-0.5B-Instruct",
            "downloads":42,
            "likes":7,
            "gated":false,
            "config":{"architectures":["Qwen2ForCausalLM"],"model_type":"qwen2"},
            "safetensors":{"parameters":{"BF16":494032768},"total":494032768},
            "tags":["transformers","safetensors","text-generation"]
        }]"#;
        let models: Vec<HubModel> = serde_json::from_str(json)?;
        assert_eq!(models[0].model_class(), "Qwen2ForCausalLM");
        assert_eq!(
            models[0].safetensors.as_ref().map(|value| value.parameters["BF16"]),
            Some(494_032_768)
        );
        assert!(!models[0].is_gated());
        Ok(())
    }

    #[test]
    fn extracts_the_next_page_cursor_from_link_header() {
        let header = reqwest::header::HeaderValue::from_static(
            "<https://huggingface.co/api/models?limit=20&cursor=next%3D%3D>; rel=\"next\"",
        );
        assert_eq!(parse_next_cursor(&header).as_deref(), Some("next=="));
    }

    #[test]
    fn searches_across_model_tasks_without_a_pipeline_filter() {
        let parameters = search_parameters("reranker", "20");

        assert!(parameters.iter().any(|(key, value)| *key == "search" && *value == "reranker"));
        assert!(!parameters.iter().any(|(key, _)| *key == "filter"));
    }
}
