use super::*;

#[test]
fn url_normalization_and_display_match_react() {
    assert_eq!(normalize_browser_input_url("  "), None);
    assert_eq!(
        normalize_browser_input_url("http://127.0.0.1:8199/test"),
        Some("http://127.0.0.1:8199/test".into())
    );
    assert_eq!(
        normalize_browser_input_url("localhost:3000/app"),
        Some("http://localhost:3000/app".into())
    );
    assert_eq!(
        normalize_browser_input_url(":5173/docs"),
        Some("http://localhost:5173/docs".into())
    );
    assert_eq!(
        normalize_browser_input_url("example.com/a?b=1"),
        Some("https://example.com/a?b=1".into())
    );
    assert_eq!(
        normalize_browser_input_url("about:blank"),
        Some("about:blank".into())
    );

    let parts = split_display_url("https://example.com:8443/path?x=1#h").unwrap();
    assert_eq!(parts.scheme, "https://");
    assert_eq!(parts.host, "example.com:8443");
    assert_eq!(parts.path, "/path?x=1#h");
    assert!(parts.is_https);

    let root = split_display_url("http://localhost:3000/").unwrap();
    assert_eq!(root.path, "");
    assert!(!root.is_https);

    assert_eq!(
        parse_localhost_port("http://127.0.0.1:8199/"),
        Some(("127.0.0.1:8199".into(), 8199))
    );
    assert_eq!(parse_localhost_port("https://example.com"), None);
}

#[test]
fn zoom_and_viewport_helpers_match_react() {
    assert_eq!(clamp_browser_zoom(0.1), 0.5);
    assert_eq!(clamp_browser_zoom(4.0), 2.0);
    assert_eq!(clamp_browser_zoom(f64::NAN), 1.0);
    assert_eq!(next_zoom_step(1.0, true), 1.1);
    assert_eq!(next_zoom_step(1.0, false), 0.9);
    assert_eq!(next_zoom_step(2.0, true), 2.0);
    assert_eq!(next_zoom_step(0.5, false), 0.5);

    assert_eq!(
        resolve_viewport_dimensions(ViewportPreset::Mobile, ViewportOrientation::Portrait, None),
        ViewportDimensions {
            target_width: Some(390),
            target_height: Some(844),
        }
    );
    assert_eq!(
        resolve_viewport_dimensions(
            ViewportPreset::Tablet,
            ViewportOrientation::Landscape,
            Some(900)
        ),
        ViewportDimensions {
            target_width: Some(900),
            target_height: Some(768),
        }
    );
}

#[test]
fn session_and_control_stores_handle_lifecycle() {
    let mut s = SessionState::default();
    step_session(
        &mut s,
        SessionEvent::CreateTab("t1".into(), "http://127.0.0.1:8199/".into()),
        1000,
    );
    assert_eq!(s.tabs["t1"].display_title(), "127.0.0.1:8199");
    assert_eq!(s.recent_urls.len(), 1);

    step_session(
        &mut s,
        SessionEvent::ApplyView(BrowserTabView {
            tab_id: "t1".into(),
            url: "http://127.0.0.1:8199/page".into(),
            title: "Local Demo".into(),
            loading: false,
            agent_controlled: true,
            epoch: Some(2),
        }),
        2000,
    );
    assert_eq!(s.tabs["t1"].display_title(), "Local Demo");
    assert_eq!(s.tabs["t1"].agent_controlled, Some(true));

    step_session(&mut s, SessionEvent::SetCrashed("t1".into(), true), 3000);
    assert!(s.tabs["t1"].crashed);

    let mut c = ControlState::default();
    step_control(&mut c, ControlEvent::TrackAgentTab("t1".into(), "s1".into()));
    assert!(c.session_drives_browser("s1"));
    step_control(&mut c, ControlEvent::SetStopped("s1".into(), true));
    assert!(c.is_session_stopped("s1"));
    step_control(&mut c, ControlEvent::ForgetSession("s1".into()));
    assert!(!c.session_drives_browser("s1"));
    assert!(!c.is_session_stopped("s1"));
}

#[test]
fn policy_failure_copy_matches_known_reasons() {
    let f = parse_policy_failure("invalid_params", "invalid_origin: bad url");
    assert_eq!(f.reason.as_deref(), Some("invalid_origin"));
    assert!(describe_failure(&f).contains("https://example.com"));
}
