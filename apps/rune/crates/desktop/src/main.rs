//! `troupe-preview`: the Rune desktop app. One process per data dir (a second launch focuses
//! the first), a supervised local `troupe-server` on loopback (`launcher`), and the workspace
//! shell in a native window that opens hidden and shows on its first frame (`window`).
//!
//! It opens the Tauri app's data (`paths`), so it refuses to start while the Tauri app runs
//! (`data_lock`). `TROUPE_SERVER_URL` attaches to a server instead of managing one,
//! `TROUPE_PREVIEW_DATA_DIR` moves every file under one folder, and `RUNE_DRIVE_SOCK` links
//! the live driver (the `drive` feature). Headless runs (`--snap`, `--tree`) start no server.

#![deny(unsafe_code)]

mod data_lock;
mod launcher;
mod logs;
mod paths;
mod refusal;
mod window;

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
        run(Launch { config: None, route }, None, &paths);
        return;
    }
    rune::set_app_id("troupe-preview");
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
    log::info!("[startup] server {:?} at {}", started.mode, started.config.base_url);
    run(Launch { config: Some(started.config), route }, started.control.as_ref(), &paths);
    if let Some(control) = started.control {
        control.stop();
    }
    log::info!("[startup] quit");
    log::logger().flush();
}

fn run(launch: Launch, control: Option<&Control>, paths: &Paths) {
    let saved = window::load(&paths.window_file());
    window::track(&paths.window_file());
    let mut spec = troupe_app::spec(launch);
    spec.size = window::DEFAULT_SIZE;
    spec.min_size = Some(window::MIN_SIZE);
    let app = troupe_app::app_with(spec)
        .window_options(window::options(saved.as_ref()))
        .chrome(window::chrome())
        .maximized(saved.as_ref().is_some_and(|f| f.maximized))
        .on_window(window::created)
        .on_ready(|w| {
            w.show();
            w.focus();
            log::info!("[window] first frame ready; shown");
        })
        .on_close(|_, _| window::save())
        .on_quit(|_, _| window::save())
        .on_reopen(|_, _, _| {
            log::info!("[instance] a second launch asked for the window");
            launcher::retry();
            false
        });
    if let Some(control) = control {
        let handle = app.handle();
        control.on_notice(move |notice| handle.update(move |s, _| toast(s, notice)));
    }
    automation::install(app).run();
}

/// What the window says when the server stops and comes back.
fn toast(s: &mut State, notice: Notice) {
    let (kind, title, detail) = match notice {
        Notice::Restarting { status, .. } => {
            (ToastKind::Warning, "Troupe server stopped", format!("{status}. Restarting…"))
        }
        Notice::Recovered => (ToastKind::Success, "Troupe server restarted", String::new()),
        Notice::GaveUp { exits, .. } => (
            ToastKind::Error,
            "Troupe server keeps stopping",
            format!("It stopped {exits} times in a row. Open Troupe Preview again to retry; its log is server.log."),
        ),
    };
    s.toasts.push(s.now, kind, title, (!detail.is_empty()).then_some(detail.as_str()));
}
