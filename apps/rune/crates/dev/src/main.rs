//! `troupe-dev`: the Rune desktop shell in a native window. It connects straight to
//! `TROUPE_SERVER_URL` with `TROUPE_SERVER_TOKEN` as device `TROUPE_DEVICE_ID`, with no
//! launcher, and links the live driver, so `RUNE_DRIVE_SOCK=<path>` lets tests and
//! `tools/parity` drive the window. Without `TROUPE_SERVER_URL` it opens unconnected.
//! `TROUPE_ROUTE` takes the URL hash the web client would open (`#/nowhere`), since a native
//! window has no URL.

#![forbid(unsafe_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use troupe_app::{Launch, Route};

fn config() -> Option<client::Config> {
    let base_url = std::env::var("TROUPE_SERVER_URL")
        .ok()
        .filter(|v| !v.is_empty())?;
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    Some(client::Config {
        base_url: base_url.trim_end_matches('/').to_owned(),
        token: std::env::var("TROUPE_SERVER_TOKEN")
            .ok()
            .filter(|v| !v.is_empty()),
        device_id: std::env::var("TROUPE_DEVICE_ID").unwrap_or_else(|_| "troupe-dev".into()),
        client_instance_id: format!("troupe-dev-{pid}-{nanos:x}"),
        seed: (u64::from(pid) << 32) | u64::from(nanos),
        host_local: None,
    })
}

fn main() {
    let launch = Launch {
        config: config(),
        route: Route::from_hash(&std::env::var("TROUPE_ROUTE").unwrap_or_default()),
    };
    automation::install_with(troupe_app::app(launch), troupe_canvas::CanvasCompositor::new()).run();
}
