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
