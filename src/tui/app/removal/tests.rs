use super::{
    super::{CatalogRow, LocalRow},
    *,
};

#[test]
fn external_cache_model_opens_removal_confirmation() {
    let mut app = App::new(true);
    app.local_models.push(LocalRow::from(proto::LocalModelInfo {
        id: "Qwen--External".to_owned(),
        repo_id: "Qwen/External".to_owned(),
        revision: "main".to_owned(),
        commit: "abc123".to_owned(),
        path: "/hf/models--Qwen--External/snapshots/abc123".to_owned(),
        state: "available".to_owned(),
        recent_rank: None,
        selector: "/hf/models--Qwen--External/snapshots/abc123".to_owned(),
        managed: false,
        ..Default::default()
    }));

    app.open_remove_dialog();

    let dialog = app.remove_dialog.expect("confirmation should open");
    assert_eq!(dialog.repo_id, "Qwen/External");
    assert_eq!(dialog.source, CacheOrigin::HfCache);
}

fn variant(name: &str, downloaded: bool) -> proto::CatalogVariant {
    proto::CatalogVariant {
        name: name.to_owned(),
        downloaded,
        memory_fit: "fits".to_owned(),
        ..Default::default()
    }
}

fn local(repo_id: &str) -> LocalRow {
    LocalRow::from(proto::LocalModelInfo {
        id: repo_id.replace('/', "--").replace(':', "@"),
        repo_id: repo_id.to_owned(),
        state: "available".to_owned(),
        managed: true,
        ..Default::default()
    })
}

/// Removing one variant of a search result leaves the other installed
/// one downloaded and listed; removing the repository takes every one.
#[test]
fn a_removal_forgets_only_what_it_removed() {
    let mut app = App::new(true);
    app.catalog.push(CatalogRow::from(proto::CatalogModel {
        id: "owner/m-GGUF".to_owned(),
        variants: vec![variant("Q4_K_M", true), variant("Q8_0", true)],
        default_variant: Some("Q8_0".to_owned()),
        downloaded: true,
        local_source: "mirmir".to_owned(),
        ..Default::default()
    }));
    app.local_models = vec![local("owner/m-GGUF:Q4_K_M"), local("owner/m-GGUF:Q8_0")];
    app.apply_removal("Owner/M-gguf:q8_0");
    let model = &app.catalog[0];
    assert!(model.variants[0].downloaded && !model.variants[1].downloaded);
    assert!(!model.downloaded && model.local_source == LocalSource::Remote);
    let listed = app.local_models.iter().map(|model| model.repo_id.as_str()).collect::<Vec<_>>();
    assert_eq!(listed, ["owner/m-GGUF:Q4_K_M"]);
    app.apply_removal("owner/m-GGUF");
    assert!(app.catalog[0].variants.iter().all(|variant| !variant.downloaded));
    assert!(app.local_models.is_empty());
}
