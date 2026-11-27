//! The app's log: every `log` line, with a UTC time, level and target, appended to
//! `troupe-preview.log` in the Tauri app's log directory (`app_log_dir`), beside its
//! `termul.log`. A file past 5 MB at launch moves to `.prev`, as the Tauri app keeps one old
//! file. Debug builds also print to stderr. `RUST_LOG` sets the level (`info` by default: the
//! GPU and socket crates' debug lines fill megabytes a minute). Panics land in the file.
//!

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{LevelFilter, Log, Metadata, Record};

pub const FILE_NAME: &str = "troupe-preview.log";
const MAX_BYTES: u64 = 5 * 1024 * 1024;

struct FileLog {
    file: Option<Mutex<File>>,
    stderr: bool,
    level: LevelFilter,
}

impl Log for FileLog {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:<5} {} {}\n",
            timestamp(SystemTime::now()),
            record.level(),
            record.target(),
            record.args()
        );
        if let Some(file) = &self.file
            && let Ok(mut file) = file.lock()
        {
            let _ = file.write_all(line.as_bytes());
        }
        if self.stderr {
            eprint!("{line}");
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file
            && let Ok(mut file) = file.lock()
        {
            let _ = file.flush();
        }
    }
}

/// Installs the logger and the panic hook. A log file that can't open leaves stderr only, and
/// says why there.
pub fn init(log_dir: &Path) -> PathBuf {
    let path = log_dir.join(FILE_NAME);
    let file = open(&path);
    if let Err(e) = &file {
        eprintln!("[logs] cannot open {}: {e}; logging to stderr only", path.display());
    }
    let level = level(std::env::var("RUST_LOG").ok().as_deref());
    let logger = FileLog {
        stderr: cfg!(debug_assertions) || file.is_err(),
        file: file.ok().map(Mutex::new),
        level,
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("[panic] {info}");
        log::logger().flush();
        previous(info);
    }));
    path
}

fn open(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        fs::rename(path, path.with_extension("log.prev"))?;
    }
    OpenOptions::new().create(true).append(true).open(path)
}

/// The global level `RUST_LOG` names; a module-scoped directive keeps the default.
pub fn level(rust_log: Option<&str>) -> LevelFilter {
    rust_log
        .into_iter()
        .flat_map(|spec| spec.split([',', ' ']))
        .filter(|part| !part.contains('='))
        .filter_map(|part| part.trim().parse().ok())
        .next_back()
        .unwrap_or(LevelFilter::Info)
}

/// `2026-11-27T04:05:06.789Z`.
pub fn timestamp(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60,
        since.subsec_millis()
    )
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
#[path = "logs_tests.rs"]
mod tests;
