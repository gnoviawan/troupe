use client::http::HttpError;
use rune::{Runtime, TextEdit};
use serde_json::{Value, json};

use super::{CatalogFetch, CatalogStatus};
use crate::State;
use crate::tests::{present, runtime_at, tap};
use model::settings::mcp_catalog::Search;

fn edit(rt: &mut Runtime<State>, f: impl FnOnce(&mut State)) {
    rt.update_if(move |s, _| {
        f(s);
        true
    });
    rt.frame(0.016);
    rt.frame(0.032);
}

fn sent(rt: &Runtime<State>, prefix: &str) -> Vec<String> {
    rt.state.mcp_ui.posted.iter().filter(|(p, _)| p.starts_with(prefix)).map(|(p, _)| p.clone()).collect()
}

fn row(name: &str, state: &str) -> Value {
    json!({ "name": name, "title": null, "description": format!("{name} does things"), "version": "1.0.0", "status": "active", "state": state, "reason": null, "defaultOption": null, "repositoryUrl": null, "hasIcon": false })
}

fn page(names: &[&str], next: Option<&str>, total: u64) -> Value {
    json!({ "items": names.iter().map(|n| row(n, "installable")).collect::<Vec<_>>(), "nextCursor": next, "total": total, "updatedAt": "2026-11-26T10:00:00Z", "sample": false })
}

fn status() -> Value {
    json!({ "updatedAt": "2026-11-26T10:00:00Z", "sample": false, "servers": 3, "sync": { "state": "idle" } })
}

fn answer(rt: &mut Runtime<State>, tag: CatalogFetch, result: Result<Value, HttpError>) {
    edit(rt, move |s| s.catalog_fetched(tag, result));
}

fn generation(rt: &Runtime<State>) -> u64 {
    rt.state.mcp_ui.catalog.generation
}

fn opened() -> Runtime<State> {
    let mut rt = runtime_at((1280.0, 800.0));
    edit(&mut rt, |s| s.open_settings(Some(crate::settings_mcp::PAGE)));
    rt
}

#[test]
fn the_page_searches_the_catalog_and_reads_its_status() {
    let mut rt = opened();
    assert_eq!(sent(&rt, "/mcp-catalog/search"), ["/mcp-catalog/search?limit=50"]);
    assert_eq!(sent(&rt, "/mcp-catalog/status").len(), 1);
    assert!(present(&rt, "catalog-loading"));
    let g = generation(&rt);
    answer(&mut rt, CatalogFetch::Search { generation: g, query: String::new(), append: false }, Ok(page(&["io.github/alpha", "io.github/beta", "com.x/gamma"], None, 3)));
    answer(&mut rt, CatalogFetch::Status(1), Ok(status()));
    for name in ["io.github/alpha", "io.github/beta", "com.x/gamma"] {
        assert!(present(&rt, &format!("catalog-{name}")), "a card for {name}");
    }
    assert!(present(&rt, "mcp-catalog-sync"));
    assert_eq!(sent(&rt, "/mcp-catalog/search").len(), 1, "a catalog the rows match is not searched again");
}

#[test]
fn typing_searches_after_the_debounce_and_an_older_answer_is_dropped() {
    let mut rt = opened();
    let first = generation(&rt);
    edit(&mut rt, |s| s.mcp_ui.catalog.query = TextEdit::new("git hub"));
    edit(&mut rt, |s| {
        let now = s.now;
        s.catalog_tick(now);
    });
    assert_eq!(sent(&rt, "/mcp-catalog/search").len(), 1, "nothing before the debounce");
    edit(&mut rt, |s| {
        s.now += 0.3;
        let now = s.now;
        s.catalog_tick(now);
    });
    assert_eq!(sent(&rt, "/mcp-catalog/search").last().map(String::as_str), Some("/mcp-catalog/search?query=git%20hub&limit=50"));
    answer(&mut rt, CatalogFetch::Search { generation: first, query: String::new(), append: false }, Ok(page(&["old/one"], None, 1)));
    assert!(rt.state.mcp_ui.catalog.search.shown().is_none(), "the first search's answer came too late");
    let g = generation(&rt);
    answer(&mut rt, CatalogFetch::Search { generation: g, query: "git hub".into(), append: false }, Ok(page(&["io.github/hub"], None, 1)));
    assert!(present(&rt, "catalog-io.github/hub"));
}

#[test]
fn nine_words_ask_for_fewer_without_a_request() {
    let mut rt = opened();
    edit(&mut rt, |s| s.mcp_ui.catalog.query = TextEdit::new("a b c d e f g h i"));
    edit(&mut rt, |s| {
        let now = s.now;
        s.catalog_tick(now);
        s.now += 0.3;
        let now = s.now;
        s.catalog_tick(now);
    });
    assert_eq!(rt.state.mcp_ui.catalog.search, Search::TooManyTerms);
    assert_eq!(sent(&rt, "/mcp-catalog/search").len(), 1);
    assert!(present(&rt, "too-many"));
}

#[test]
fn load_more_appends_the_next_page() {
    let mut rt = opened();
    let g = generation(&rt);
    answer(&mut rt, CatalogFetch::Search { generation: g, query: String::new(), append: false }, Ok(page(&["a/one", "a/two"], Some("c2"), 3)));
    tap(&mut rt, "load-more");
    assert_eq!(sent(&rt, "/mcp-catalog/search").last().map(String::as_str), Some("/mcp-catalog/search?cursor=c2&limit=50"));
    answer(&mut rt, CatalogFetch::Search { generation: g, query: String::new(), append: true }, Ok(page(&["a/three"], None, 3)));
    let rows: Vec<String> = rt.state.mcp_ui.catalog.search.shown().unwrap().rows.iter().map(|r| r.name.clone()).collect();
    assert_eq!(rows, ["a/one", "a/two", "a/three"]);
    assert!(!present(&rt, "load-more"));
}

#[test]
fn a_refused_sync_change_puts_the_switch_back_and_says_why() {
    let mut rt = opened();
    answer(&mut rt, CatalogFetch::Status(1), Ok(status()));
    assert!(present(&rt, "mcp-catalog-sync-switch"));
    // The switch sits under the cards, below the fold of an 800 px window.
    edit(&mut rt, |s| s.catalog_set_sync(false));
    let body = rt.state.mcp_ui.posted.iter().rfind(|(p, _)| p == "/mcp-catalog/sync").map(|(_, b)| b.clone());
    assert_eq!(body, Some(json!({ "enabled": false })));
    assert!(matches!(rt.state.mcp_ui.catalog.status, CatalogStatus::Ready { requested: Some(false), .. }));
    let refused = HttpError::Refused { status: 403, code: "FORBIDDEN".into(), message: "no".into() };
    answer(&mut rt, CatalogFetch::Sync(2), Err(refused));
    assert!(matches!(rt.state.mcp_ui.catalog.status, CatalogStatus::Ready { requested: None, .. }));
    assert!(rt.state.toasts.visible().iter().any(|t| t.title == "Only the host can change catalog sync."));
}
