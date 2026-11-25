use model::editor::Unavailable;
use model::editor::media::MediaKind;

use super::pdf::next_step;
use super::video::format_duration;
use super::{Content, Effect, Load, PdfDoc, Viewers, Zoom};

const PDF: &[u8] = include_bytes!("fixtures/ten-pages.pdf");

#[test]
fn a_viewer_asks_for_the_bytes_once_and_a_refusal_shows_why() {
    let mut v = Viewers::default();
    v.open("/p/a.png", MediaKind::Image);
    v.open("/p/a.png", MediaKind::Image);
    assert_eq!(v.effects, [Effect::Read { path: "/p/a.png".into() }]);
    v.loaded("/p/a.png", Err(("NOT_FOUND".into(), "gone".into())));
    assert!(matches!(v.files["/p/a.png"].load, Load::Unavailable(Unavailable::Missing)));
}

#[test]
fn a_ten_page_pdf_opens_with_its_page_sizes_and_renders_a_page() {
    let mut v = Viewers::default();
    v.open("/p/doc.pdf", MediaKind::Pdf);
    v.loaded("/p/doc.pdf", Ok(PDF.to_vec()));
    let Load::Ready { size, content: Content::Pdf(doc) } = &v.files["/p/doc.pdf"].load else {
        panic!("the PDF opens");
    };
    assert_eq!(*size, PDF.len() as u64);
    assert_eq!(doc.pages(), 10);
    assert_eq!(doc.sizes[4], (612.0, 792.0));
    assert_eq!(doc.label.get().as_ref(), "1 / 10");
    assert!((doc.fit_scale(644.0) - 1.0).abs() < 1e-6, "fit width leaves the 32 pt gutter");
}

#[test]
fn a_damaged_pdf_is_refused_with_a_reason() {
    assert_eq!(PdfDoc::open(b"%PDF-1.4 not really".to_vec()).err().as_deref(), Some("the PDF is damaged or not valid"));
    let mut v = Viewers::default();
    v.open("/p/bad.pdf", MediaKind::Pdf);
    v.loaded("/p/bad.pdf", Ok(b"nope".to_vec()));
    assert!(matches!(&v.files["/p/bad.pdf"].load, Load::Unavailable(Unavailable::Unreadable(m)) if m.contains("damaged")));
}

#[test]
fn pdf_page_five_rasterises_at_the_scale_it_draws() {
    let doc = PdfDoc::open(PDF.to_vec()).expect("fixture parses");
    let cache = rune::ImageCache::new(std::sync::Arc::new(|| {}));
    let image = doc.page(&cache, 4, 0.5, 2.0).expect("page 5 renders");
    assert_eq!(image.wait().ok(), Some([612, 792]), "0.5 pt per point at 2x is the page's own size in pixels");
    let (w, h, rgba) = doc.rasterise(4, 1.0).expect("page 5 rasterises");
    assert_eq!((w, h), (612, 792));
    let at = |x: usize, y: usize| &rgba[(y * 612 + x) * 4..(y * 612 + x) * 4 + 3];
    let (inside, outside) = (at(100, 100), at(20, 20));
    assert!(inside[2] > inside[0] + 60, "the page's blue band is drawn: {inside:?}");
    assert!(outside.iter().all(|c| *c > 240), "the page is white around it: {outside:?}");
    assert!(doc.page(&cache, 10, 1.0, 1.0).is_none(), "there is no page 11");
}

#[test]
fn zoom_steps_follow_the_web_viewers() {
    assert!((next_step(1.0, 1) - 1.1).abs() < 1e-6);
    assert!((next_step(1.0, -1) - 0.9).abs() < 1e-6);
    assert!((next_step(5.0, 1) - 5.0).abs() < 1e-6);
    assert!((next_step(0.3, -1) - 0.25).abs() < 1e-6);
    let mut v = Viewers::default();
    v.open("/p/a.png", MediaKind::Image);
    v.zoom_by("/p/a.png", 1, 1.0);
    assert_eq!(v.files["/p/a.png"].zoom, Zoom::Scale(1.25));
    v.zoom_by("/p/a.png", -1, 0.05);
    assert_eq!(v.files["/p/a.png"].zoom, Zoom::Scale(0.05), "the image zoom stops at 5%");
}

#[test]
fn durations_read_as_the_web_footer_writes_them() {
    assert_eq!(format_duration(3.4), "0:03");
    assert_eq!(format_duration(65.0), "1:05");
    assert_eq!(format_duration(3_725.0), "1:02:05");
}
