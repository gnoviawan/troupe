#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;

use super::*;

fn overlay_from(document: Value) -> TroupeOverlay {
    parse_overlay(&document).unwrap()
}

fn counting_fetch(body: Result<Value, String>) -> (OverlayFetch, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let fetch: OverlayFetch = Arc::new(move || {
        seen.fetch_add(1, Ordering::SeqCst);
        let body = body
            .clone()
            .map(|value| serde_json::to_vec(&value).unwrap());
        Box::pin(async move { body })
    });
    (fetch, calls)
}

fn remote_document() -> Value {
    json!({
        "schemaVersion": 1,
        "overrides": [{ "id": "gemini", "name": "Gemini CLI", "brandColor": { "light": "#112233", "dark": "#445566" } }],
        "agents": [{
            "id": "remote-only",
            "name": "Remote Only",
            "version": "1.0.0",
            "description": "from the overlay",
            "distribution": { "npx": { "package": "remote-only@1.0.0" } }
        }]
    })
}

#[test]
fn bundled_overlay_carries_oh_my_pi_and_the_brand_colours() {
    let overlay = bundled_overlay();
    let omp = overlay
        .agents
        .iter()
        .find(|agent| agent.id == "oh-my-pi")
        .expect("oh-my-pi ships in the bundled overlay");
    assert_eq!(
        omp.distribution["binary"]["linux-x86_64"],
        json!({ "cmd": "omp", "args": ["acp"] })
    );
    assert!(omp.distribution["binary"]["linux-x86_64"]
        .get("archive")
        .is_none());
    let claude = overlay
        .overrides
        .iter()
        .find(|entry| entry.id == "claude-acp")
        .expect("claude-acp override");
    assert_eq!(
        claude.brand_color,
        Some(AgentBrandColor {
            light: "#C15F3C".to_string(),
            dark: "#D97757".to_string(),
            gradient: None,
        })
    );
}

#[test]
fn an_unknown_schema_version_is_refused() {
    let error = parse_overlay(&json!({ "schemaVersion": 2, "overrides": [] })).unwrap_err();
    assert_eq!(
        error.to_string(),
        "overlay schemaVersion 2 is not supported"
    );
    let error = parse_overlay(&json!({ "overrides": [] })).unwrap_err();
    assert_eq!(error.to_string(), "overlay has no schemaVersion");
}

#[test]
fn bad_entries_and_fields_are_dropped_one_by_one() {
    let big_icon = format!(
        "<svg viewBox=\"0 0 1 1\">{}</svg>",
        "x".repeat(MAX_ICON_BYTES)
    );
    let overlay = overlay_from(json!({
        "schemaVersion": 1,
        "overrides": [
            { "id": "../escape", "name": "Nope" },
            { "id": "a", "name": "   ", "brandColor": { "light": "red", "dark": "#000000" } },
            { "id": "b", "icon": "<svg width=\"4\"><path/></svg>" },
            { "id": "c", "icon": big_icon },
            { "id": "d", "hidden": true, "icon": "<svg viewBox=\"0 0 4 4\"><path/></svg>" },
            { "id": "d", "name": "Second d" }
        ],
        "agents": [
            { "id": "no-distribution", "name": "x" },
            { "id": "..", "distribution": { "npx": { "package": "x@1.0.0" } } },
            { "id": "ok", "distribution": { "npx": { "package": "ok@1.0.0" } } }
        ]
    }));
    assert_eq!(
        overlay.overrides,
        vec![
            AgentOverride {
                id: "a".into(),
                name: None,
                hidden: false,
                icon: None,
                brand_color: None
            },
            AgentOverride {
                id: "b".into(),
                name: None,
                hidden: false,
                icon: None,
                brand_color: None
            },
            AgentOverride {
                id: "c".into(),
                name: None,
                hidden: false,
                icon: None,
                brand_color: None
            },
            AgentOverride {
                id: "d".into(),
                name: None,
                hidden: true,
                icon: Some("<svg viewBox=\"0 0 4 4\"><path/></svg>".into()),
                brand_color: None,
            },
        ]
    );
    let ids: Vec<&str> = overlay
        .agents
        .iter()
        .map(|agent| agent.id.as_str())
        .collect();
    assert_eq!(ids, ["ok"]);
    assert_eq!(overlay.agents[0].name, "ok");
}

#[test]
fn a_gradient_needs_two_hex_stops_per_theme() {
    let brand = parse_brand_color(&json!({
        "light": "#1C69FF",
        "dark": "#4796E3",
        "gradient": { "light": ["#1C69FF", "#9168C0"], "dark": ["#4796E3"] }
    }));
    assert_eq!(brand, None);
}

#[test]
fn the_presentation_serializes_in_camel_case_without_empty_fields() {
    let resolved = ResolvedOverlay {
        overlay: Arc::new(overlay_from(remote_document())),
        source: OverlaySource::Bundled,
        fetched_at: None,
    };
    assert_eq!(
        serde_json::to_value(resolved.presentation()).unwrap(),
        json!({
            "overrides": [{ "id": "gemini", "name": "Gemini CLI", "brandColor": { "light": "#112233", "dark": "#445566" } }],
            "source": "bundled"
        })
    );
}

#[tokio::test]
async fn a_refresh_installs_and_caches_the_network_copy() {
    let root = tempfile::tempdir().unwrap();
    let (fetch, calls) = counting_fetch(Ok(remote_document()));
    let service = TroupeOverlayService::with_fetch(root.path(), fetch);
    assert_eq!(service.current().source, OverlaySource::Bundled);

    assert!(service.refresh_if_stale().await);
    let current = service.current();
    assert_eq!(current.source, OverlaySource::Network);
    assert_eq!(current.overlay.agents[0].id, "remote-only");

    // Fresh within the TTL: no second fetch.
    assert!(!service.refresh_if_stale().await);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // A restart serves the cache without a fetch.
    let (fetch, calls) = counting_fetch(Err("offline".to_string()));
    let reopened = TroupeOverlayService::with_fetch(root.path(), fetch);
    assert_eq!(reopened.current().source, OverlaySource::Cache);
    assert_eq!(
        reopened.current().overlay.overrides[0].name.as_deref(),
        Some("Gemini CLI")
    );
    assert!(!reopened.wants_refresh());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_failed_refresh_keeps_the_bundled_copy_and_gates_retries() {
    let root = tempfile::tempdir().unwrap();
    let (fetch, calls) = counting_fetch(Err("offline".to_string()));
    let service = TroupeOverlayService::with_fetch(root.path(), fetch);

    assert!(!service.refresh_if_stale().await);
    assert!(!service.refresh_if_stale().await);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!service.wants_refresh());
    let current = service.current();
    assert_eq!(current.source, OverlaySource::Bundled);
    assert!(current
        .overlay
        .agents
        .iter()
        .any(|agent| agent.id == "oh-my-pi"));
}

#[tokio::test]
async fn a_newer_schema_from_the_network_is_ignored() {
    let root = tempfile::tempdir().unwrap();
    let (fetch, _) = counting_fetch(Ok(
        json!({ "schemaVersion": 2, "overrides": [{ "id": "x", "hidden": true }] }),
    ));
    let service = TroupeOverlayService::with_fetch(root.path(), fetch);

    assert!(!service.refresh_if_stale().await);
    assert_eq!(service.current().source, OverlaySource::Bundled);
    assert!(!root.path().join(CACHE_FILENAME).exists());
}

#[test]
fn a_cache_with_an_unknown_schema_falls_back_to_the_bundled_copy() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join(CACHE_FILENAME),
        serde_json::to_vec(&json!({
            "fetchedAt": chrono::Utc::now().to_rfc3339(),
            "document": { "schemaVersion": 9 }
        }))
        .unwrap(),
    )
    .unwrap();
    let (fetch, _) = counting_fetch(Err("offline".to_string()));
    let service = TroupeOverlayService::with_fetch(root.path(), fetch);
    assert_eq!(service.current().source, OverlaySource::Bundled);
}

#[test]
fn icons_that_restyle_the_app_or_reach_the_network_are_refused() {
    let cases = [
        (
            r#"<svg viewBox="0 0 4 4"><style>body{display:none}</style><path d="M0 0h4v4z"/></svg>"#,
            "a <style> element",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><image href="https://evil.example/ping.png"/></svg>"#,
            "an <image> element",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><a href="https://evil.example"><path d="M0 0h4v4z"/></a></svg>"#,
            "an <a> element",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><a><path d="M0 0h4v4z"/></a></svg>"#,
            "an <a> element",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><foreignObject><div>x</div></foreignObject></svg>"#,
            "a <foreignObject> element",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><script>alert(1)</script></svg>"#,
            "a <script> element",
        ),
        (
            r#"<svg viewBox="0 0 4 4" onload="alert(1)"><path d="M0 0h4v4z"/></svg>"#,
            "an on* event handler",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><use xlink:href="https://evil.example/s.svg#x"/></svg>"#,
            "an href that is not a #fragment",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><use href = 'data:image/svg+xml,x'/></svg>"#,
            "an href that is not a #fragment",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><path fill="url(https://evil.example/p)" d="M0 0h4v4z"/></svg>"#,
            "a url() that is not a #fragment",
        ),
        (
            r#"<svg viewBox="0 0 4 4"><path style="fill:URL( 'https://evil.example/p' )" d="M0 0h4v4z"/></svg>"#,
            "a url() that is not a #fragment",
        ),
    ];
    for (svg, reason) in cases {
        assert_eq!(icon_rejection(svg), Some(reason), "{svg}");
    }
}

#[test]
fn fragment_references_and_plain_shapes_are_accepted() {
    let svg = r##"<svg viewBox="0 0 24 24"><defs><linearGradient id="g"><stop offset="0" stop-color="#000"/></linearGradient><path id="p" d="M0 0h4v4z"/></defs><use href="#p"/><use xlink:href='#p'/><path fill="url(#g)" style="fill: url( '#g' )" d="M0 0h24v24z"/><title>Agent on top</title></svg>"##;
    assert_eq!(icon_rejection(svg), None);
    let animate = r#"<svg viewBox="0 0 4 4"><animate attributeName="opacity"/></svg>"#;
    assert_eq!(icon_rejection(animate), None);
}

#[test]
fn a_refused_icon_keeps_the_rest_of_its_override() {
    let overlay = overlay_from(json!({
        "schemaVersion": 1,
        "overrides": [{
            "id": "gemini",
            "name": "Gemini",
            "icon": "<svg viewBox=\"0 0 4 4\"><image href=\"https://evil.example/p.png\"/></svg>",
            "brandColor": { "light": "#112233", "dark": "#445566" }
        }]
    }));
    assert_eq!(
        overlay.overrides,
        vec![AgentOverride {
            id: "gemini".into(),
            name: Some("Gemini".into()),
            hidden: false,
            icon: None,
            brand_color: Some(AgentBrandColor {
                light: "#112233".into(),
                dark: "#445566".into(),
                gradient: None,
            }),
        }]
    );
}
