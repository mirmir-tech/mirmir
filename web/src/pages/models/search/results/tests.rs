use super::{PageRequest, Results, Settled};
use crate::types::catalog::{CatalogModel, CatalogResults};

fn answer(ids: &[&str], cursor: Option<&str>) -> CatalogResults {
    CatalogResults {
        models: ids
            .iter()
            .map(|id| CatalogModel {
                id: (*id).to_owned(),
                ..Default::default()
            })
            .collect(),
        next_cursor: cursor.map(ToOwned::to_owned),
    }
}

fn ids(results: &Results) -> Vec<&str> {
    results.models().iter().map(|model| model.id.as_str()).collect()
}

/// The answer for `old`, held until `new` has answered, neither replaces
/// the results for `new` nor ends its loading.
#[test]
fn an_older_search_answer_changes_nothing() {
    let mut results = Results::default();
    let old = results.search("old").expect("a request");
    let new = results.search("new").expect("a request");
    assert!(!results.current(old) && results.current(new));

    assert_eq!(results.found(new, "new", Ok(answer(&["new/model"], None))), Settled::Applied);
    assert_eq!(results.found(old, "old", Ok(answer(&["old/model"], None))), Settled::Stale);
    assert_eq!(ids(&results), ["new/model"]);
    assert!(!results.loading());
}

/// A late answer cannot end the loading of the search that superseded it,
/// and its failure is not reported.
#[test]
fn an_older_search_neither_ends_loading_nor_reports() {
    let mut results = Results::default();
    let old = results.search("old").expect("a request");
    let new = results.search("new").expect("a request");
    assert_eq!(results.found(old, "old", Err("HTTP 503".to_owned())), Settled::Stale);
    assert_eq!(results.found(old, "old", Ok(answer(&["old/model"], None))), Settled::Stale);
    assert!(results.loading() && results.models().is_empty());
    assert_eq!(
        results.found(new, "new", Err("HTTP 503".to_owned())),
        Settled::Failed("HTTP 503".to_owned())
    );
    assert!(!results.loading());
}

/// A page still in flight when a new search starts neither appends its
/// models nor replaces the new results' cursor.
#[test]
fn a_page_of_an_older_search_is_not_appended() {
    let mut results = Results::default();
    let first = results.search("old").expect("a request");
    results.found(first, "old", Ok(answer(&["old/1"], Some("old-2"))));
    let page = results.page().expect("a page");
    assert_eq!(
        page,
        PageRequest {
            ticket: page.ticket,
            query: "old".to_owned(),
            cursor: "old-2".to_owned()
        }
    );

    let new = results.search("new").expect("a request");
    results.found(new, "new", Ok(answer(&["new/1"], Some("new-2"))));
    assert_eq!(
        results.paged(page.ticket, Ok(answer(&["old/2"], Some("old-3")))),
        Settled::Stale
    );
    assert_eq!(ids(&results), ["new/1"]);
    assert_eq!(results.page().map(|page| page.cursor), Some("new-2".to_owned()));
}

/// Pages continue the query their results answer, one at a time, until
/// the cursor runs out; a failed page can be requested again.
#[test]
fn pages_continue_the_answered_query() {
    let mut results = Results::default();
    let ticket = results.search("  qwen ").expect("a request");
    assert!(results.page().is_none(), "no page while the search is in flight");
    results.found(ticket, "  qwen ", Ok(answer(&["a"], Some("2"))));

    let page = results.page().expect("a page");
    assert_eq!(page.query, "qwen");
    assert!(results.page().is_none(), "one page at a time");
    assert_eq!(
        results.paged(page.ticket, Err("HTTP 502".to_owned())),
        Settled::Failed("HTTP 502".to_owned())
    );

    let page = results.page().expect("the failed page again");
    assert_eq!(page.cursor, "2");
    assert!(results.has_more());
    assert_eq!(results.paged(page.ticket, Ok(answer(&["a", "b"], None))), Settled::Applied);
    assert_eq!(ids(&results), ["a", "b"]);
    assert!(!results.has_more());
    assert!(results.page().is_none(), "no cursor, no page");
}

/// Clearing the query clears the table at once and supersedes the search
/// in flight.
#[test]
fn an_empty_query_clears_and_supersedes() {
    let mut results = Results::default();
    let first = results.search("old").expect("a request");
    results.found(first, "old", Ok(answer(&["old/1"], Some("old-2"))));
    let pending = results.search("older").expect("a request");
    assert!(results.search("  ").is_none());
    assert!(results.models().is_empty() && !results.loading() && results.page().is_none());
    assert_eq!(results.found(pending, "older", Ok(answer(&["older/1"], None))), Settled::Stale);
    assert!(results.models().is_empty());
}
