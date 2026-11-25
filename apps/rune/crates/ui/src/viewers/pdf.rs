//! A PDF in the viewer (`PdfViewer.tsx`): its pages' sizes, and each page rasterised by hayro
//! at the scale it draws at, kept until the scale changes.

use std::cell::RefCell;
use std::collections::HashMap;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};
use rune::{Image, ImageCache};

/// `ZOOM_STEPS`.
const ZOOM_STEPS: [f32; 16] = [0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0];
pub(crate) const MIN_SCALE: f32 = ZOOM_STEPS[0];
pub(crate) const MAX_SCALE: f32 = ZOOM_STEPS[ZOOM_STEPS.len() - 1];
/// Space beside the page at fit width (`PAGE_GUTTER`).
pub(crate) const PAGE_GUTTER: f32 = 32.0;
const EPSILON: f32 = 0.001;

/// The next zoom step past `scale` in `direction` (`nextStep`).
pub(crate) fn next_step(scale: f32, direction: i32) -> f32 {
    let mut steps = ZOOM_STEPS.to_vec();
    if direction < 0 {
        steps.reverse();
    }
    let d = direction.signum() as f32;
    steps.iter().copied().find(|s| d * (s - scale) > EPSILON).unwrap_or(steps[steps.len() - 1])
}

pub struct PdfDoc {
    pdf: Pdf,
    /// Each page's size in points, as drawn (rotation applied).
    pub sizes: Vec<(f32, f32)>,
    /// The header's "page / pages", following the scroll.
    pub label: rune::LiveText,
    /// Rasterised pages by page index and device pixels per point, in thousandths.
    rendered: RefCell<HashMap<(usize, u32), Image>>,
}

impl PdfDoc {
    /// Parses `bytes`; a password or a damaged file is refused with `failureMessage`'s words.
    pub fn open(bytes: Vec<u8>) -> Result<Self, String> {
        let pdf = Pdf::new(bytes).map_err(|e| match e {
            LoadPdfError::Decryption(_) => "the PDF is password-protected".to_owned(),
            LoadPdfError::Invalid => "the PDF is damaged or not valid".to_owned(),
        })?;
        let sizes: Vec<(f32, f32)> = pdf.pages().iter().map(|p| p.render_dimensions()).collect();
        if sizes.is_empty() {
            return Err("the PDF has no pages".to_owned());
        }
        Ok(Self { label: rune::LiveText::new(format!("1 / {}", sizes.len())), pdf, sizes, rendered: RefCell::new(HashMap::new()) })
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

    /// Page `index` drawn at `scale` points per PDF point on a `device` scale screen. Pages
    /// render once per scale; rune decodes the pixels off the UI thread.
    pub fn page(&self, cache: &ImageCache, index: usize, scale: f32, device: f32) -> Option<Image> {
        let px = (scale * device).max(0.05);
        let key = (index, (px * 1000.0).round() as u32);
        if let Some(found) = self.rendered.borrow().get(&key) {
            return Some(found.clone());
        }
        let (w, h, rgba) = self.rasterise(index, px)?;
        let image = cache.rgba(w, h, rgba);
        let mut rendered = self.rendered.borrow_mut();
        rendered.retain(|(_, k), _| *k == key.1);
        rendered.insert(key, image.clone());
        Some(image)
    }

    /// Page `index` as opaque RGBA at `px` pixels per PDF point: its width, height and bytes.
    pub(crate) fn rasterise(&self, index: usize, px: f32) -> Option<(u32, u32, Vec<u8>)> {
        let page = self.pdf.pages().get(index)?;
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
