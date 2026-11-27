use super::{
    build_agentation_init_script, clamp_tab_zoom, wrap_agentation_toolbar_script, MAX_TAB_ZOOM,
    MIN_TAB_ZOOM,
};

#[test]
fn clamp_tab_zoom_respects_bounds_and_rounds() {
    assert_eq!(clamp_tab_zoom(0.1), MIN_TAB_ZOOM);
    assert_eq!(clamp_tab_zoom(5.0), MAX_TAB_ZOOM);
    assert_eq!(clamp_tab_zoom(1.254), 1.25);
    assert_eq!(clamp_tab_zoom(0.9), 0.9);
    assert_eq!(clamp_tab_zoom(f64::NAN), 1.0);
    assert_eq!(clamp_tab_zoom(f64::INFINITY), 1.0);
    assert_eq!(clamp_tab_zoom(f64::NEG_INFINITY), 1.0);
}

#[test]
fn agentation_init_script_omits_bridge_nonce_from_window() {
    let init = build_agentation_init_script("http://127.0.0.1:4321/secret", "tab-1");
    assert!(init.contains("__TERMUL_AGENTATION_ENDPOINT__"));
    assert!(init.contains("__TERMUL_TAB_ID__"));
    assert!(!init.contains("__TERMUL_AGENTATION_NONCE__"));
}

#[test]
fn wrap_agentation_toolbar_script_passes_nonce_via_closure_only() {
    let wrapped = wrap_agentation_toolbar_script("/* bundle */", Some("nonce-xyz"));
    assert!(wrapped.starts_with("(function(__troupeBridgeNonce){"));
    assert!(wrapped.ends_with("})(\"nonce-xyz\");"));
    assert!(!wrapped.contains("window.__TERMUL_AGENTATION_NONCE__"));

    let no_nonce = wrap_agentation_toolbar_script("/* bundle */", None);
    assert!(no_nonce.ends_with("})(null);"));
}
