//! Where a hosted tab may connect.
//!
//! Every Chromium the host starts sends all of its traffic, loopback
//! included, through this loopback proxy (`LaunchSpec::proxy_port`). The
//! proxy resolves each host itself, refuses the request when any address
//! is one the policy forbids, and connects to the address it checked, so
//! Chromium never resolves a name and a rebinding name cannot swap the
//! address after the check. Popups, frames, workers and WebSockets take
//! the same path as the page.
//!
//! The proxy forwards only to addresses the policy allows, so a local
//! process that finds its port gains nothing it could not already reach.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use tauri::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A request head longer than this is refused.
const MAX_HEAD: usize = 16 * 1024;

/// AWS IPv6 instance metadata endpoint `fd00:ec2::254`.
const AWS_METADATA_V6: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254);
/// Alibaba Cloud metadata endpoint, inside the carrier-grade NAT range.
const ALIBABA_METADATA: Ipv4Addr = Ipv4Addr::new(100, 100, 100, 200);
const METADATA_HOSTNAMES: &[&str] = &["metadata.google.internal", "metadata"];

/// Whether tabs may reach loopback and private networks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalAddresses {
    /// The default for a server other people reach.
    Deny,
    /// The operator's opt-in (`--browser-host-allow-internal`). Link-local,
    /// metadata and Troupe's own ports stay refused.
    Allow,
}

/// What kind of address a tab wants to reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressClass {
    /// Unspecified, link-local, multicast, broadcast and cloud metadata.
    Never,
    Loopback,
    /// RFC 1918, carrier-grade NAT, unique-local IPv6 and `0.0.0.0/8`.
    Private,
    Public,
}

pub fn classify(ip: IpAddr) -> AddressClass {
    match ip.to_canonical() {
        IpAddr::V4(v4) => classify_v4(v4),
        IpAddr::V6(v6) => classify_v6(v6),
    }
}

fn classify_v4(ip: Ipv4Addr) -> AddressClass {
    let [a, b, ..] = ip.octets();
    if ip.is_unspecified()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip == ALIBABA_METADATA
    {
        AddressClass::Never
    } else if ip.is_loopback() {
        AddressClass::Loopback
    } else if ip.is_private() || a == 0 || (a == 100 && (64..128).contains(&b)) {
        AddressClass::Private
    } else {
        AddressClass::Public
    }
}

fn classify_v6(ip: Ipv6Addr) -> AddressClass {
    let first = ip.segments()[0];
    if ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xffc0) == 0xfe80
        || ip == AWS_METADATA_V6
    {
        AddressClass::Never
    } else if ip.is_loopback() {
        AddressClass::Loopback
    } else if (first & 0xfe00) == 0xfc00 {
        AddressClass::Private
    } else {
        AddressClass::Public
    }
}

/// The address policy, shared by the proxy and the navigation hook.
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub internal: InternalAddresses,
    /// The proxy's own port, which no tab may loop back into.
    proxy_port: u16,
}

impl Policy {
    pub fn allows(&self, addr: SocketAddr) -> bool {
        let class = classify(addr.ip());
        let own_port = class == AddressClass::Loopback
            && (addr.port() == self.proxy_port
                || crate::host_perimeter::is_owned_port(addr.port()));
        match (class, self.internal) {
            (AddressClass::Never, _) => false,
            _ if own_port => false,
            (AddressClass::Public, _) => true,
            (_, InternalAddresses::Allow) => true,
            (_, InternalAddresses::Deny) => false,
        }
    }

    /// The navigation hook's verdict for `url`, from what the URL itself
    /// says. Names are resolved by the proxy when the request goes out.
    pub fn allows_url(&self, url: &Url) -> bool {
        match url.scheme() {
            "about" => return url.as_str() == "about:blank",
            // Blob provenance cannot be established from a navigation URL alone.
            "data" | "javascript" | "blob" => return false,
            "http" | "https" | "ws" | "wss" => {}
            _ => return false,
        }
        let port = url.port_or_known_default().unwrap_or(80);
        match url.host() {
            Some(url::Host::Ipv4(ip)) => self.allows(SocketAddr::new(IpAddr::V4(ip), port)),
            Some(url::Host::Ipv6(ip)) => self.allows(SocketAddr::new(IpAddr::V6(ip), port)),
            Some(url::Host::Domain(name)) => self.allows_name(name, port),
            None => false,
        }
    }

    fn allows_name(&self, name: &str, port: u16) -> bool {
        let name = name.trim_end_matches('.').to_ascii_lowercase();
        if METADATA_HOSTNAMES.contains(&name.as_str()) {
            return false;
        }
        let loopback_name = name == "localhost" || name.ends_with(".localhost");
        !loopback_name || self.allows(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }
}

/// The running proxy. Dropping it stops accepting connections.
pub struct Perimeter {
    policy: Policy,
    task: tokio::task::AbortHandle,
}

impl Perimeter {
    /// Bind a loopback port and start serving.
    pub async fn start(internal: InternalAddresses) -> std::io::Result<Arc<Self>> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let policy = Policy {
            internal,
            proxy_port: listener.local_addr()?.port(),
        };
        let task = tokio::spawn(serve(listener, policy)).abort_handle();
        Ok(Arc::new(Self { policy, task }))
    }

    pub fn port(&self) -> u16 {
        self.policy.proxy_port
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }
}

impl Drop for Perimeter {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(listener: TcpListener, policy: Policy) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(_) => {
                // Out of descriptors and the like: back off instead of spinning.
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                continue;
            }
        };
        if !peer.ip().is_loopback() {
            continue;
        }
        tokio::spawn(async move {
            if let Err(reason) = proxy(stream, policy).await {
                log::debug!("[chromium-perimeter] request refused: {reason}");
            }
        });
    }
}

/// One proxied connection: `CONNECT host:port` for TLS and WebSockets, or
/// an absolute-form request for plain HTTP.
async fn proxy(mut client: TcpStream, policy: Policy) -> Result<(), &'static str> {
    let (head, rest) = read_head(&mut client).await?;
    let text = std::str::from_utf8(&head).map_err(|_| "non-utf8 head")?;
    let request_line = text.lines().next().ok_or("empty head")?;
    let mut parts = request_line.split(' ');
    let (method, target, version) = match (parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(t), Some(v)) => (m, t, v),
        _ => return Err("malformed request line"),
    };
    let connect = method.eq_ignore_ascii_case("CONNECT");
    let (host, port, path) = if connect {
        let (host, port) = split_host_port(target).ok_or("malformed CONNECT target")?;
        (host, port, String::new())
    } else {
        let url = Url::parse(target).map_err(|_| "not an absolute-form request")?;
        if url.scheme() != "http" {
            return Err("only http is proxied without CONNECT");
        }
        let host = url
            .host_str()
            .ok_or("no host")?
            .trim_matches(['[', ']'])
            .to_string();
        let port = url.port_or_known_default().ok_or("no port")?;
        let mut path = url.path().to_string();
        if let Some(query) = url.query() {
            path.push('?');
            path.push_str(query);
        }
        (host, port, path)
    };
    let Some(addr) = resolve_allowed(&host, port, policy).await else {
        let _ = client
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return Err("address refused by the perimeter");
    };
    let mut upstream = TcpStream::connect(addr)
        .await
        .map_err(|_| "upstream connect failed")?;
    if connect {
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .map_err(|_| "client gone")?;
    } else {
        upstream
            .write_all(&origin_form_head(text, method, &path, version))
            .await
            .map_err(|_| "upstream write failed")?;
    }
    upstream
        .write_all(&rest)
        .await
        .map_err(|_| "upstream write failed")?;
    let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    Ok(())
}

/// Resolve `host` and return the address to connect to, or `None` when
/// any of its addresses is refused.
async fn resolve_allowed(host: &str, port: u16, policy: Policy) -> Option<SocketAddr> {
    if METADATA_HOSTNAMES.contains(&host.trim_end_matches('.').to_ascii_lowercase().as_str()) {
        return None;
    }
    let addrs: Vec<SocketAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => tokio::net::lookup_host((host, port)).await.ok()?.collect(),
    };
    if addrs.is_empty() || !addrs.iter().all(|addr| policy.allows(*addr)) {
        return None;
    }
    addrs.first().copied()
}

fn split_host_port(target: &str) -> Option<(String, u16)> {
    let (host, port) = target.rsplit_once(':')?;
    let host = host.trim_matches(['[', ']']);
    let port = port.parse().ok()?;
    (!host.is_empty()).then(|| (host.to_string(), port))
}

/// The request head with an origin-form target, without proxy headers, and
/// with `Connection: close`: the next request on the client's connection
/// may name another host, so it must come on a new connection.
fn origin_form_head(head: &str, method: &str, path: &str, version: &str) -> Vec<u8> {
    let mut out = format!("{method} {path} {version}\r\n");
    for line in head.split("\r\n").skip(1) {
        if line.is_empty() {
            continue;
        }
        let name = line
            .split(':')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "proxy-connection" | "proxy-authorization" | "connection" | "keep-alive"
        ) {
            continue;
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.push_str("Connection: close\r\n\r\n");
    out.into_bytes()
}

/// Read until the end of the request head; return the head and any bytes
/// read past it.
async fn read_head(client: &mut TcpStream) -> Result<(Vec<u8>, Vec<u8>), &'static str> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    loop {
        let n = client
            .read(&mut chunk)
            .await
            .map_err(|_| "client read failed")?;
        if n == 0 {
            return Err("client closed before the head ended");
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buf.split_off(end + 4);
            buf.truncate(end);
            return Ok((buf, rest));
        }
        if buf.len() > MAX_HEAD {
            return Err("request head too long");
        }
    }
}

#[cfg(test)]
#[path = "perimeter_tests.rs"]
mod tests;
