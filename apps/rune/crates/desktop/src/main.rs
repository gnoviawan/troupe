//! `troupe-preview`: the Rune desktop app. One process per data dir (a second launch focuses
//! the first), a supervised local `troupe-server` on loopback (`launcher`), and the workspace
//! shell in a native window that opens hidden and shows on its first frame (`window`), with
//! the app menu (`menu`), the tray, notifications and the log actions (`services`).
//!
//! It opens the Tauri app's data (`paths`), so it refuses to start while the Tauri app runs
//! (`data_lock`). `TROUPE_SERVER_URL` attaches to a server instead of managing one,
//! `TROUPE_PREVIEW_DATA_DIR` moves every file under one folder, and `RUNE_DRIVE_SOCK` links
//! the live driver (the `drive` feature). Headless runs (`--snap`, `--tree`) start no server.

#![deny(unsafe_code)]

mod data_lock;
mod launcher;
mod logs;
mod menu;
mod paths;
mod refusal;
mod services;
mod window;

use std::sync::Mutex;

use launcher::supervisor::Notice;
use launcher::{Control, LaunchError};
use paths::Paths;
use troupe_app::{Launch, Route, State};
use ui::overlay::ToastKind;

fn main() {
    let paths = match Paths::from_env() {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("troupe-preview: cannot find the data directory: {e}");
            std::process::exit(1);
        }
    };
    let log_file = logs::init(&paths.log_dir);
    services::set_log_file(log_file.clone());
    log::info!(
        "[startup] troupe-preview v{} | {} {} | pid {} | data={} | log={}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::process::id(),
        paths.data_dir.display(),
        log_file.display()
    );
    let route = Route::from_hash(&std::env::var("TROUPE_ROUTE").unwrap_or_default());
    if rune::is_headless() {
        log::info!("[startup] headless run; no server");
        build(Launch { config: None, route }, None, &paths).run();
        return;
    }
    rune::set_app_id("troupe-preview");
    // The instance id canonicalizes the folder, which needs it to exist.
    if let Err(e) = std::fs::create_dir_all(&paths.data_dir) {
        log::warn!("[startup] cannot create {}: {e}", paths.data_dir.display());
    }
    if !rune::single_instance(&paths.instance_id()) {
        log::info!("[instance] troupe-preview already runs on this data dir; it takes focus");
        return;
    }
    let started = match launcher::start(&paths) {
        Ok(started) => started,
        Err(LaunchError::Locked(holder)) => {
            refusal::show("Troupe is already running", &holder.message(&paths.data_dir));
            std::process::exit(1);
        }
        Err(LaunchError::Failed(why)) => {
            refusal::show("Troupe cannot start", &why);
            std::process::exit(1);
        }
    };
    log::info!("[startup] server {:?} at {}", started.mode, launcher::redact_url(&started.config.base_url));
    let app = build(Launch { config: Some(started.config), route }, started.control.as_ref(), &paths);
    let _tray = services::tray(app.handle());
    *CONTROL.lock().unwrap_or_else(|e| e.into_inner()) = started.control;
    // Rune ends the process right after the quit hooks, so the server stops in its last step.
    rune::fast_exit(stop_server);
    app.run();
    stop_server();
}

static CONTROL: Mutex<Option<Control>> = Mutex::new(None);

/// Stops a server the app started, once, as the app ends.
fn stop_server() {
    if let Some(control) = CONTROL.lock().unwrap_or_else(|e| e.into_inner()).take() {
        control.stop();
    }
    log::info!("[startup] quit");
    log::logger().flush();
}

fn build(launch: Launch, control: Option<&Control>, paths: &Paths) -> rune::App<State> {
    let saved = window::load(&paths.window_file());
    window::track(&paths.window_file());
    if paths::Os::CURRENT != paths::Os::Mac {
        troupe_app::set_shell_bar(menu::bar);
    }
    let mut spec = troupe_app::spec(launch);
    spec.size = window::DEFAULT_SIZE;
    spec.min_size = Some(window::MIN_SIZE);
    let mut app = troupe_app::app_with(spec)
        .window_options(window::options(saved.as_ref()))
        .chrome(window::chrome())
        .maximized(saved.as_ref().is_some_and(|f| f.maximized))
        .on_window(window::created)
        .on_ready(|w| {
            window::keep_on_screen(w);
            w.show();
            w.focus();
            log::info!("[window] first frame ready; shown");
        })
        .on_close(|_, _| window::save())
        .on_quit(|_, _| window::save())
        // Rune shows a hidden window on a second launch but leaves a showing one where it is;
        // the Tauri app shows, unminimizes and focuses it either way.
        .on_reopen(|_, _, _| {
            log::info!("[instance] a second launch asked for the window");
            launcher::retry();
            let Some(window) = window::handle() else { return false };
            window.open();
            window.focus();
            true
        });
    // Close hides the window on Linux and macOS, as the Tauri app hides to its tray there;
    // File > Quit and the tray's Quit end the app. Windows closes for good.
    if !cfg!(target_os = "windows") {
        app = app.keep_running();
    }
    if paths::Os::CURRENT == paths::Os::Mac {
        app = app.standard_menus();
        for item in menu::native_items() {
            app = app.menu_item(item);
        }
    }
    if let Some(control) = control {
        let handle = app.handle();
        control.on_notice(move |notice| {
            let (_, title, detail) = notice_text(&notice);
            if !handle.is_visible() {
                services::notify(title, &detail);
            }
            handle.update(move |s, _| toast(s, notice));
        });
    }
    automation::install(app)
}

/// What the window says when the server stops and comes back. A hidden window says it in a
/// system notification as well.
fn toast(s: &mut State, notice: Notice) {
    let (kind, title, detail) = notice_text(&notice);
    s.toasts.push(s.now, kind, title, (!detail.is_empty()).then_some(detail.as_str()));
}

fn notice_text(notice: &Notice) -> (ToastKind, &'static str, String) {
    match notice {
        Notice::Restarting { status, .. } => {
            (ToastKind::Warning, "Troupe server stopped", format!("{status}. Restarting…"))
        }
        Notice::Recovered => (ToastKind::Success, "Troupe server restarted", String::new()),
        Notice::GaveUp { exits, .. } => (
            ToastKind::Error,
            "Troupe server keeps stopping",
            format!("It stopped {exits} times in a row. Open Troupe Preview again to retry; its log is server.log."),
        ),
    }
}
