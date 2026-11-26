//! A PDF in the viewer (`PdfViewer.tsx`): its pages' sizes, and each page rasterised by hayro
//! at the scale it draws at. Pages rasterise on a thread of their own where the target has
//! threads, and only the pages drawn last stay, within [`BUDGET`] bytes.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};
use rune::{Image, ImageCache, Waker};

/// `ZOOM_STEPS`.
const ZOOM_STEPS: [f32; 16] = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0];
pub(crate) const MIN_SCALE: f32 = ZOOM_STEPS[0];
pub(crate) const MAX_SCALE: f32 = ZOOM_STEPS[ZOOM_STEPS.len() - 1];
/// Space beside the page at fit width (`PAGE_GUTTER`).
pub(crate) const PAGE_GUTTER: f32 = 32.0;
const EPSILON: f32 = 0.001;
/// The longest side a page rasterises to, in pixels; a larger page draws scaled up.
pub(crate) const MAX_SIDE: f32 = 8192.0;
/// Bytes of rasterised pages kept; the pages drawn longest ago go first.
const BUDGET: usize = 256 << 20;
/// Pages kept whatever their size, so the pages on screen never evict each other.
const MIN_KEPT: usize = 4;
/// Pages waiting for the rasteriser; past this the one drawn longest ago is dropped.
const MAX_PENDING: usize = 48;

/// The next zoom step past `scale` in `direction` (`nextStep`).
pub(crate) fn next_step(scale: f32, direction: i32) -> f32 {
    let mut steps = ZOOM_STEPS.to_vec();
    if direction < 0 {
        steps.reverse();
    }
    let d = direction.signum() as f32;
    steps.iter().copied().find(|s| d * (s - scale) > EPSILON).unwrap_or(steps[steps.len() - 1])
}

/// Pixels per point for a page of `size` points asked for at `px`: at most [`MAX_SIDE`] a side.
pub(crate) fn page_px(size: (f32, f32), px: f32) -> f32 {
    px.min(MAX_SIDE / size.0.max(size.1).max(1.0))
}

/// A parsed PDF and its pages' sizes in points, as drawn (rotation applied).
pub struct Parsed {
    pdf: Pdf,
    sizes: Vec<(f32, f32)>,
}

/// Parses `bytes`; a password or a damaged file is refused with `failureMessage`'s words.
pub fn parse(bytes: Vec<u8>) -> Result<Parsed, String> {
    let pdf = Pdf::new(bytes).map_err(|e| match e {
        LoadPdfError::Decryption(_) => "the PDF is password-protected".to_owned(),
        LoadPdfError::Invalid => "the PDF is damaged or not valid".to_owned(),
    })?;
    let sizes: Vec<(f32, f32)> = pdf.pages().iter().map(|p| p.render_dimensions()).collect();
    if sizes.is_empty() {
        return Err("the PDF has no pages".to_owned());
    }
    Ok(Parsed { pdf, sizes })
}

/// Page `index` as opaque RGBA at `px` pixels per PDF point, capped by [`page_px`]: its width,
/// height and bytes.
fn rasterise(pdf: &Pdf, index: usize, px: f32) -> Option<(u32, u32, Vec<u8>)> {
    let page = pdf.pages().get(index)?;
    let px = page_px(page.render_dimensions(), px);
    let started = clock();
    let pixmap = hayro::render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings { x_scale: px, y_scale: px, bg_color: WHITE },
    );
    let (w, h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    match (started, clock()) {
        (Some(a), Some(b)) => log::info!("[viewer] pdf page {} rendered {w}x{h} in {:.0} ms", index + 1, (b - a) * 1000.0),
        _ => log::info!("[viewer] pdf page {} rendered {w}x{h}", index + 1),
    }
    Some((w, h, pixmap.data_as_u8_slice().to_vec()))
}

/// A page to rasterise, dropped unrendered once `live` clears.
struct Job {
    index: usize,
    key: u32,
    live: Arc<AtomicBool>,
}

/// A rasterised page on its way to the UI thread.
struct Raster {
    index: usize,
    key: u32,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

type Landed = Arc<Mutex<Vec<Raster>>>;

/// Who rasterises: a thread fed jobs in order, or the caller, where there are no threads.
enum Rasteriser {
    #[cfg(not(target_arch = "wasm32"))]
    Thread(std::sync::mpsc::Sender<Job>),
    Inline,
}

/// One page's rasterisation: the scale it was last asked for, and the image it shows.
struct Page {
    wanted: u32,
    /// The rasterisation of `wanted` under way.
    pending: Option<Arc<AtomicBool>>,
    /// The scale key, image and byte size last rasterised; an image at an old scale stays
    /// drawn, stretched, until the new one lands.
    shown: Option<(u32, Image, usize)>,
    /// The tick it was last drawn at.
    used: u64,
}

impl Page {
    fn cancel(&self) {
        if let Some(live) = &self.pending {
            live.store(false, Ordering::Relaxed);
        }
    }
}

pub struct PdfDoc {
    pdf: Arc<Pdf>,
    /// Each page's size in points, as drawn (rotation applied).
    pub sizes: Vec<(f32, f32)>,
    /// The header's "page / pages", following the scroll.
    pub label: rune::LiveText,
    pages: RefCell<HashMap<usize, Page>>,
    tick: Cell<u64>,
    budget: usize,
    landed: Landed,
    rasteriser: Rasteriser,
}

impl PdfDoc {
    /// Parses `bytes` and rasterises on the calling thread.
    pub fn open(bytes: Vec<u8>) -> Result<Self, String> {
        parse(bytes).map(|parsed| Self::new(parsed, None, false))
    }

    /// A viewer for `parsed`; with `threaded`, pages rasterise on a thread that `wake`s the
    /// window as each lands.
    pub fn new(parsed: Parsed, wake: Option<Waker>, threaded: bool) -> Self {
        let Parsed { pdf, sizes } = parsed;
        let pdf = Arc::new(pdf);
        let landed = Landed::default();
        let rasteriser = if threaded { spawn(pdf.clone(), landed.clone(), wake) } else { Rasteriser::Inline };
        Self {
            label: rune::LiveText::new(format!("1 / {}", sizes.len())),
            pdf,
            sizes,
            pages: RefCell::default(),
            tick: Cell::new(0),
            budget: BUDGET,
            landed,
            rasteriser,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_budget(mut self, budget: usize) -> Self {
        self.budget = budget;
        self
    }

    #[must_use]
    pub fn pages(&self) -> usize {
        self.sizes.len()
    }

    /// The scale that fits the first page's width into `width` (`fitScale`).
    #[must_use]
    pub fn fit_scale(&self, width: f32) -> f32 {
        if width <= 0.0 {
            return 1.0;
        }
        ((width - PAGE_GUTTER) / self.sizes[0].0).clamp(MIN_SCALE, MAX_SCALE)
    }

    /// Page `index` drawn at `scale` points per PDF point on a `device` scale screen: the last
    /// image rasterised for it, while one at this scale is asked for. Rune decodes the pixels
    /// off the UI thread.
    pub fn page(&self, cache: &ImageCache, index: usize, scale: f32, device: f32) -> Option<Image> {
        if index >= self.sizes.len() {
            return None;
        }
        let px = (scale * device).max(0.05);
        let key = (px * 1000.0).round() as u32;
        let tick = self.tick.get() + 1;
        self.tick.set(tick);
        let ask = {
            let mut pages = self.pages.borrow_mut();
            let page = pages.entry(index).or_insert(Page { wanted: key, pending: None, shown: None, used: tick });
            page.used = tick;
            let have = page.shown.as_ref().is_some_and(|(k, ..)| *k == key);
            let asked = page.wanted == key && page.pending.as_ref().is_some_and(|l| l.load(Ordering::Relaxed));
            if have || asked {
                None
            } else {
                page.cancel();
                let live = Arc::new(AtomicBool::new(true));
                page.wanted = key;
                page.pending = Some(live.clone());
                Some(Job { index, key, live })
            }
        };
        if let Some(job) = ask {
            self.drop_stale_jobs();
            self.submit(job);
        }
        self.land(cache);
        self.pages.borrow().get(&index).and_then(|p| p.shown.as_ref().map(|(_, image, _)| image.clone()))
    }

    fn submit(&self, job: Job) {
        match &self.rasteriser {
            #[cfg(not(target_arch = "wasm32"))]
            Rasteriser::Thread(jobs) => {
                if let Err(std::sync::mpsc::SendError(job)) = jobs.send(job) {
                    run(&self.pdf, &self.landed, job);
                }
            }
            Rasteriser::Inline => run(&self.pdf, &self.landed, job),
        }
    }

    /// Past [`MAX_PENDING`] waiting pages, forgets the ones drawn longest ago.
    fn drop_stale_jobs(&self) {
        let mut pages = self.pages.borrow_mut();
        let mut waiting: Vec<(u64, usize)> = pages.iter().filter(|(_, p)| p.pending.is_some()).map(|(i, p)| (p.used, *i)).collect();
        if waiting.len() < MAX_PENDING {
            return;
        }
        waiting.sort_unstable();
        for (_, index) in &waiting[..=waiting.len() - MAX_PENDING] {
            if let Some(page) = pages.get_mut(index) {
                page.cancel();
                page.pending = None;
            }
        }
    }

    /// Takes in the pages rasterised since the last call, then evicts past the budget; true
    /// when one landed.
    pub fn land(&self, cache: &ImageCache) -> bool {
        let landed = std::mem::take(&mut *self.landed.lock().unwrap_or_else(PoisonError::into_inner));
        if landed.is_empty() {
            return false;
        }
        let mut pages = self.pages.borrow_mut();
        for r in landed {
            let Some(page) = pages.get_mut(&r.index) else { continue };
            if page.wanted == r.key {
                page.pending = None;
            }
            let bytes = r.rgba.len();
            page.shown = Some((r.key, cache.rgba(r.width, r.height, r.rgba), bytes));
        }
        evict(&mut pages, self.budget);
        true
    }

    /// The pages holding an image, in page order.
    #[cfg(test)]
    pub(crate) fn resident(&self) -> Vec<usize> {
        let mut held: Vec<usize> = self.pages.borrow().iter().filter(|(_, p)| p.shown.is_some()).map(|(i, _)| *i).collect();
        held.sort_unstable();
        held
    }

    /// Page `index` rasterised now at `px` pixels per PDF point: its width, height and bytes.
    #[cfg(test)]
    pub(crate) fn rasterise(&self, index: usize, px: f32) -> Option<(u32, u32, Vec<u8>)> {
        rasterise(&self.pdf, index, px)
    }
}

impl Drop for PdfDoc {
    fn drop(&mut self) {
        for page in self.pages.get_mut().values() {
            page.cancel();
        }
    }
}

/// Drops the images drawn longest ago while they hold more than `budget` bytes, keeping at
/// least [`MIN_KEPT`].
fn evict(pages: &mut HashMap<usize, Page>, budget: usize) {
    let mut held: Vec<(u64, usize, usize)> = pages.iter().filter_map(|(i, p)| p.shown.as_ref().map(|s| (p.used, *i, s.2))).collect();
    let mut total: usize = held.iter().map(|h| h.2).sum();
    held.sort_unstable();
    for (_, index, bytes) in held.iter().take(held.len().saturating_sub(MIN_KEPT)) {
        if total <= budget {
            break;
        }
        if let Some(page) = pages.remove(index) {
            page.cancel();
        }
        total -= bytes;
    }
}

fn run(pdf: &Pdf, landed: &Landed, job: Job) {
    if !job.live.load(Ordering::Relaxed) {
        return;
    }
    if let Some((width, height, rgba)) = rasterise(pdf, job.index, job.key as f32 / 1000.0) {
        landed.lock().unwrap_or_else(PoisonError::into_inner).push(Raster { index: job.index, key: job.key, width, height, rgba });
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn(pdf: Arc<Pdf>, landed: Landed, wake: Option<Waker>) -> Rasteriser {
    let (jobs, queue) = std::sync::mpsc::channel::<Job>();
    let spawned = std::thread::Builder::new().name("troupe-pdf".into()).spawn(move || {
        for job in queue {
            run(&pdf, &landed, job);
            if let Some(wake) = &wake {
                wake();
            }
        }
    });
    match spawned {
        Ok(_) => Rasteriser::Thread(jobs),
        Err(e) => {
            log::warn!("[viewer] pdf thread failed, pages rasterise inline: {e}");
            Rasteriser::Inline
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn spawn(_pdf: Arc<Pdf>, _landed: Landed, _wake: Option<Waker>) -> Rasteriser {
    Rasteriser::Inline
}

/// Seconds on a monotonic clock, where the target has one.
#[cfg(not(target_arch = "wasm32"))]
fn clock() -> Option<f64> {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    Some(START.get_or_init(std::time::Instant::now).elapsed().as_secs_f64())
}

#[cfg(target_arch = "wasm32")]
fn clock() -> Option<f64> {
    None
}
