//! `troupe-server --browser-host`: register an agent browser host backed by
//! a Chromium on this host, so the `browser` tool works without the desktop
//! app. Off by default; without the flag the tool keeps answering
//! `capability_unavailable`.
//!
//! The host reuses the desktop's automation core unchanged: origin consent
//! tickets go to the `/ws` clients through the permission rendezvous, the
//! browser policy store lives under the state dir, and every request a tab
//! makes passes the Chromium perimeter proxy. `browser_host` owns the
//! processes (a profile per project, idle shutdown) and the screencast
//! streams the `/ws` pane requests use.

use std::path::Path;
use std::sync::Arc;

use crate::acp::browser_policy::{self, BrowserPolicyService, LateProfileControl};
use crate::acp::host_ticket::HostTickets;
use crate::acp::session_persistence::UNASSIGNED_PROJECT_ID;
use crate::acp::SessionPersistence;
use crate::browser_automation::core::{ProjectResolver, RootResolver};
use crate::browser_automation::engine::chromium::perimeter::{InternalAddresses, Perimeter};
use crate::browser_automation::engine::chromium::{launch, ChromiumEngine, Tabs};
use crate::browser_automation::hosted::HostedSurface;
use crate::browser_automation::{self, DesktopBrowserHost, HostServices};
use crate::browser_host::BrowserHost;
use crate::web::desktop_mode::BrowserHostOptions;
use crate::web::{EventSink, ProjectRegistry, WsRelaySink};

/// State-dir folder of everything the browser host writes.
const ROOT_DIR: &str = "browser-host";

/// Find a browser, start the perimeter and register the host. On error
/// the server keeps running without a browser host.
///
/// # Errors
/// No browser on this host, or the perimeter could not bind.
pub async fn install(
    options: BrowserHostOptions,
    state_dir: &Path,
    relay: &Arc<WsRelaySink>,
    persistence: &Arc<SessionPersistence>,
    registry: &Arc<ProjectRegistry>,
) -> Result<(), String> {
    let binary = launch::find().ok_or_else(|| {
        format!(
            "no Chrome, Edge or Chromium found; install one or set {}",
            launch::PATH_VAR
        )
    })?;
    let internal = if options.allow_internal {
        InternalAddresses::Allow
    } else {
        InternalAddresses::Deny
    };
    let perimeter = Perimeter::start(internal)
        .await
        .map_err(|e| format!("could not start the browser perimeter: {e}"))?;
    let root = state_dir.join(ROOT_DIR);
    let policy = perimeter.policy();
    let processes = BrowserHost::new(
        binary,
        state_dir.to_path_buf(),
        root.join("profiles"),
        options.no_sandbox,
        perimeter,
        Arc::clone(relay),
    );
    let tabs = Arc::new(Tabs::new(processes.clone(), policy));
    processes.attach(&tabs);
    crate::browser_host::install(processes);
    let sinks: Vec<Arc<dyn EventSink>> = vec![relay.clone()];

    let tickets = Arc::new(HostTickets::for_relay(Arc::clone(relay)));
    match relay.rendezvous() {
        Some(rendezvous) => rendezvous.set_host_tickets(Arc::clone(&tickets)),
        None => return Err("the permission rendezvous is not attached".to_string()),
    }
    let profile_control = Arc::new(LateProfileControl::default());
    let policy_service = Arc::new(BrowserPolicyService::new(
        root.join("policy"),
        Arc::new(crate::host_perimeter::owned_ports),
        sinks.clone(),
        profile_control.clone(),
    ));
    browser_policy::install(Arc::clone(&policy_service));

    let host = DesktopBrowserHost::hosted(
        Arc::new(HostedSurface {
            tabs: tabs.clone(),
            sinks: sinks.clone(),
            artifacts: root.join("artifacts"),
            sessions: root.join("sessions"),
        }),
        Arc::new(ChromiumEngine::new(tabs)),
        sinks,
        HostServices {
            permissions: tickets,
            policy: policy_service,
            project_of: project_of(persistence),
            root_of: root_of(persistence, registry),
        },
    );
    profile_control.attach(host.profile_control());
    browser_automation::set_browser_host(host);
    tracing::info!(
        target: "termul::web::hosted_browser",
        internal = ?internal,
        no_sandbox = options.no_sandbox,
        "browser host registered"
    );
    Ok(())
}

/// The session's project, as the desktop resolves it.
fn project_of(persistence: &Arc<SessionPersistence>) -> ProjectResolver {
    let persistence = Arc::clone(persistence);
    Arc::new(move |session_id| {
        persistence
            .metadata(session_id)
            .ok()
            .and_then(|meta| meta.project_id)
            .unwrap_or_else(|| UNASSIGNED_PROJECT_ID.to_string())
    })
}

/// The folder a session's uploads come from: its project's root, else its
/// own absolute working directory.
fn root_of(persistence: &Arc<SessionPersistence>, registry: &Arc<ProjectRegistry>) -> RootResolver {
    let (persistence, registry) = (Arc::clone(persistence), Arc::clone(registry));
    Arc::new(move |session_id| {
        let meta = persistence.metadata(session_id).ok()?;
        match meta.project_id {
            Some(project_id) => registry.canonical_root(&project_id),
            None => Some(std::path::PathBuf::from(meta.cwd)).filter(|cwd| cwd.is_absolute()),
        }
    })
}

/// The browser conformance runner's host: the same parts on throwaway
/// profiles under `data`, with the runner's loopback fixtures allowed.
/// Returns a second engine over the same tabs for the input cases, or
/// `None` when this machine has no browser.
#[cfg(feature = "conformance")]
pub async fn conformance_host(
    data: &Path,
    sinks: Vec<Arc<dyn EventSink>>,
    services: HostServices,
) -> Option<Arc<dyn crate::browser_automation::engine::BrowserEngine>> {
    use crate::browser_automation::engine::chromium::LocalSource;

    let binary = launch::find()?;
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(data)
            .ok()?;
        let _ = std::fs::set_permissions(data, std::fs::Permissions::from_mode(0o700));
    }
    let perimeter = Perimeter::start(InternalAddresses::Allow).await.ok()?;
    let policy = perimeter.policy();
    // A throwaway profile: the sandbox is not what the suite checks.
    let source = Arc::new(LocalSource::new(
        binary,
        data.to_path_buf(),
        data.join("profiles"),
        true,
        perimeter,
    ));
    let tabs = Arc::new(Tabs::new(source, policy));
    let host = DesktopBrowserHost::hosted(
        Arc::new(HostedSurface {
            tabs: tabs.clone(),
            sinks: sinks.clone(),
            artifacts: data.join("artifacts"),
            sessions: data.join("browser-sessions"),
        }),
        Arc::new(ChromiumEngine::new(tabs.clone())),
        sinks,
        services,
    );
    browser_automation::set_browser_host(host);
    Some(Arc::new(ChromiumEngine::new(tabs)))
}
