use super::*;

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
