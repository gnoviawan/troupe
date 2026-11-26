use super::*;
use crate::server_update::UpdateChannel;

/// An unknown channel must fail WITHOUT any network call — the parse is
/// the gate, so a bogus value never reaches reqwest.
#[tokio::test]
async fn fetch_channel_manifest_rejects_unknown_channel_without_network() {
    let result = fetch_channel_manifest("bogus").await;
    let err = result.expect_err("bogus channel must error");
    assert!(
        err.contains("unknown update channel"),
        "unexpected error: {err}"
    );
}

/// Parity guard: the accepted channels must map to the same URLs
/// `server_update::UpdateChannel::manifest_url()` returns. If someone
/// changes the Rust constant, this breaks — prompting a check that the
/// renderer `CHANNEL_MANIFEST_URLS` constant (the error-message source)
/// is updated in lockstep so the surfaced error keeps naming the right URL.
#[test]
fn channel_manifest_urls_match_server_update() {
    assert_eq!(
        UpdateChannel::Stable.manifest_url(),
        "https://github.com/gnoviawan/troupe/releases/latest/download/latest-stable.json"
    );
    assert_eq!(
        UpdateChannel::Nightly.manifest_url(),
        "https://github.com/gnoviawan/troupe/releases/download/nightly/nightly.json"
    );
}

#[test]
fn signed_channel_endpoint_is_nightly_only() {
    assert_eq!(
        signed_channel_endpoint(UpdateChannel::Nightly),
        Some(UpdateChannel::Nightly.manifest_url())
    );
    assert_eq!(signed_channel_endpoint(UpdateChannel::Stable), None);
}

#[test]
fn resolve_signed_channel_rejects_unsupported_channels_without_network() {
    assert_eq!(
        resolve_signed_channel("nightly"),
        Ok(UpdateChannel::Nightly)
    );
    let stable = resolve_signed_channel("stable").expect_err("stable uses check()");
    assert!(
        stable.contains("does not support in-app updates"),
        "{stable}"
    );
    let bogus = resolve_signed_channel("bogus").expect_err("bogus channel");
    assert!(bogus.contains("unknown update channel"), "{bogus}");
}

fn v(s: &str) -> semver::Version {
    semver::Version::parse(s).expect("valid semver")
}

fn nightly_accepts(current: &str, remote: &str) -> bool {
    accepts_channel_update(UpdateChannel::Nightly, &v(current), &v(remote))
}

#[test]
fn nightly_offers_only_newer_builds() {
    assert!(nightly_accepts("0.5.37", "0.5.38"));
    assert!(nightly_accepts("0.5.37", "0.5.120"));
    assert!(!nightly_accepts("0.5.37", "0.5.37"), "installed build");
    assert!(!nightly_accepts("0.5.38", "0.5.37"), "older build");
}

#[test]
fn nightly_moves_across_stable_minors() {
    assert!(nightly_accepts("0.5.37", "0.6.0"), "next stable");
    assert!(nightly_accepts("0.6.0", "0.6.1"), "stable install switching to Nightly");
    assert!(!nightly_accepts("0.6.0", "0.5.37"), "nightly of an older minor");
    assert!(!nightly_accepts("0.6.0", "0.6.0"), "nightly with no commits since the stable");
}

#[test]
fn a_build_from_before_the_x_y_n_scheme_is_offered_the_new_nightly() {
    assert!(nightly_accepts("0.5.0-nightly.20261124.116.c181ffe", "0.5.37"));
    assert!(nightly_accepts("0.5.0", "0.5.37"));
}

#[test]
fn stable_never_accepts_signed_channel_updates() {
    assert!(!accepts_channel_update(
        UpdateChannel::Stable,
        &v("0.5.0"),
        &v("0.5.37")
    ));
}

/// Stable never reads the Nightly manifest and Nightly never reads the
/// Stable one.
#[test]
fn stable_and_nightly_manifests_never_cross() {
    let nightly = signed_channel_endpoint(UpdateChannel::Nightly).expect("nightly endpoint");
    assert!(nightly.ends_with("/releases/download/nightly/nightly.json"));
    assert_ne!(nightly, UpdateChannel::Stable.manifest_url());
    assert!(!nightly.contains("/latest/download/"));
    assert_eq!(signed_channel_endpoint(UpdateChannel::Stable), None);

    let config: serde_json::Value =
        serde_json::from_str(include_str!("../../tauri.conf.json")).expect("tauri.conf.json");
    let stable_endpoints = config["plugins"]["updater"]["endpoints"]
        .as_array()
        .expect("stable updater endpoints");
    assert!(!stable_endpoints.is_empty());
    for endpoint in stable_endpoints {
        let endpoint = endpoint.as_str().expect("endpoint string");
        assert!(!endpoint.contains("nightly"), "{endpoint}");
        assert_ne!(endpoint, nightly);
    }
}
