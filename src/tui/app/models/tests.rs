use super::{super::variant_size, *};

fn row(id: &str, compatibility: &str, memory_fit: &str) -> proto::CatalogModel {
    proto::CatalogModel {
        id: id.to_owned(),
        compatibility: compatibility.to_owned(),
        memory_fit: memory_fit.to_owned(),
        ..Default::default()
    }
}

/// The TUI orders rows as the server does: partially admitted models above
/// unknown ones, unsupported and unrecognized statuses last.
#[test]
fn catalog_rows_rank_by_the_servers_typed_order() {
    let mut app = App::new(true);
    let rows = [
        row("unrecognized", "someday", "fits"),
        row("unsupported", "unsupported", "fits"),
        row("unknown", "unknown", "fits"),
        row("partial-tight", "partial", "tight"),
        row("partial", "partial", "fits"),
        row("supported", "supported", "does_not_fit"),
    ];
    let response = proto::SearchModelsResponse {
        models: rows.to_vec(),
        ..Default::default()
    };
    app.apply_search_response(response, false);
    let order = app.catalog.iter().map(|model| model.id.as_str()).collect::<Vec<_>>();
    assert_eq!(
        order,
        ["supported", "partial", "partial-tight", "unknown", "unsupported", "unrecognized"]
    );
}

/// The server's fit and size spellings parse once into the catalog's types:
/// the chosen variant's over the row's, unknown ones as unknown.
#[test]
fn catalog_fit_and_size_parse_the_servers_spellings() {
    let app = App::new(true);
    let mut model = row("owner/m-GGUF", "unknown", "does_not_fit");
    assert_eq!(app.catalog_fit(&model), catalog::MemoryFit::DoesNotFit);
    let variant = proto::CatalogVariant {
        name: "Q4_K_M".to_owned(),
        size_kind: "estimated".to_owned(),
        weight_bytes: Some(4),
        memory_fit: "tight".to_owned(),
        ..Default::default()
    };
    model.variants = vec![variant.clone()];
    model.default_variant = Some("Q4_K_M".to_owned());
    assert_eq!(app.catalog_fit(&model), catalog::MemoryFit::Tight);
    assert_eq!(variant_size(&variant), catalog::VariantSize::Estimated(4));
    let exact = proto::CatalogVariant {
        size_kind: "exact".to_owned(),
        ..variant.clone()
    };
    assert_eq!(variant_size(&exact), catalog::VariantSize::Exact(4));
    let future = proto::CatalogVariant {
        size_kind: "someday".to_owned(),
        memory_fit: "someday".to_owned(),
        ..variant
    };
    assert_eq!(variant_size(&future), catalog::VariantSize::Unknown);
    model.variants = vec![future];
    assert_eq!(app.catalog_fit(&model), catalog::MemoryFit::Unknown);
    let target = app.pull_target(&model).ok();
    assert_eq!(target.as_deref(), Some("owner/m-GGUF:Q4_K_M"));
}

fn variant(name: &str, memory_fit: &str, downloaded: bool) -> proto::CatalogVariant {
    proto::CatalogVariant {
        name: name.to_owned(),
        memory_fit: memory_fit.to_owned(),
        downloaded,
        ..Default::default()
    }
}

/// A search result for `owner/m-GGUF` offering `variants`, its default
/// `Q8_0`, searched for.
fn searched(variants: Vec<proto::CatalogVariant>) -> App {
    let mut app = App::new(true);
    app.search_query = "m-GGUF".to_owned();
    let downloaded = variants.iter().any(|variant| variant.name == "Q8_0" && variant.downloaded);
    app.catalog.push(proto::CatalogModel {
        id: "owner/m-GGUF".to_owned(),
        compatibility: "supported".to_owned(),
        memory_fit: "fits".to_owned(),
        default_variant: Some("Q8_0".to_owned()),
        downloaded,
        local_source: if downloaded {
            "mirmir"
        } else {
            "remote"
        }
        .to_owned(),
        variants,
        ..Default::default()
    });
    app
}

/// Choosing a variant that does not fit keeps its row listed, so the
/// choice can be taken back.
#[test]
fn choosing_a_variant_that_does_not_fit_keeps_the_row() {
    let mut app =
        searched(vec![variant("Q4_K_M", "fits", false), variant("Q8_0", "does_not_fit", false)]);
    app.cycle_variant(-1);
    assert_eq!(app.visible_catalog_count(), 1);
    app.cycle_variant(1);
    assert_eq!(app.catalog_fit(&app.catalog[0]), catalog::MemoryFit::DoesNotFit);
    assert_eq!(app.visible_catalog_count(), 1);
    app.cycle_variant(-1);
    assert_eq!(app.catalog_variants.get("owner/m-GGUF").map(String::as_str), Some("Q4_K_M"));
}

/// Removing and loading from a search result take the chosen variant: a
/// variant not downloaded is neither, though the default is; one that is
/// downloaded is, alone, though the default is not.
#[test]
fn search_actions_take_the_chosen_variant() {
    let mut app = searched(vec![variant("Q4_K_M", "fits", false), variant("Q8_0", "fits", true)]);
    app.cycle_variant(-1);
    app.open_remove_dialog();
    assert!(app.remove_dialog.is_none());
    assert!(app.load_target().is_none());
    app.cycle_variant(1);
    app.open_remove_dialog();
    let dialog = app.remove_dialog.take().map(|dialog| dialog.repo_id);
    assert_eq!(dialog.as_deref(), Some("owner/m-GGUF:Q8_0"));
    let mut app = searched(vec![variant("Q4_K_M", "fits", true), variant("Q8_0", "fits", false)]);
    app.cycle_variant(-1);
    app.open_remove_dialog();
    let dialog = app.remove_dialog.take().map(|dialog| (dialog.repo_id, dialog.source));
    assert_eq!(dialog, Some(("owner/m-GGUF:Q4_K_M".to_owned(), "MIRMIR".to_owned())));
    let target = app.load_target().map(|target| (target.selector, target.config_id));
    let expected = ("owner/m-GGUF:Q4_K_M".to_owned(), "owner--m-GGUF@Q4_K_M".to_owned());
    assert_eq!(target, Some(expected));
}

/// A GGUF repository another tool's cache holds whole loads and removes as
/// that repository, whichever variant is chosen; it downloads no variant.
#[test]
fn a_repository_in_another_tools_cache_acts_whole() {
    let mut app = searched(vec![variant("Q4_K_M", "fits", false), variant("Q8_0", "fits", false)]);
    "hf_cache".clone_into(&mut app.catalog[0].local_source);
    app.cycle_variant(-1);
    app.open_remove_dialog();
    let dialog = app.remove_dialog.take().map(|dialog| (dialog.repo_id, dialog.source));
    assert_eq!(dialog, Some(("owner/m-GGUF".to_owned(), "HF CACHE".to_owned())));
    let target = app.load_target().map(|target| target.selector);
    assert_eq!(target.as_deref(), Some("owner/m-GGUF"));
    // The row says so whichever variant is chosen, and offers no download.
    assert_eq!(
        app.catalog_presence(&app.catalog[0]),
        super::super::CatalogPresence::ExternalCache
    );
    app.cycle_variant(1);
    assert_eq!(
        app.catalog_presence(&app.catalog[0]),
        super::super::CatalogPresence::ExternalCache
    );
}

/// A variant's pull or removal leaves what the row tells of its whole
/// repository: a partial download, another tool's cache, a row without
/// variants.
#[test]
fn a_variant_change_keeps_the_repositorys_own_state() {
    let mut app = searched(vec![variant("Q4_K_M", "fits", true), variant("Q8_0", "fits", false)]);
    "partial".clone_into(&mut app.catalog[0].local_source);
    app.catalog.push(proto::CatalogModel {
        id: "owner/whole".to_owned(),
        downloaded: true,
        local_source: "hf_cache".to_owned(),
        ..Default::default()
    });
    app.mark_downloaded("owner/m-GGUF:Q4_K_M", false);
    app.mark_downloaded("owner/whole:Q4_K_M", false);
    assert!(!app.catalog[0].variants[0].downloaded);
    assert_eq!(app.catalog[0].local_source, "partial");
    assert!(app.catalog[1].downloaded && app.catalog[1].local_source == "hf_cache");
    app.mark_downloaded("owner/m-GGUF", false);
    assert_eq!(app.catalog[0].local_source, "remote");
}
