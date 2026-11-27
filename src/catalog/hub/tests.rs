use super::*;

#[test]
fn decodes_search_metadata_used_by_fit_estimator() -> Result<()> {
    let json = r#"[{
        "id":"Qwen/Qwen2.5-0.5B-Instruct",
        "library_name":"transformers",
        "downloads":42,
        "likes":7,
        "gated":false,
        "config":{"architectures":["Qwen2ForCausalLM"],"model_type":"qwen2"},
        "safetensors":{"parameters":{"BF16":494032768},"total":494032768},
        "tags":["transformers","safetensors","text-generation"]
    }]"#;
    let models: Vec<HubModel> = serde_json::from_str(json)?;
    assert_eq!(models[0].model_class(), "Qwen2ForCausalLM");
    assert_eq!(models[0].library_name.as_deref(), Some("transformers"));
    assert_eq!(
        models[0].safetensors.as_ref().map(|value| value.parameters["BF16"]),
        Some(494_032_768)
    );
    assert!(!models[0].is_gated());
    Ok(())
}

#[test]
fn names_a_class_without_architectures() -> Result<()> {
    let json = r#"[
        {"id":"a/typed","config":{"model_type":"qwen3"}},
        {"id":"a/blank","config":{"model_type":""},"gguf":{"architecture":"qwen35"}},
        {"id":"a/gguf","gguf":{"total":4000000000,"architecture":"gemma4"}},
        {"id":"a/bare"}
    ]"#;
    let models: Vec<HubModel> = serde_json::from_str(json)?;
    let classes = models.iter().map(HubModel::model_class).collect::<Vec<_>>();
    // An empty `model_type` falls back to the GGUF architecture.
    assert_eq!(classes, ["qwen3", "qwen35", "gemma4", "unknown"]);
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

/// The one test of base-model `SafeTensors` weights search, its GGUF
/// variants, pull and the header preflight share: root files a snapshot
/// loads, whole, sharded or consolidated, in any case; never an adapter, a
/// vision projector, a subdirectory's file or the index alone.
#[test]
fn tells_base_safetensors_weights_from_auxiliary_files() {
    let base = [
        "model.safetensors",
        "model-00001-of-00002.safetensors",
        "model-00002-of-00002.safetensors",
        "consolidated.safetensors",
        "Model.SafeTensors",
    ];
    for path in base {
        assert!(is_base_safetensors(path), "{path}");
    }
    let auxiliary = [
        "adapters/adapter_model.safetensors",
        "adapter_model.safetensors",
        "adapters.safetensors",
        "Adapter_Model.safetensors",
        "mmproj-model-f16.safetensors",
        "mmproj-patch-embedding.safetensors",
        "vision/model.safetensors",
        "original/model-00001-of-00002.safetensors",
        "model.safetensors.index.json",
        "model-Q4_K_M.gguf",
        "README.md",
    ];
    for path in auxiliary {
        assert!(!is_base_safetensors(path), "{path}");
    }
}
