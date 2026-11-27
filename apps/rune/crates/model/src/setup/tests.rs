use super::{Answers, AuthGate, auth_verdict, restart_landing_url};

fn answers(host: &str, remote: bool, port: Option<f64>) -> Answers {
    Answers {
        host: host.into(),
        port,
        sessions_dir: "/s".into(),
        projects_file: "/p.json".into(),
        allow_remote_writes: remote,
        update_channel: None,
        update_interval_secs: Some(21600.0),
    }
}

#[test]
fn a_loopback_bind_never_keeps_remote_writes() {
    let a = answers("localhost", true, Some(8080.0)).normalized();
    assert_eq!((a.host.as_str(), a.allow_remote_writes), ("127.0.0.1", false));
    let mut b = answers("ALL", true, Some(8080.0)).normalized();
    assert_eq!((b.host.as_str(), b.allow_remote_writes), ("0.0.0.0", true));
    b.set_host("127.0.0.1");
    assert!(!b.allow_remote_writes);
}

#[test]
fn the_port_must_be_a_whole_number_in_range() {
    for (port, ok) in [(Some(8080.0), true), (Some(1.0), true), (Some(65535.0), true), (Some(0.0), false), (Some(65536.0), false), (Some(80.5), false), (None, false)] {
        assert_eq!(answers("127.0.0.1", false, port).port_error().is_none(), ok, "{port:?}");
    }
}

#[test]
fn the_landing_keeps_the_page_path_and_encodes_the_token() {
    assert_eq!(restart_landing_url("0.0.0.0", 8195, Some("a+b/c"), "box.lan", "/rune/"), "http://box.lan:8195/rune/#token=a%2Bb%2Fc");
    assert_eq!(restart_landing_url("127.0.0.1", 8080, None, "x", ""), "http://127.0.0.1:8080/");
}

#[test]
fn only_an_unauthorized_refusal_shows_the_token_gate() {
    assert_eq!(auth_verdict(Ok(())), AuthGate::Ok);
    assert_eq!(auth_verdict(Err(Some("UNAUTHORIZED"))), AuthGate::Unauthorized);
    assert_eq!(auth_verdict(Err(Some("NOT_FOUND"))), AuthGate::NetworkError);
    assert_eq!(auth_verdict(Err(None)), AuthGate::NetworkError);
}
