//! Remote access for `troupe-server --desktop`, switched while the server
//! runs: a LAN listener and a cloudflared quick tunnel. Each is its own
//! [`serve_router`] beside the loopback listener the desktop app uses, on the
//! same `AcpManager`, relay, terminals and store, so turning one on or off,
//! or rotating its token, never restarts the server or stops an agent
//! (`serve_router` never kills agents; only `serve` does, at exit).
//!
//! Both listeners take one remote bearer token, `<state dir>/web-auth-token`,
//! the file the Tauri app's shared-live host uses. It is never the desktop
//! app's own loopback token. Rotating it replaces the file and restarts the
//! running listeners, which closes every socket that used the old token.
//!
//! Remote writes reuse the server's `--allow-remote-writes` gate: a LAN peer
//! is not loopback, so it writes only with the opt-in, and the tunnel (whose
//! peer is cloudflared on loopback) denies every write without it. A remote
//! connection is never host-local: neither listener's gate carries the
//! host-local credential. Only a host-local `/ws` client may call this API.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::sync::{oneshot, Mutex};
use tracing::{info, warn};

use crate::acp::{AcpCatalogService, AcpInstallService, AcpManager};
use crate::pty::PtyManager;
use crate::remote::{cloudflared, RemoteAccess, RemoteBindMode, RemoteServerState, SharedLiveData};
use crate::web::auth::{self, WebAuth, WebAuthResolution, WEB_AUTH_TOKEN_FILE};
use crate::web::host_allow::HostAllowList;
use crate::web::project_registry::ProjectRegistry;
use crate::web::sink::WsRelaySink;
use crate::web::store::WebStore;
use crate::web::{serve_router, ServerConfig};

/// Longest a stopping listener may take to drain before its serve task is
/// aborted. Streamed bodies already end at stop (`web::stream_cutoff`); this
/// is the backstop that keeps a stop, a rotation or the server's exit from
/// hanging on a connection that never finishes.
pub const STOP_BOUND: Duration = Duration::from_secs(5);

const ROTATE_BIND_ATTEMPTS: u32 = 5;
const ROTATE_BIND_BACKOFF: Duration = Duration::from_millis(50);

/// End every HTML preview session: a `/preview/<token>/` URL a remote
/// device learned must stop serving once its access goes.
fn close_previews() {
    let closed = crate::html_preview::registry().close_all();
    if closed > 0 {
        info!(target: "termul::web::listeners", closed, "preview sessions closed");
    }
}

/// What a remote listener serves: the running host's pieces.
pub struct ListenerParts {
    pub acp: Arc<AcpManager>,
    pub pty: Arc<PtyManager>,
    pub relay: Arc<WsRelaySink>,
    pub registry: Arc<ProjectRegistry>,
    pub acp_catalog: Option<Arc<AcpCatalogService>>,
    pub acp_install: Option<Arc<AcpInstallService>>,
    pub store: Arc<WebStore>,
    /// The state dir: the remote token file lives here.
    pub state_dir: PathBuf,
    /// `--allow-remote-writes`.
    pub allow_remote_writes: bool,
}

struct Lan {
    addr: SocketAddr,
    auth: Arc<WebAuth>,
    shutdown: oneshot::Sender<()>,
    serve: tokio::task::JoinHandle<()>,
}

/// The remote listeners of one host.
pub struct Listeners {
    parts: ListenerParts,
    /// The LAN listener; its lock also serializes every operation.
    lan: Mutex<Option<Lan>>,
    tunnel: RemoteServerState,
}

/// The LAN listener as the settings page shows it. `urls` carry the token
/// in the fragment: secrets, never logged.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanStatus {
    pub port: u16,
    pub urls: Vec<String>,
}

/// What [`Listeners::status`] reports. No `Debug`: it carries share URLs.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenersStatus {
    pub lan: Option<LanStatus>,
    /// The tunnel's share URL (with the token fragment) once it is up.
    pub tunnel: Option<String>,
    pub tunnel_running: bool,
    /// Remote clients may write (`--allow-remote-writes`).
    pub remote_writes: bool,
}

impl Listeners {
    #[must_use]
    pub fn new(parts: ListenerParts) -> Self {
        Self {
            parts,
            lan: Mutex::new(None),
            tunnel: RemoteServerState::new(),
        }
    }

    fn access(&self) -> RemoteAccess {
        RemoteAccess::from_allow_writes(self.parts.allow_remote_writes)
    }

    /// The remote token's gate, loaded from or generated into the state dir.
    fn remote_gate(&self) -> Result<Arc<WebAuth>, String> {
        match auth::resolve_shared_live(true, &self.parts.state_dir)? {
            WebAuthResolution::Gated { auth, .. } => Ok(auth),
            WebAuthResolution::Ungated => Err("the remote listeners need a token".to_string()),
        }
    }

    pub async fn status(&self) -> ListenersStatus {
        let lan = self.lan.lock().await;
        self.status_with(lan.as_ref())
    }

    fn status_with(&self, lan: Option<&Lan>) -> ListenersStatus {
        let tunnel = self.tunnel.status();
        ListenersStatus {
            lan: lan.map(|lan| LanStatus {
                port: lan.addr.port(),
                urls: lan_urls(lan.addr.port())
                    .iter()
                    .map(|url| lan.auth.share_url(url))
                    .collect(),
            }),
            tunnel: tunnel.tunnel_url,
            tunnel_running: tunnel.running,
            remote_writes: self.parts.allow_remote_writes,
        }
    }

    /// Bind the LAN listener on `0.0.0.0:port` (`0` picks a free port). A
    /// running listener on another port is moved; on the same port it stays.
    ///
    /// # Errors
    /// The token cannot be resolved or the port cannot be bound.
    pub async fn start_lan(&self, port: u16) -> Result<ListenersStatus, String> {
        let mut lan = self.lan.lock().await;
        if lan
            .as_ref()
            .is_some_and(|running| port == 0 || running.addr.port() == port)
        {
            return Ok(self.status_with(lan.as_ref()));
        }
        if let Some(running) = lan.take() {
            stop_lan(running).await;
        }
        *lan = Some(
            self.bind_lan(port)
                .await
                .map_err(LanBindError::into_message)?,
        );
        Ok(self.status_with(lan.as_ref()))
    }

    async fn bind_lan(&self, port: u16) -> Result<Lan, LanBindError> {
        let auth = self.remote_gate().map_err(LanBindError::Other)?;
        let (shutdown, stopped) = oneshot::channel::<()>();
        let parts = &self.parts;
        let (addr, serve) = serve_router(
            Arc::clone(&parts.acp),
            Arc::clone(&parts.pty),
            parts.pty.terminal_events(),
            parts.pty.cwd_tracker(),
            parts.pty.git_tracker(),
            parts.pty.exit_code_tracker(),
            Arc::clone(&parts.relay),
            Arc::clone(&parts.registry),
            lan_config(port, parts),
            HostAllowList::any(),
            async move {
                let _ = stopped.await;
            },
            parts.acp_catalog.clone(),
            parts.acp_install.clone(),
            Arc::clone(&parts.store),
            false,
            Some(Arc::clone(&auth)),
            None,
        )
        .await
        .map_err(|error| LanBindError::from_serve_error(port, error))?;
        warn!(
            target: "termul::web::listeners",
            port = addr.port(),
            remote_writes = parts.allow_remote_writes,
            "LAN listener on: every device on the network that holds the remote token can connect"
        );
        Ok(Lan {
            addr,
            auth,
            shutdown,
            serve,
        })
    }

    async fn rebind_lan_after_rotation(&self, port: u16) -> Result<Lan, String> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.bind_lan(port).await {
                Ok(lan) => return Ok(lan),
                Err(error) if error.is_addr_in_use() && attempt < ROTATE_BIND_ATTEMPTS => {
                    let backoff = ROTATE_BIND_BACKOFF * attempt;
                    warn!(
                        target: "termul::web::listeners",
                        port,
                        attempt,
                        backoff_ms = backoff.as_millis() as u64,
                        "LAN port still in use while rotating the token; retrying"
                    );
                    tokio::time::sleep(backoff).await;
                }
                Err(error) => return Err(error.into_message()),
            }
        }
    }

    /// Unbind the LAN listener, close its sockets and the preview sessions.
    pub async fn stop_lan(&self) -> ListenersStatus {
        let mut lan = self.lan.lock().await;
        if let Some(running) = lan.take() {
            stop_lan(running).await;
            close_previews();
        }
        self.status_with(None)
    }

    /// Start the tunnel: a loopback listener and cloudflared in front of it.
    ///
    /// # Errors
    /// The listener or cloudflared failed to start; nothing is left running.
    pub async fn start_tunnel(&self) -> Result<ListenersStatus, String> {
        let lan = self.lan.lock().await;
        if !self.tunnel.status().running {
            self.open_tunnel().await?;
        }
        Ok(self.status_with(lan.as_ref()))
    }

    async fn open_tunnel(&self) -> Result<(), String> {
        let parts = &self.parts;
        let started = self
            .tunnel
            .start_with_access(
                Arc::clone(&parts.acp),
                Arc::clone(&parts.pty),
                Arc::clone(&parts.relay),
                Arc::clone(&parts.registry),
                RemoteBindMode::Localhost,
                true,
                false,
                &SharedLiveData {
                    store: Arc::clone(&parts.store),
                    data_dir: parts.state_dir.clone(),
                },
                parts.acp_catalog.clone(),
                parts.acp_install.clone(),
                self.access(),
            )
            .await?;
        let Some(port) = started.port else {
            let _ = self.tunnel.stop().await;
            return Err("the tunnel listener reported no port".to_string());
        };
        match cloudflared::start_quick_tunnel(port).await {
            Ok(tunnel) => {
                self.tunnel.attach_tunnel(tunnel.url, tunnel.child)?;
                info!(
                    target: "termul::web::listeners",
                    port,
                    access = self.access().as_str(),
                    "tunnel on"
                );
                Ok(())
            }
            Err(error) => {
                let _ = self.tunnel.stop().await;
                warn!(target: "termul::web::listeners", "tunnel failed to start: {error}");
                Err(error)
            }
        }
    }

    /// Stop cloudflared and the tunnel listener, closing its sockets and the
    /// preview sessions.
    pub async fn stop_tunnel(&self) -> ListenersStatus {
        let lan = self.lan.lock().await;
        if self.tunnel.stop().await.is_ok() {
            close_previews();
        }
        info!(target: "termul::web::listeners", "tunnel off");
        self.status_with(lan.as_ref())
    }

    /// Replace the remote token and restart the running listeners on it, so
    /// every remote socket that used the old token is closed. The tunnel
    /// comes back on a new URL.
    ///
    /// # Errors
    /// The token file could not be removed, or a listener failed to restart.
    pub async fn rotate_token(&self) -> Result<ListenersStatus, String> {
        let mut lan = self.lan.lock().await;
        let lan_port = match lan.take() {
            Some(running) => {
                let port = running.addr.port();
                stop_lan(running).await;
                Some(port)
            }
            None => None,
        };
        let tunnel_running = self.tunnel.status().running;
        if tunnel_running {
            let _ = self.tunnel.stop().await;
        }
        let token_file = self.parts.state_dir.join(WEB_AUTH_TOKEN_FILE);
        match std::fs::remove_file(&token_file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "cannot replace the remote token at {}: {error}",
                    token_file.display()
                ))
            }
        }
        self.remote_gate()?;
        close_previews();
        info!(target: "termul::web::listeners", "remote token rotated");
        if let Some(port) = lan_port {
            *lan = Some(self.rebind_lan_after_rotation(port).await?);
        }
        if tunnel_running {
            self.open_tunnel().await?;
        }
        Ok(self.status_with(lan.as_ref()))
    }

    /// Server exit: stop both listeners and the preview sessions.
    pub async fn stop_all(&self) {
        if let Some(running) = self.lan.lock().await.take() {
            stop_lan(running).await;
        }
        let _ = self.tunnel.stop().await;
        close_previews();
    }
}

enum LanBindError {
    AddrInUse(String),
    Other(String),
}

impl LanBindError {
    fn from_serve_error(
        port: u16,
        error: Box<dyn std::error::Error + Send + Sync>,
    ) -> LanBindError {
        let message = format!("cannot bind the LAN listener on port {port}: {error}");
        if error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::AddrInUse)
        {
            LanBindError::AddrInUse(message)
        } else {
            LanBindError::Other(message)
        }
    }

    fn is_addr_in_use(&self) -> bool {
        matches!(self, LanBindError::AddrInUse(_))
    }

    fn into_message(self) -> String {
        match self {
            LanBindError::AddrInUse(message) | LanBindError::Other(message) => message,
        }
    }
}

async fn stop_lan(mut lan: Lan) {
    let _ = lan.shutdown.send(());
    match tokio::time::timeout(STOP_BOUND, &mut lan.serve).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            warn!(target: "termul::web::listeners", "LAN listener task ended badly: {error}");
        }
        Err(_) => {
            warn!(
                target: "termul::web::listeners",
                bound_secs = STOP_BOUND.as_secs(),
                "LAN listener did not drain in time; aborting it"
            );
            lan.serve.abort();
            let _ = lan.serve.await;
        }
    }
    info!(target: "termul::web::listeners", port = lan.addr.port(), "LAN listener off");
}

/// The addresses a device on the network can open.
fn lan_urls(port: u16) -> Vec<String> {
    crate::server_qr::detect_lan_ipv4()
        .map(|ip| vec![format!("http://{ip}:{port}/")])
        .unwrap_or_default()
}

/// The LAN listener's config: all interfaces, the host's tuning, and the
/// host's `--allow-remote-writes`.
fn lan_config(port: u16, parts: &ListenerParts) -> ServerConfig {
    ServerConfig {
        host: RemoteBindMode::All.host().to_string(),
        port,
        event_log_capacity: parts.relay.event_log_capacity(),
        agent_idle_window_secs: crate::web::config::desktop_agent_idle_window_secs().secs,
        agent_idle_window_locked: crate::web::config::desktop_agent_idle_window_secs().from_env,
        overflow_tolerance: crate::web::config::DEFAULT_OVERFLOW_TOLERANCE,
        max_terminals_per_project: crate::web::config::DEFAULT_MAX_TERMINALS_PER_PROJECT,
        receipt_retention_secs: parts.relay.ledger().retention().as_secs(),
        partition_gc_age_secs: crate::web::config::DEFAULT_PARTITION_GC_AGE_SECS,
        partition_byte_budget: crate::web::config::DEFAULT_PARTITION_BYTE_BUDGET,
        deprecation_warnings: Vec::new(),
        seed_project_root: None,
        projects_file: None,
        sessions_dir: None,
        acp_catalog_dir: None,
        store_file: None,
        allow_remote_writes: parts.allow_remote_writes,
        web_auth_token: None,
        allowed_origins: crate::web::origin::OriginPolicy::default(),
        state_dir: None,
        explicit_config: true,
        host_explicit: true,
        rune_web_dir: None,
    }
}
