//! Drives a real Chromium through the engine: click, type and navigate a
//! local page, and the perimeter refusing a loopback page by default.
//! Needs a Chrome, Edge or Chromium (`launch::find`); CI always has one.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::perimeter::{InternalAddresses, Perimeter, Policy};
use super::{launch, ChromiumEngine, LocalSource, Tabs};
use crate::browser_automation::engine::assets;
use crate::browser_automation::engine::events::{NavPolicy, NavVerdict, TabEvent, TabEvents};
use crate::browser_automation::engine::profile::DataStore;
use crate::browser_automation::engine::{BrowserEngine, InputMode, KeyInput, Point};

const LABEL: &str = "tab-00000000-0000-4000-8000-000000000001";

const FIRST_PAGE: &str = r#"<!doctype html><html><head><title>first</title><style>
body { margin: 0; font: 16px sans-serif; }
#go { position: absolute; left: 20px; top: 20px; width: 100px; height: 40px; }
#field { position: absolute; left: 20px; top: 100px; width: 200px; height: 30px; }
#out { position: absolute; left: 20px; top: 160px; }
</style></head><body>
<button id="go" onclick="clicks++; show()">go</button>
<input id="field" oninput="show()">
<div id="out"></div>
<script>
let clicks = 0;
function show() {
  document.getElementById('out').textContent =
    'clicks:' + clicks + ' typed:' + document.getElementById('field').value;
}
show();
</script></body></html>"#;

const SECOND_PAGE: &str = "<!doctype html><title>second</title><p>second page</p>";

/// A loopback HTTP server for the two pages.
async fn serve_pages() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                continue;
            };
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).await.unwrap_or(0) == 1
                {
                    head.push(byte[0]);
                }
                let line = String::from_utf8_lossy(&head);
                let body = if line.contains("GET /next ") {
                    SECOND_PAGE
                } else {
                    FIRST_PAGE
                };
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            });
        }
    });
    base
}

/// The core's hop verdict, reduced to the address policy.
struct AddressOnly(Policy);

impl NavPolicy for AddressOnly {
    fn decide(&self, _tab: &str, url: &Url) -> NavVerdict {
        if self.0.allows_url(url) {
            NavVerdict::Allow
        } else {
            NavVerdict::Deny
        }
    }
}

struct Harness {
    tabs: Arc<Tabs>,
    engine: ChromiumEngine,
    events: mpsc::UnboundedReceiver<(String, TabEvent)>,
    _profiles: tempfile::TempDir,
}

/// A Chromium on a throwaway profile with one tab at `url`. `None` when
/// this machine has no browser (never on CI).
async fn harness(internal: InternalAddresses, url: &str) -> Option<Harness> {
    let Some(binary) = launch::find() else {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI runners must have a Chromium"
        );
        eprintln!("skipped: no Chrome, Edge or Chromium on this machine");
        return None;
    };
    let profiles = tempfile::tempdir().unwrap();
    let perimeter = Perimeter::start(internal).await.unwrap();
    let policy = perimeter.policy();
    // The sandbox is irrelevant to what these tests check, and CI and
    // container runs are often root.
    let source = Arc::new(LocalSource::new(
        binary,
        profiles.path().to_path_buf(),
        true,
        perimeter,
    ));
    let tabs = Arc::new(Tabs::new(source, policy));
    let (tx, events) = mpsc::unbounded_channel();
    let tab_events = TabEvents::new(LABEL, tx, Arc::new(AddressOnly(policy)));
    tabs.open(LABEL, url, (800.0, 600.0), tab_events, &DataStore::App)
        .await
        .expect("tab opens");
    Some(Harness {
        engine: ChromiumEngine::new(tabs.clone()),
        tabs,
        events,
        _profiles: profiles,
    })
}

impl Harness {
    /// Wait for the event `matches` accepts; panics after 20 s.
    async fn wait_for(&mut self, what: &str, matches: impl Fn(&TabEvent) -> bool) -> TabEvent {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
        loop {
            let event = tokio::time::timeout_at(deadline, self.events.recv())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
                .expect("event stream open")
                .1;
            if matches(&event) {
                return event;
            }
        }
    }

    async fn eval(&self, script: assets::Script) -> Value {
        self.engine
            .evaluate(LABEL, script)
            .await
            .expect("helper runs")
    }

    async fn shows(&self, text: &str) -> bool {
        // The page updates its text in the input handler, after the key
        // event returned.
        for _ in 0..20 {
            if self.eval(assets::contains_text(text)).await == json!(true) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        false
    }
}

#[tokio::test]
async fn click_type_and_navigate_a_local_page() {
    let base = serve_pages().await;
    let Some(mut h) = harness(InternalAddresses::Allow, &format!("{base}/")).await else {
        return;
    };
    h.wait_for("the first page to load", |e| {
        matches!(e, TabEvent::LoadFinished)
    })
    .await;
    assert!(h.shows("clicks:0 typed:").await, "the page script ran");

    h.engine
        .pointer_click(LABEL, Point { x: 70.0, y: 40.0 }, InputMode::Trusted)
        .await
        .expect("click lands");
    assert!(h.shows("clicks:1").await, "the click reached the button");

    h.engine
        .pointer_click(LABEL, Point { x: 120.0, y: 115.0 }, InputMode::Trusted)
        .await
        .expect("click focuses the field");
    h.engine
        .key(LABEL, KeyInput::Text("hello"), InputMode::Trusted)
        .await
        .expect("text lands");
    assert!(
        h.shows("typed:hello").await,
        "the typed text reached the field"
    );

    let next = format!("{base}/next");
    h.tabs
        .navigate(LABEL, &next)
        .await
        .expect("navigation starts");
    let committed = h
        .wait_for(
            "the second page to commit",
            |e| matches!(e, TabEvent::NavCommitted { url, .. } if url.ends_with("/next")),
        )
        .await;
    assert!(matches!(committed, TabEvent::NavCommitted { .. }));
    h.wait_for("the second page to load", |e| {
        matches!(e, TabEvent::LoadFinished)
    })
    .await;
    assert_eq!(h.eval(assets::location()).await, json!(next));
    assert!(h.shows("second page").await);

    h.tabs.close(LABEL).await;
    h.wait_for("the tab to close", |e| matches!(e, TabEvent::Closed))
        .await;
}

#[tokio::test]
async fn a_loopback_page_is_refused_by_default() {
    let base = serve_pages().await;
    let Some(mut h) = harness(InternalAddresses::Deny, "about:blank").await else {
        return;
    };
    h.tabs
        .navigate(LABEL, &format!("{base}/"))
        .await
        .expect("navigation starts");
    let refused = h
        .wait_for(
            "the loopback hop",
            |e| matches!(e, TabEvent::NavStarting { url, .. } if url.starts_with(&base)),
        )
        .await;
    assert!(
        matches!(
            refused,
            TabEvent::NavStarting {
                verdict: NavVerdict::Deny,
                ..
            }
        ),
        "the hop to a loopback page is denied"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(h.eval(assets::location()).await, json!("about:blank"));
    h.tabs.close(LABEL).await;
}
