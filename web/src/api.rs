use gloo_net::http::{Request, Response};
use leptos::prelude::*;
use serde::{Serialize, de::DeserializeOwned};

use crate::state::RuntimeState;

const BASE: &str = "/api/mirmir/v1";

pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let response = Request::get(&format!("{BASE}{path}"))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    decode(response).await
}

pub async fn post<T: DeserializeOwned, B: Serialize>(
    state: RuntimeState,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let response = Request::post(&format!("{BASE}{path}"))
        .header("x-mirmir-csrf", &state.csrf.get_untracked())
        .json(body)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    decode(response).await
}

pub async fn post_empty<T: DeserializeOwned>(path: &str) -> Result<T, String> {
    let response = Request::post(&format!("{BASE}{path}"))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    decode(response).await
}

pub async fn raw_post<B: Serialize>(
    state: RuntimeState,
    path: &str,
    body: &B,
) -> Result<Response, String> {
    let response = Request::post(&format!("{BASE}{path}"))
        .header("x-mirmir-csrf", &state.csrf.get_untracked())
        .json(body)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.ok() {
        Ok(response)
    } else {
        Err(response_error(response).await)
    }
}

async fn decode<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    if !response.ok() {
        return Err(response_error(response).await);
    }
    response.json().await.map_err(|error| error.to_string())
}

async fn response_error(response: Response) -> String {
    let status = response.status();
    let body = response.json::<serde_json::Value>().await.unwrap_or_default();
    body.pointer("/error/message")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| format!("HTTP {status}"), ToOwned::to_owned)
}
