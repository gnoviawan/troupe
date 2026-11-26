//! The file viewers of an editor tab (`components/editor/media/`): images, PDFs and videos.
//! A tab whose file is media mounts a [`Viewer`] here; the app reads the file's bytes
//! (`GET /fs/raw`) and hands them back through [`Viewers::loaded`]. [`view`] draws one file.
//!
//! An image decodes on rune's image workers. A PDF is parsed and its pages rasterised on the
//! CPU by hayro (`docs/rune/decisions.md`, "PDF engine"). A video plays through [`video`]:
//! ffmpeg into a `NativeSurface` on the desktop, the browser's own decoder on the web. On the
//! desktop a PDF or video opens on a thread of its own, and [`Viewers::pump`] takes it in.

mod pdf;
mod view;
pub mod video;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use model::Level;
use model::editor::Unavailable;
use model::editor::media::MediaKind;
use rune::{Image, ImageCache, Waker};

pub use model::editor::media::viewer_for_path;
pub use pdf::PdfDoc;
pub use view::view;

/// `ImageViewer`'s zoom: fit the pane, or a scale of the natural size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    Fit,
    Scale(f32),
}

pub(crate) const ZOOM_STEP: f32 = 1.25;
pub(crate) const MIN_SCALE: f32 = 0.05;
pub(crate) const MAX_SCALE: f32 = 16.0;

/// What a viewer shows.
pub enum Content {
    Image(Image),
    Pdf(PdfDoc),
    Video(video::Player),
}

pub enum Load {
    Loading,
    Ready { size: u64, content: Content },
    Unavailable(Unavailable),
}

/// One media tab.
pub struct Viewer {
    pub kind: MediaKind,
    pub load: Load,
    pub zoom: Zoom,
    /// The viewing area's size: the PDF fits its pages to the width, an image fits both.
    pub width: f32,
    pub area_h: f32,
    /// Which read this viewer waits for: an open that lands for an older one is dropped.
    ticket: u64,
}

/// A PDF or video opened off the UI thread, for the viewer of `path` that asked with `ticket`.
struct Opened {
    path: String,
    ticket: u64,
    size: u64,
    result: Result<Staged, String>,
}

enum Staged {
    Pdf(pdf::Parsed),
    Video(video::Player),
}

/// Parses the PDF or stages the video: the slow part of opening, run off the UI thread.
fn stage(kind: MediaKind, path: &str, bytes: Vec<u8>, wake: Option<Waker>) -> Result<Staged, String> {
    match kind {
        MediaKind::Pdf => pdf::parse(bytes).map(Staged::Pdf),
        MediaKind::Video => video::Player::open(path, bytes, wake).map(Staged::Video),
        MediaKind::Image => Err("an image decodes on the image workers".to_owned()),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// `GET /fs/raw?path=`.
    Read { path: String },
    Log { level: Level, message: String },
}

/// Every open media file, by path.
#[derive(Default)]
pub struct Viewers {
    pub files: BTreeMap<String, Viewer>,
    pub effects: Vec<Effect>,
    /// Decodes images and PDF pages; the app installs it with the window's waker.
    pub images: Option<ImageCache>,
    /// Wakes the window when a video frame, an opened file or a PDF page lands.
    pub waker: Option<Waker>,
    opened: Arc<Mutex<Vec<Opened>>>,
    tickets: u64,
}

fn name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

impl Viewers {
    /// Mounts a viewer for `path` and asks for its bytes.
    pub fn open(&mut self, path: &str, kind: MediaKind) {
        if self.files.contains_key(path) {
            return;
        }
        self.tickets += 1;
        let ticket = self.tickets;
        self.files.insert(path.to_owned(), Viewer { kind, load: Load::Loading, zoom: Zoom::Fit, width: 0.0, area_h: 0.0, ticket });
        self.effects.push(Effect::Read { path: path.to_owned() });
    }

    pub fn close(&mut self, path: &str) {
        if let Some(viewer) = self.files.remove(path)
            && let Load::Ready { content: Content::Video(player), .. } = viewer.load
        {
            player.stop();
        }
    }

    /// The read answered: the bytes, or the host's code and message.
    pub fn loaded(&mut self, path: &str, result: Result<Vec<u8>, (String, String)>) {
        let Some(viewer) = self.files.get_mut(path) else { return };
        let log = |level, message| Effect::Log { level, message };
        match result {
            Err((code, message)) => {
                self.effects.push(log(Level::Warn, format!("viewer read of {} refused ({code})", name(path))));
                viewer.load = Load::Unavailable(model::editor::media::unavailable(&code, &message));
            }
            Ok(bytes) => {
                let size = bytes.len() as u64;
                if viewer.kind == MediaKind::Image {
                    let content = match &self.images {
                        Some(cache) => Ok(Content::Image(cache.load(bytes))),
                        None => Err("no image decoder".to_owned()),
                    };
                    self.ready(path, size, content);
                    return;
                }
                let (kind, ticket, wake, opened) = (viewer.kind, viewer.ticket, self.waker.clone(), self.opened.clone());
                let path = path.to_owned();
                spawn_or_run(move || {
                    let result = stage(kind, &path, bytes, wake.clone());
                    opened.lock().unwrap_or_else(PoisonError::into_inner).push(Opened { path, ticket, size, result });
                    if let Some(wake) = wake {
                        wake();
                    }
                });
            }
        }
    }

    /// Shows what opened, or why it could not.
    fn ready(&mut self, path: &str, size: u64, content: Result<Content, String>) {
        let Some(viewer) = self.files.get_mut(path) else { return };
        let log = |level, message| Effect::Log { level, message };
        viewer.load = match content {
            Ok(content) => {
                self.effects.push(log(Level::Info, format!("viewer opened {} {size} bytes", name(path))));
                Load::Ready { size, content }
            }
            Err(message) => {
                self.effects.push(log(Level::Warn, format!("viewer could not open {}: {message}", name(path))));
                Load::Unavailable(Unavailable::Unreadable(message))
            }
        };
    }

    /// Takes in the files opened and the PDF pages rasterised since the last call; true when
    /// the view changed.
    pub fn pump(&mut self) -> bool {
        let opened = std::mem::take(&mut *self.opened.lock().unwrap_or_else(PoisonError::into_inner));
        let mut changed = false;
        for Opened { path, ticket, size, result } in opened {
            if self.files.get(&path).is_none_or(|v| v.ticket != ticket) {
                continue;
            }
            let content = result.map(|staged| match staged {
                Staged::Pdf(parsed) => Content::Pdf(PdfDoc::new(parsed, self.waker.clone(), true)),
                Staged::Video(player) => Content::Video(player),
            });
            self.ready(&path, size, content);
            changed = true;
        }
        if let Some(cache) = &self.images {
            for viewer in self.files.values() {
                if let Load::Ready { content: Content::Pdf(doc), .. } = &viewer.load {
                    changed |= doc.land(cache);
                }
            }
        }
        changed
    }

    /// The image or PDF zoom of `path`, stepped by `direction` (1 in, -1 out) from `shown`, the
    /// scale on screen.
    pub fn zoom_by(&mut self, path: &str, direction: i32, shown: f32) {
        let Some(viewer) = self.files.get_mut(path) else { return };
        viewer.zoom = Zoom::Scale(match viewer.kind {
            MediaKind::Pdf => pdf::next_step(shown, direction),
            _ => {
                let next = if direction > 0 { shown * ZOOM_STEP } else { shown / ZOOM_STEP };
                next.clamp(MIN_SCALE, MAX_SCALE)
            }
        });
    }

    /// Whether a video is playing, so its clock in the footer moves.
    #[must_use]
    pub fn playing(&self) -> bool {
        self.files.values().any(|v| matches!(&v.load, Load::Ready { content: Content::Video(p), .. } if p.playing()))
    }

    pub fn set_zoom(&mut self, path: &str, zoom: Zoom) {
        if let Some(viewer) = self.files.get_mut(path) {
            viewer.zoom = zoom;
        }
    }
}

/// Runs `job` on a thread of its own where the target has threads, else now.
#[cfg(not(target_arch = "wasm32"))]
fn spawn_or_run(job: impl FnOnce() + Send + 'static) {
    let job = Arc::new(Mutex::new(Some(job)));
    let queued = job.clone();
    let spawned = std::thread::Builder::new().name("troupe-viewer-open".into()).spawn(move || {
        if let Some(job) = queued.lock().unwrap_or_else(PoisonError::into_inner).take() {
            job();
        }
    });
    if let Err(e) = spawned {
        log::warn!("[viewer] could not start an open thread, opening inline: {e}");
        if let Some(job) = job.lock().unwrap_or_else(PoisonError::into_inner).take() {
            job();
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn spawn_or_run(job: impl FnOnce()) {
    job();
}
