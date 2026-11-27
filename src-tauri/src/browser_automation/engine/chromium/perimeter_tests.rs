#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::{IpAddr, SocketAddr};

use tauri::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::{classify, AddressClass, InternalAddresses, Perimeter};

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

#[test]
fn addresses_fall_in_the_expected_classes() {
    let cases = [
        ("8.8.8.8", AddressClass::Public),
        ("2606:4700::1111", AddressClass::Public),
        ("127.0.0.1", AddressClass::Loopback),
        ("::1", AddressClass::Loopback),
        ("::ffff:127.0.0.1", AddressClass::Loopback),
        ("10.1.2.3", AddressClass::Private),
        ("172.16.0.1", AddressClass::Private),
        ("192.168.1.1", AddressClass::Private),
        ("100.64.0.1", AddressClass::Private),
        ("0.1.2.3", AddressClass::Private),
        ("fd12::1", AddressClass::Private),
        ("::ffff:10.0.0.1", AddressClass::Private),
        ("0.0.0.0", AddressClass::Never),
        ("::", AddressClass::Never),
        ("169.254.169.254", AddressClass::Never),
        ("fe80::1", AddressClass::Never),
        ("fd00:ec2::254", AddressClass::Never),
        ("100.100.100.200", AddressClass::Never),
        ("224.0.0.1", AddressClass::Never),
        ("255.255.255.255", AddressClass::Never),
    ];
    for (text, class) in cases {
        assert_eq!(classify(ip(text)), class, "{text}");
    }
}

#[tokio::test]
async fn the_url_verdict_follows_the_internal_address_switch() {
    let deny = Perimeter::start(InternalAddresses::Deny)
        .await
        .unwrap()
        .policy();
    let allow = Perimeter::start(InternalAddresses::Allow)
        .await
        .unwrap()
        .policy();
    let cases = [
        ("https://example.com/", true, true),
        ("http://127.0.0.1:3000/", false, true),
        ("http://localhost:3000/", false, true),
        ("http://app.localhost/", false, true),
        ("http://192.168.1.10/", false, true),
        ("http://[::1]:8000/", false, true),
        ("http://169.254.169.254/latest/meta-data/", false, false),
        ("http://metadata.google.internal/", false, false),
        ("http://0.0.0.0:3000/", false, false),
        ("file:///etc/passwd", false, false),
        ("chrome://settings/", false, false),
        ("about:blank", true, true),
        ("data:text/html,hi", true, true),
    ];
    for (url, denied_mode, allowed_mode) in cases {
        let url = Url::parse(url).unwrap();
        assert_eq!(deny.allows_url(&url), denied_mode, "deny mode: {url}");
        assert_eq!(allow.allows_url(&url), allowed_mode, "allow mode: {url}");
    }
}

#[tokio::test]
async fn no_tab_may_loop_back_into_the_proxy() {
    let perimeter = Perimeter::start(InternalAddresses::Allow).await.unwrap();
    let own = SocketAddr::new(ip("127.0.0.1"), perimeter.port());
    assert!(!perimeter.policy().allows(own));
}

async fn ask(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut reply = Vec::new();
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        stream.read_to_end(&mut reply),
    )
    .await;
    String::from_utf8_lossy(&reply).into_owned()
}

/// A loopback server that answers every request with `upstream-ok`.
async fn upstream() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nupstream-ok")
                .await;
        }
    });
    port
}

#[tokio::test]
async fn the_proxy_refuses_loopback_by_default_and_forwards_it_on_opt_in() {
    let target = upstream().await;
    let deny = Perimeter::start(InternalAddresses::Deny).await.unwrap();
    let refused = ask(
        deny.port(),
        &format!("GET http://127.0.0.1:{target}/ HTTP/1.1\r\nHost: x\r\n\r\n"),
    )
    .await;
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");
    let refused = ask(
        deny.port(),
        &format!("CONNECT localhost:{target} HTTP/1.1\r\n\r\n"),
    )
    .await;
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");

    let allow = Perimeter::start(InternalAddresses::Allow).await.unwrap();
    let forwarded = ask(
        allow.port(),
        &format!("GET http://127.0.0.1:{target}/ HTTP/1.1\r\nHost: x\r\n\r\n"),
    )
    .await;
    assert!(forwarded.ends_with("upstream-ok"), "{forwarded}");
    let metadata = ask(
        allow.port(),
        "GET http://169.254.169.254/ HTTP/1.1\r\nHost: x\r\n\r\n",
    )
    .await;
    assert!(metadata.starts_with("HTTP/1.1 403"), "{metadata}");
}
