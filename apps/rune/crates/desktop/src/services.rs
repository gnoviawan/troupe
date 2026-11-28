//! The OS services the desktop app adds to the shell: the tray (Show, Quit), system
//! notifications (`notify-rust` on Linux, `rune-notify` on macOS and Windows), the opener for
//! folders, and the Help menu's log actions. File dialogs and the clipboard are Rune's own
//! (`Tx::save_file`, `Tx::copy`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use rune::{AppHandle, FileDialogOptions, Tray, TrayEvent, TrayIcon, TrayItem, TraySpec, Tx};
use troupe_app::State;
use ui::overlay::ToastKind;

use crate::logs;

static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();

pub fn set_log_file(path: PathBuf) {
    let _ = LOG_FILE.set(path);
}

fn log_file() -> PathBuf {
    LOG_FILE.get().cloned().unwrap_or_else(|| PathBuf::from(logs::FILE_NAME))
}

/// Opens `path` (a folder or a file) in the system's file manager or default app.
pub fn open_path(path: &Path) -> Result<(), String> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    log::info!("[opener] {program} {}", path.display());
    Command::new(program).arg(path).spawn().map(|_| ()).map_err(|e| format!("{program}: {e}"))
}

pub fn reveal_logs(s: &mut State) {
    let file = log_file();
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    if let Err(e) = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|()| open_path(&dir)) {
        s.toast_error("Could not reveal the log folder", &e);
    }
}

pub fn export_log(tx: &mut Tx) {
    let options = FileDialogOptions { title: "Export Log File".into(), file_name: Some(logs::FILE_NAME.into()), ..Default::default() }
        .filter("Log Files", &["log"]);
    tx.save_file(options, |s: &mut State, answer, _tx| match answer {
        Ok(Some(dest)) => exported(s, logs::export(&log_file(), &dest).map(|()| dest)),
        Ok(None) => log::info!("[logs] export cancelled"),
        Err(e) => s.toast_error("Could not open the save dialog", &e.to_string()),
    });
}

pub fn export_log_default(s: &mut State) {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let Some(dir) = logs::default_export_dir(env) else {
        s.toast_error("Could not export the log", "No Downloads or Desktop folder");
        return;
    };
    exported(s, logs::export_new(&log_file(), &dir, logs::FILE_NAME));
}

fn exported(s: &mut State, result: Result<PathBuf, String>) {
    match result {
        Ok(dest) => {
            log::info!("[logs] exported to {}", dest.display());
            let detail = dest.display().to_string();
            s.toasts.push(s.now, ToastKind::Success, "Log file exported", Some(&detail));
        }
        Err(e) => s.toast_error("Could not export the log", &e),
    }
}

pub fn copy_log(s: &mut State, tx: &mut Tx) {
    match logs::contents(&log_file()) {
        Ok(text) => {
            tx.copy(text);
            s.toasts.push(s.now, ToastKind::Success, "Log contents copied", None);
        }
        Err(e) => s.toast_error("Could not copy the log", &e),
    }
}

/// Posts a system notification; false where none can show.
pub fn notify(title: &str, body: &str) -> bool {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        match notify_rust::Notification::new().appname("Troupe").summary(title).body(body).show() {
            Ok(_) => true,
            Err(e) => {
                log::warn!("[notify] {e}");
                false
            }
        }
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        #[cfg(target_os = "windows")]
        {
            static REGISTERED: OnceLock<bool> = OnceLock::new();
            let identity = rune_notify::Identity::new("com.termul-manager.preview", "Troupe Preview");
            if !*REGISTERED.get_or_init(|| rune_notify::register(&identity).inspect_err(|e| log::warn!("[notify] {e}")).is_ok()) {
                return false;
            }
        }
        rune_notify::post(&rune_notify::Notification::new("troupe-preview", title).body(body))
    }
}

/// A 32 px tray icon: the Troupe mark's colour as a rounded square.
fn icon() -> TrayIcon {
    const SIZE: u32 = 32;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = ((x as i32 - 15).abs().max(4) - 4, (y as i32 - 15).abs().max(4) - 4);
            let inside = dx * dx + dy * dy <= 12 * 12;
            pixels.extend_from_slice(if inside { &[0xe8, 0x6a, 0x33, 0xff] } else { &[0, 0, 0, 0] });
        }
    }
    TrayIcon::rgba(SIZE, SIZE, pixels)
}

/// The tray: a click or Show brings the window back, Quit ends the app and its server. None
/// where the platform has no tray (macOS).
pub fn tray(handle: AppHandle<State>) -> Option<Tray> {
    let spec = TraySpec::new("troupe-preview", "Troupe", icon()).tooltip("Troupe").menu(vec![
        TrayItem::action("tray-show", "Show Troupe"),
        TrayItem::Separator,
        TrayItem::action("tray-quit", "Quit Troupe"),
    ]);
    let tray = Tray::new(spec, move |event| match event {
        TrayEvent::Activate { token: Some(token) } => handle.activate(token),
        TrayEvent::Menu(id) if id == "tray-quit" => {
            log::info!("[tray] quit");
            handle.quit();
        }
        TrayEvent::Activate { token: None } | TrayEvent::Menu(_) => {
            log::info!("[tray] show");
            handle.open();
        }
        _ => {}
    });
    tray.inspect_err(|e| log::warn!("[tray] none: {e}")).ok()
}
