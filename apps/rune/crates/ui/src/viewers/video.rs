//! Video playback for the viewer (`VideoViewer.tsx`). The desktop decodes with an ffmpeg
//! subprocess into a `NativeSurface` as NV12 frames, paced on the player's own clock
//! (`docs/rune/decisions.md`, "Video on Linux"); there is no sound yet. The web hands the bytes
//! to a hidden `<video>` element, which plays with sound, and copies each frame into
//! `Pixels`, so the picture is drawn by Rune and menus stay over it.

/// What the footer lists (`VideoMetadata`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Meta {
    pub width: u32,
    pub height: u32,
    /// Seconds, when known.
    pub duration: Option<f64>,
}

/// `formatDuration`: `m:ss`, or `h:mm:ss` from an hour.
#[must_use]
pub fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

pub use imp::Player;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::io::Read;
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use rune::native_surface::{ByteFrame, ColorInfo, Format, Matrix, Transfer};
    use rune::{El, NativeSurface, SurfaceFit, Waker};

    use super::Meta;

    struct Shared {
        file: PathBuf,
        meta: Meta,
        fps: f64,
        surface: NativeSurface,
        /// Seconds into the clip the last pushed frame shows.
        position: Mutex<f64>,
        playing: AtomicBool,
        /// Bumped by each play and stop; a decode thread whose number is stale ends.
        run: AtomicU64,
        child: Mutex<Option<Child>>,
    }

    pub struct Player(Arc<Shared>);

    fn probe(file: &PathBuf) -> Result<(Meta, f64), String> {
        let out = Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,r_frame_rate:format=duration", "-of", "default=nw=1"])
            .arg(file)
            .output()
            .map_err(|e| format!("ffprobe could not start: {e}"))?;
        if !out.status.success() {
            return Err(format!("ffprobe refused the file: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(k)?.strip_prefix('=').map(str::to_owned));
        let width: u32 = field("width").and_then(|v| v.parse().ok()).ok_or("the file has no video stream")?;
        let height: u32 = field("height").and_then(|v| v.parse().ok()).ok_or("the file has no video stream")?;
        let fps = field("r_frame_rate")
            .and_then(|r| {
                let (n, d) = r.split_once('/')?;
                let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
                (d > 0.0 && n > 0.0).then_some(n / d)
            })
            .unwrap_or(30.0);
        let duration = field("duration").and_then(|v| v.parse().ok());
        // NV12 needs even sides; ffmpeg scales to them below.
        Ok((Meta { width: width & !1, height: height & !1, duration }, fps))
    }

    fn decoder(shared: &Shared, from: f64, frames: Option<u32>) -> Result<Child, String> {
        let mut c = Command::new("ffmpeg");
        c.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-ss", &format!("{from:.3}")]).arg("-i").arg(&shared.file);
        if let Some(n) = frames {
            c.args(["-frames:v", &n.to_string()]);
        }
        let (w, h) = (shared.meta.width, shared.meta.height);
        c.args(["-an", "-vf", &format!("scale={w}:{h}"), "-f", "rawvideo", "-pix_fmt", "nv12", "pipe:1"]);
        c.stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("ffmpeg could not start: {e}"))
    }

    fn frame(meta: Meta, mut planes: Vec<u8>) -> ByteFrame {
        let uv = planes.split_off((meta.width * meta.height) as usize);
        ByteFrame {
            width: meta.width,
            height: meta.height,
            format: Format::Nv12Video,
            color: ColorInfo { matrix: Matrix::Bt709, transfer: Transfer::Bt709 },
            planes: [planes, uv],
        }
    }

    fn frame_len(meta: Meta) -> usize {
        (meta.width * meta.height * 3 / 2) as usize
    }

    impl Player {
        /// Writes the clip to a temporary file, reads its size and length, and shows its
        /// first frame.
        pub fn open(path: &str, bytes: Vec<u8>, wake: Option<Waker>) -> Result<Self, String> {
            let ext = path.rsplit('.').next().unwrap_or("mp4");
            let file = std::env::temp_dir().join(format!("troupe-video-{}-{:x}.{ext}", std::process::id(), seahash(&bytes)));
            std::fs::write(&file, &bytes).map_err(|e| format!("could not stage the clip: {e}"))?;
            let (meta, fps) = probe(&file)?;
            let surface = NativeSurface::new();
            if let Some(wake) = wake {
                surface.set_waker(wake);
            }
            let shared = Arc::new(Shared {
                file,
                meta,
                fps,
                surface,
                position: Mutex::new(0.0),
                playing: AtomicBool::new(false),
                run: AtomicU64::new(0),
                child: Mutex::new(None),
            });
            let mut first = decoder(&shared, 0.0, Some(1))?;
            let mut buf = vec![0; frame_len(meta)];
            let read = first.stdout.take().map(|mut out| out.read_exact(&mut buf));
            let _ = first.wait();
            if matches!(read, Some(Ok(()))) {
                let _ = shared.surface.push_bytes(frame(meta, buf), 0);
            }
            log::info!("[viewer] video {}x{} {:.1} fps {:?} s", meta.width, meta.height, fps, meta.duration);
            Ok(Self(shared))
        }

        #[must_use]
        pub fn meta(&self) -> Meta {
            self.0.meta
        }

        #[must_use]
        pub fn playing(&self) -> bool {
            self.0.playing.load(Ordering::Relaxed)
        }

        /// Seconds into the clip.
        #[must_use]
        pub fn position(&self) -> f64 {
            *self.0.position.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        }

        /// Play from where it stopped (from the start once it ended), or pause.
        pub fn toggle(&self) {
            if self.playing() {
                self.stop();
                return;
            }
            let shared = self.0.clone();
            let run = shared.run.fetch_add(1, Ordering::Relaxed) + 1;
            let mut from = self.position();
            if shared.meta.duration.is_some_and(|d| from >= d - 0.05) {
                from = 0.0;
            }
            let mut child = match decoder(&shared, from, None) {
                Ok(child) => child,
                Err(e) => {
                    log::warn!("[viewer] video play failed: {e}");
                    return;
                }
            };
            let Some(mut out) = child.stdout.take() else { return };
            *shared.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(child);
            shared.playing.store(true, Ordering::Relaxed);
            let spawned = std::thread::Builder::new().name("troupe-video".into()).spawn(move || {
                let started = Instant::now();
                let mut n: u64 = 0;
                loop {
                    let mut buf = vec![0; frame_len(shared.meta)];
                    if out.read_exact(&mut buf).is_err() || shared.run.load(Ordering::Relaxed) != run {
                        break;
                    }
                    // The player's own presentation clock: frame n shows at n / fps.
                    let due = Duration::from_secs_f64(n as f64 / shared.fps);
                    if let Some(wait) = due.checked_sub(started.elapsed()) {
                        std::thread::sleep(wait);
                    }
                    let _ = shared.surface.push_bytes(frame(shared.meta, buf), run << 32 | n);
                    *shared.position.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = from + n as f64 / shared.fps;
                    n += 1;
                }
                if shared.run.load(Ordering::Relaxed) == run {
                    shared.playing.store(false, Ordering::Relaxed);
                    log::info!("[viewer] video ended after {n} frames");
                }
            });
            if let Err(e) = spawned {
                log::warn!("[viewer] video thread failed: {e}");
                self.stop();
            }
        }

        /// Pauses and ends the decoder.
        pub fn stop(&self) {
            self.0.run.fetch_add(1, Ordering::Relaxed);
            self.0.playing.store(false, Ordering::Relaxed);
            if let Some(mut child) = self.0.child.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        /// `el` showing the picture.
        pub fn show(&self, el: El) -> El {
            el.native_surface(&self.0.surface, SurfaceFit::Fit)
        }
    }

    impl Drop for Shared {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.file);
        }
    }

    /// A content hash for the staged file's name, so two tabs of one clip share nothing stale.
    fn seahash(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3))
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::Cell;
    use std::rc::Rc;

    use rune::{El, Pixels, SurfaceFit, Waker};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::{Closure, JsValue};
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlVideoElement};

    use super::Meta;

    struct Inner {
        video: HtmlVideoElement,
        url: String,
        pixels: Pixels,
        timer: Cell<Option<i32>>,
        tick: Closure<dyn FnMut()>,
    }

    pub struct Player(Rc<Inner>);

    fn copy(video: &HtmlVideoElement, canvas: &HtmlCanvasElement, pixels: &Pixels) {
        let (w, h) = (video.video_width(), video.video_height());
        if w == 0 || h == 0 {
            return;
        }
        if canvas.width() != w || canvas.height() != h {
            canvas.set_width(w);
            canvas.set_height(h);
        }
        let Some(ctx) = canvas.get_context("2d").ok().flatten().and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok()) else {
            return;
        };
        if ctx.draw_image_with_html_video_element(video, 0.0, 0.0).is_err() {
            return;
        }
        if let Ok(data) = ctx.get_image_data(0.0, 0.0, f64::from(w), f64::from(h)) {
            pixels.set(w, h, data.data().0);
        }
    }

    impl Player {
        /// Hands the clip to a hidden `<video>` and shows its first frame once it loads.
        pub fn open(path: &str, bytes: Vec<u8>, _wake: Option<Waker>) -> Result<Self, String> {
            let js = |e: JsValue| format!("{e:?}");
            let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
            let mime = model::editor::media::mime_for_path(path).unwrap_or("video/mp4");
            let array = js_sys::Uint8Array::from(bytes.as_slice());
            let parts = js_sys::Array::of1(&array);
            let options = web_sys::BlobPropertyBag::new();
            options.set_type(mime);
            let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(js)?;
            let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
            let video: HtmlVideoElement = document.create_element("video").map_err(js)?.dyn_into().map_err(|_| "not a video element")?;
            video.set_muted(false);
            video.set_preload("auto");
            video.set_src(&url);
            let canvas: HtmlCanvasElement = document.create_element("canvas").map_err(js)?.dyn_into().map_err(|_| "not a canvas")?;
            let pixels = Pixels::new();
            let tick = {
                let (video, canvas, pixels) = (video.clone(), canvas.clone(), pixels.clone());
                Closure::<dyn FnMut()>::new(move || copy(&video, &canvas, &pixels))
            };
            video.set_onloadeddata(Some(tick.as_ref().unchecked_ref()));
            video.set_onseeked(Some(tick.as_ref().unchecked_ref()));
            Ok(Self(Rc::new(Inner { video, url, pixels, timer: Cell::new(None), tick })))
        }

        #[must_use]
        pub fn meta(&self) -> Meta {
            let v = &self.0.video;
            let duration = v.duration();
            Meta { width: v.video_width(), height: v.video_height(), duration: duration.is_finite().then_some(duration) }
        }

        #[must_use]
        pub fn playing(&self) -> bool {
            !self.0.video.paused() && !self.0.video.ended()
        }

        #[must_use]
        pub fn position(&self) -> f64 {
            self.0.video.current_time()
        }

        pub fn toggle(&self) {
            let inner = &self.0;
            if self.playing() {
                self.stop();
                return;
            }
            let _ = inner.video.play();
            if inner.timer.get().is_none()
                && let Some(window) = web_sys::window()
                && let Ok(id) = window.set_interval_with_callback_and_timeout_and_arguments_0(inner.tick.as_ref().unchecked_ref(), 33)
            {
                inner.timer.set(Some(id));
            }
        }

        pub fn stop(&self) {
            let _ = self.0.video.pause();
            if let (Some(id), Some(window)) = (self.0.timer.take(), web_sys::window()) {
                window.clear_interval_with_handle(id);
            }
        }

        pub fn show(&self, el: El) -> El {
            el.pixels_fit(&self.0.pixels, SurfaceFit::Fit)
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            if let (Some(id), Some(window)) = (self.timer.take(), web_sys::window()) {
                window.clear_interval_with_handle(id);
            }
            let _ = self.video.pause();
            self.video.set_onloadeddata(None);
            self.video.set_onseeked(None);
            self.video.set_src("");
            let _ = web_sys::Url::revoke_object_url(&self.url);
        }
    }
}
