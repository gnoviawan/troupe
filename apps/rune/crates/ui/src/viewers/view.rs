//! Drawing a media tab: `ImageViewer`, `PdfViewer` and `VideoViewer` with their footer or
//! header (`viewer-chrome.tsx`), or `UnsupportedFileView` when the file cannot show.

use rune::{El, Image, ScrollSpec, col, el, row, text};
use theme::Tokens;
use theme::icons as i;

use super::pdf::{MAX_SCALE, MIN_SCALE, PdfDoc};
use super::video::{Player, format_duration};
use super::{Content, Load, Viewer, Viewers, Zoom};
use crate::type_::sans;
use crate::{IconActionButton, IconActionSize};

type Get<S> = fn(&S) -> &Viewers;
type Lens<S> = fn(&mut S) -> &mut Viewers;

/// Device pixels per point PDF pages rasterise at, so text stays sharp on a 2x screen.
const PDF_DEVICE_SCALE: f32 = 2.0;
/// Space between PDF pages (`gap-3`) and around them (`p-4`).
const PAGE_GAP: f32 = 12.0;

/// One media tab for `path`, or None while the app holds no viewer for it.
pub fn view<S: 'static>(s: &S, get: Get<S>, lens: Lens<S>, t: &'static Tokens, path: &str) -> Option<El> {
    let viewers = get(s);
    let viewer = viewers.files.get(path)?;
    let c = &t.colors;
    let body = col().key(format!("viewer-{path}")).w_full().grow(1.0).min_h(0.0).fill(c.background);
    Some(match &viewer.load {
        Load::Loading => body.child(col().grow(1.0).w_full().center().child(text("Loading...").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground))),
        Load::Unavailable(why) => body.child(crate::editor::unavailable::<S>(t, path, why)),
        Load::Ready { size, content: Content::Image(image) } => body.child(image_view(t, lens, path, viewer, image, *size)),
        Load::Ready { content: Content::Pdf(doc), .. } => body.child(pdf_view(get, lens, t, path, viewer, doc)),
        Load::Ready { size, content: Content::Video(player) } => body.child(video_view(t, lens, path, player, *size)),
    })
}

fn icon_button<S: 'static>(t: &Tokens, glyph: &'static rune::Icon, label: &str, disabled: bool, press: impl Fn(&mut S) + 'static) -> El {
    IconActionButton::new(glyph, label).size(IconActionSize::Sm).disabled(disabled).build(t).on_press(press)
}

fn text_button<S: 'static>(t: &'static Tokens, label: &str, pressed: bool, press: impl Fn(&mut S) + 'static) -> El {
    crate::Button::new(label)
        .variant(crate::ButtonVariant::Ghost)
        .size(crate::ButtonSize::Xs)
        .build(t)
        .checked(pressed)
        .on_press(press)
}

/// `ViewerFooter`: the facts on the left, the controls on the right.
fn footer(t: &Tokens, facts: &[Option<String>], controls: Vec<El>) -> El {
    let c = &t.colors;
    let facts: Vec<String> = facts.iter().flatten().cloned().collect();
    row()
        .key("viewer-footer")
        .w_full()
        .h(32.0)
        .px(12.0)
        .items_center()
        .justify_between()
        .gap(12.0)
        .border_sides([1.0, 0.0, 0.0, 0.0], c.border)
        .fill(c.background)
        .child(text(facts.join(" · ")).key("viewer-facts").font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).min_w(0.0))
        .child(row().gap(4.0).items_center().children(controls))
}

/// The scale the image shows at: its zoom, or at fit the largest that fits the area without
/// growing past its own size (`max-h-full max-w-full`).
fn shown_scale(viewer: &Viewer, natural: [u32; 2]) -> f32 {
    match viewer.zoom {
        Zoom::Scale(s) => s,
        Zoom::Fit => {
            let (aw, ah) = ((viewer.width - 32.0).max(1.0), (viewer.area_h - 32.0).max(1.0));
            (aw / natural[0].max(1) as f32).min(ah / natural[1].max(1) as f32).min(1.0)
        }
    }
}

fn image_view<S: 'static>(t: &'static Tokens, lens: Lens<S>, path: &str, viewer: &Viewer, image: &Image, size: u64) -> El {
    let c = &t.colors;
    let natural = image.size();
    let scale = natural.map_or(1.0, |n| shown_scale(viewer, n));
    let mut picture = el().key("viewer-image");
    if let Some([w, h]) = natural {
        picture = picture.size(w as f32 * scale, h as f32 * scale).pixels(image.pixels());
    }
    let measured = path.to_owned();
    let area = col()
        .key("viewer-image-area")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .on_layout(move |s: &mut S, [_, _, w, h]| {
            if let Some(v) = lens(s).files.get_mut(&measured) {
                v.width = w;
                v.area_h = h;
            }
        })
        .child(col().w_full().min_h(viewer.area_h).padding(16.0).center().child(picture.fill(c.muted)));
    let (out, inn, one, fit) = (path.to_owned(), path.to_owned(), path.to_owned(), path.to_owned());
    let controls = vec![
        icon_button::<S>(t, &i::ZOOM_OUT, "Zoom out", false, move |s| lens(s).zoom_by(&out, -1, scale)),
        icon_button::<S>(t, &i::ZOOM_IN, "Zoom in", false, move |s| lens(s).zoom_by(&inn, 1, scale)),
        text_button::<S>(t, "1:1", viewer.zoom == Zoom::Scale(1.0), move |s| lens(s).set_zoom(&one, Zoom::Scale(1.0))),
        text_button::<S>(t, "Fit", viewer.zoom == Zoom::Fit, move |s| lens(s).set_zoom(&fit, Zoom::Fit)),
    ];
    let facts = [
        natural.map(|[w, h]| format!("{w} × {h}")),
        Some(model::editor::format_size(size)),
        match viewer.zoom {
            Zoom::Fit => None,
            Zoom::Scale(z) => Some(format!("{}%", (z * 100.0).round())),
        },
    ];
    col().w_full().grow(1.0).min_h(0.0).child(area).child(footer(t, &facts, controls))
}

fn pdf_view<S: 'static>(get: Get<S>, lens: Lens<S>, t: &'static Tokens, path: &str, viewer: &Viewer, doc: &PdfDoc) -> El {
    let c = &t.colors;
    let fit = doc.fit_scale(viewer.width);
    let scale = match viewer.zoom {
        Zoom::Fit => fit,
        Zoom::Scale(z) => z,
    };
    let (out, inn, fitted) = (path.to_owned(), path.to_owned(), path.to_owned());
    let header = row()
        .key("pdf-header")
        .w_full()
        .h(40.0)
        .px(12.0)
        .items_center()
        .justify_between()
        .border_sides([0.0, 0.0, 1.0, 0.0], c.border)
        .child(
            text(doc.label.get().to_string())
                .live_text(&doc.label)
                .key("pdf-page-count")
                .font(sans(12.0, 400.0, 16.0))
                .color(c.muted_foreground),
        )
        .child(
            row()
                .gap(4.0)
                .items_center()
                .child(icon_button::<S>(t, &i::ZOOM_OUT, "Zoom out", scale <= MIN_SCALE + 0.001, move |s| lens(s).zoom_by(&out, -1, scale)))
                .child(text(format!("{}%", (scale * 100.0).round())).font(sans(12.0, 400.0, 16.0)).color(c.muted_foreground).min_w(40.0))
                .child(icon_button::<S>(t, &i::ZOOM_IN, "Zoom in", scale >= MAX_SCALE - 0.001, move |s| lens(s).zoom_by(&inn, 1, scale)))
                .child(el().w(1.0).h(16.0).mx(2.0).fill(c.border))
                .child(icon_button::<S>(t, &i::ARROW_RIGHT_LEFT, "Fit width", false, move |s| lens(s).set_zoom(&fitted, Zoom::Fit)).checked(viewer.zoom == Zoom::Fit)),
        );
    let (w0, h0) = doc.sizes[0];
    let row_h = (h0 * scale).round() + PAGE_GAP;
    let (rows_path, measured) = (path.to_owned(), path.to_owned());
    let (label, count) = (doc.label.clone(), doc.pages());
    let pages = col()
        .key(format!("pdf-pages-{path}"))
        .w_full()
        .py(16.0)
        .virtual_rows_version((scale * 1000.0) as u32)
        .virtual_rows(
            doc.pages(),
            row_h,
            |_s: &S, i| i.to_string().into(),
            move |s: &S, i| pdf_page(get(s), t, &rows_path, i, scale),
        );
    let area = col()
        .key(format!("pdf-scroll-{path}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(c.muted.alpha(0.4))
        .scroll(ScrollSpec { stick_bottom: false })
        .on_layout(move |s: &mut S, [_, _, w, h]| {
            if let Some(v) = lens(s).files.get_mut(&measured) {
                v.width = w;
                v.area_h = h;
            }
        })
        .on_scroll_offset(move |at| {
            let page = if at.max[1] > 0.0 && at.offset[1] >= at.max[1] - 1.0 { count } else { ((at.offset[1] - 16.0) / row_h).floor().max(0.0) as usize + 1 };
            label.set(format!("{} / {count}", page.min(count)));
        })
        .child(pages.min_w((w0 * scale).round() + 32.0));
    col().w_full().grow(1.0).min_h(0.0).child(header).child(area)
}

/// One page (`PdfPage`): white, shadowed, at its size at `scale`, rasterised when first shown.
fn pdf_page(viewers: &Viewers, t: &Tokens, path: &str, index: usize, scale: f32) -> El {
    let c = &t.colors;
    let Some(Viewer { load: Load::Ready { content: Content::Pdf(doc), .. }, .. }) = viewers.files.get(path) else {
        return el();
    };
    let (w, h) = doc.sizes[index];
    let mut page = el().key(format!("pdf-page-{}", index + 1)).size((w * scale).round(), (h * scale).round()).fill(rune::Color::WHITE).border(1.0, c.border);
    if let Some(image) = viewers.images.as_ref().and_then(|cache| doc.page(cache, index, scale, PDF_DEVICE_SCALE)) {
        page = page.pixels(image.pixels());
    }
    row().w_full().h((h * scale).round() + PAGE_GAP).justify_center().items_start().child(page)
}

fn video_view<S: 'static>(t: &'static Tokens, lens: Lens<S>, path: &str, player: &Player, size: u64) -> El {
    let meta = player.meta();
    let toggled = path.to_owned();
    let playing = player.playing();
    let picture = player.show(el().key("viewer-video").grow(1.0).w_full().min_h(0.0));
    let play = crate::Button::new(if playing { "Pause" } else { "Play" })
        .variant(crate::ButtonVariant::Ghost)
        .size(crate::ButtonSize::Xs)
        .build(t)
        .key("viewer-video-play")
        .on_press(move |s: &mut S| {
            if let Some(Viewer { load: Load::Ready { content: Content::Video(p), .. }, .. }) = lens(s).files.get(&toggled) {
                p.toggle();
            }
        });
    let facts = [
        (meta.width > 0).then(|| format!("{} × {}", meta.width, meta.height)),
        meta.duration.map(|d| format!("{} / {}", format_duration(player.position()), format_duration(d))),
        Some(model::editor::format_size(size)),
    ];
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .child(col().w_full().grow(1.0).min_h(0.0).padding(16.0).center().child(picture))
        .child(footer(t, &facts, vec![play]))
}
