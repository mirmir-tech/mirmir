use super::{App, Screen, rendered};
use crate::rpc::proto::{CatalogModel, LocalModelInfo};

#[test]
fn empty_search_renders_only_local_models() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.local_models.push(local_model("Qwen--Local", "Qwen/Local"));
    let mut missing = local_model("Missing--Hidden", "Missing/Hidden");
    missing.state = "missing".to_owned();
    app.local_models.push(missing);
    app.catalog.push(catalog_model("Remote/Hidden", ""));

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("LOCAL MODELS"));
    assert!(text.contains("Qwen--Local"));
    assert!(text.contains("AVAILABLE"));
    assert!(text.contains("▶ load"));
    assert!(!text.contains("Remote/Hidden"));
    assert!(!text.contains("Missing--Hidden"));
    assert!(!text.contains("HUGGING FACE RESULTS"));
    Ok(())
}

#[test]
fn unsupported_download_stays_visible_with_its_reason() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    let mut model = local_model("Org--Unsupported", "Org/Unsupported");
    model.loadable = false;
    model.load_unavailable_reason = "unsupported execution contract".to_owned();
    app.local_models.push(model);

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("Org--Unsupported"));
    assert!(text.contains("ERROR"));
    assert!(text.contains("unsupported execution contract"));
    Ok(())
}

#[test]
fn search_replaces_local_list_and_marks_cached_models() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.search_query = "Qwen".to_owned();
    app.editing_search = true;
    app.local_models.push(local_model("Local--Hidden", "Local/Hidden"));
    app.catalog.push(catalog_model("Qwen/Test", "hf_cache"));

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("HUGGING FACE RESULTS"));
    assert!(text.contains("Qwen/Test"));
    assert!(text.contains("downloaded"));
    assert!(!text.contains("Local--Hidden"));
    assert!(!text.contains("LOCAL MODELS"));
    Ok(())
}

#[test]
fn search_marks_mirmir_downloads() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.search_query = "Qwen".to_owned();
    app.editing_search = true;
    app.catalog.push(catalog_model("Qwen/Managed", "mirmir"));

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("Qwen/Managed"));
    assert!(text.contains("downloaded"));
    Ok(())
}

#[test]
fn incompatible_remote_models_are_hidden_until_requested() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.search_query = "encoder".to_owned();
    app.editing_search = true;
    let mut incompatible = catalog_model("BAAI/Encoder", "remote");
    incompatible.compatibility = "unsupported".to_owned();
    incompatible.memory_fit = "unknown".to_owned();
    app.catalog.push(incompatible);

    assert!(!rendered(&mut app, 120, 36)?.contains("BAAI/Encoder"));
    app.catalog_filter = super::super::app::CatalogFilter::All;
    assert!(rendered(&mut app, 120, 36)?.contains("BAAI/Encoder"));
    Ok(())
}

#[test]
fn unknown_remote_contracts_remain_visible_for_download() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.search_query = "reranker".to_owned();
    app.editing_search = true;
    let mut candidate = catalog_model("Alibaba-NLP/Reranker", "remote");
    candidate.compatibility = "unknown".to_owned();
    app.catalog.push(candidate);

    assert!(rendered(&mut app, 120, 36)?.contains("Alibaba-NLP/Reranker"));
    Ok(())
}

#[test]
fn local_gated_results_remain_visible_and_marked() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.search_query = "gated".to_owned();
    app.editing_search = true;
    let mut model = catalog_model("Org/Gated", "hf_cache");
    model.compatibility = "unsupported".to_owned();
    model.gated = true;
    app.catalog.push(model);

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("Org/Gated"));
    assert!(text.contains("downloaded"));
    Ok(())
}

#[test]
fn renders_download_percentage_and_binary_sizes() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.transfer_repo = Some("Qwen/Downloading".to_owned());
    app.transfer_phase = Some("downloading".to_owned());
    app.transfer_downloaded_bytes = 512 * 1_024 * 1_024;
    app.transfer_total_bytes = Some(1_024 * 1_024 * 1_024);
    app.action_message = Some("4 files".to_owned());

    let text = rendered(&mut app, 120, 36)?;
    assert!(text.contains("DOWNLOAD · DOWNLOADING"));
    assert!(text.contains("Qwen/Downloading"));
    assert!(text.contains("50.0%"));
    assert!(text.contains("512.00 MiB / 1.00 GiB"));
    Ok(())
}

fn local_model(id: &str, repo_id: &str) -> LocalModelInfo {
    LocalModelInfo {
        id: id.to_owned(),
        repo_id: repo_id.to_owned(),
        revision: "main".to_owned(),
        commit: "abc123".to_owned(),
        path: format!("/models/{id}"),
        state: "available".to_owned(),
        recent_rank: Some(0),
        selector: id.to_owned(),
        managed: true,
        loadable: true,
        model_class: "Qwen2ForCausalLM".to_owned(),
        ..Default::default()
    }
}

fn catalog_model(id: &str, local_source: &str) -> CatalogModel {
    CatalogModel {
        id: id.to_owned(),
        downloads: 42,
        likes: 7,
        gated: false,
        model_class: "Qwen2ForCausalLM".to_owned(),
        compatibility: "supported".to_owned(),
        memory_fit: "fits".to_owned(),
        estimated_weight_bytes: Some(1_000_000_000),
        estimated_required_bytes: Some(2_500_000_000),
        budget_bytes: Some(8_000_000_000),
        confidence: "medium".to_owned(),
        reason: "fits test budget".to_owned(),
        downloaded: local_source == "mirmir",
        local_source: local_source.to_owned(),
        ..Default::default()
    }
}

fn variant(
    name: &str,
    size_kind: &str,
    required: u64,
    fit: &str,
) -> crate::rpc::proto::CatalogVariant {
    crate::rpc::proto::CatalogVariant {
        name: name.to_owned(),
        size_kind: size_kind.to_owned(),
        weight_bytes: Some(required / 2),
        required_bytes: Some(required),
        memory_fit: fit.to_owned(),
        downloaded: false,
    }
}

/// A GGUF repository is one row showing its default quantization; ←/→
/// choose another, whose size and fit the row shows and whose `repo:VARIANT`
/// a download pulls.
#[test]
fn search_chooses_a_gguf_repositorys_quantization() -> Result<(), std::convert::Infallible> {
    let mut app = App::new(true);
    app.screen = Screen::Models;
    app.search_query = "gemma".to_owned();
    app.editing_search = true;
    let mut model = catalog_model("ggml-org/gemma-4-E4B-it-GGUF", "remote");
    model.compatibility = "unknown".to_owned();
    model.variants = vec![
        variant("Q4_0", "estimated", 6_000_000_000, "fits"),
        variant("Q8_0", "estimated", 10_000_000_000, "tight"),
        variant("BF16", "exact", 17_000_000_000, "does_not_fit"),
    ];
    model.default_variant = Some("Q4_0".to_owned());
    app.catalog.push(model);

    let text = rendered(&mut app, 140, 36)?;
    assert!(text.contains("ggml-org/gemma-4-E4B-it-GGUF") && text.contains("‹Q4_0› 1/3"));
    // `~` marks a size estimated before the files are listed.
    assert!(text.contains("~5.58 GiB"));
    assert!(text.contains("FITS") && text.contains("←/→"));
    app.cycle_variant(1);
    let text = rendered(&mut app, 140, 36)?;
    assert!(text.contains("‹Q8_0› 2/3") && text.contains("TIGHT"));
    assert_eq!(app.pull_target(&app.catalog[0]), "ggml-org/gemma-4-E4B-it-GGUF:Q8_0");
    // Downloaded since the search, from any client: the local list says so.
    let downloaded =
        local_model("ggml-org--gemma-4-E4B-it-GGUF--Q8_0", "ggml-org/gemma-4-E4B-it-GGUF:Q8_0");
    app.local_models.push(downloaded);
    assert!(rendered(&mut app, 140, 36)?.contains("DOWNLOADED"));
    // The last quantization stays chosen past the end.
    app.cycle_variant(1);
    app.cycle_variant(1);
    assert_eq!(
        app.chosen_variant(&app.catalog[0]).map(|variant| variant.name.as_str()),
        Some("BF16")
    );
    Ok(())
}
