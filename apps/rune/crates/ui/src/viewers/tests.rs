use std::time::{Duration, Instant};

use model::editor::Unavailable;
use model::editor::media::MediaKind;

use super::pdf::{MAX_SIDE, next_step, page_px};
use super::video::format_duration;
use super::{Content, Effect, Load, PdfDoc, Viewers, Zoom};

const PDF: &[u8] = include_bytes!("fixtures/ten-pages.pdf");

/// Pumps until no viewer is loading: PDFs and videos open on a thread of their own.
fn settle(v: &mut Viewers) {
    let until = Instant::now() + Duration::from_secs(60);
    while v.files.values().any(|f| matches!(f.load, Load::Loading)) {
        assert!(Instant::now() < until, "the open lands");
        v.pump();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A PDF of blank pages of these sizes in points.
fn blank_pdf(sizes: &[(u32, u32)]) -> Vec<u8> {
    let kids: Vec<String> = (0..sizes.len()).map(|i| format!("{} 0 R", i + 3)).collect();
    let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_owned(), format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), sizes.len())];
    objects.extend(sizes.iter().map(|(w, h)| format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] >>")));
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for offset in offsets {
        out.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).bytes());
    out
}

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
    settle(&mut v);
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
    settle(&mut v);
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

fn draw(v: &Viewers, _cx: &mut rune::Cx<Viewers>) -> rune::El {
    let t = theme::tokens("termul", true);
    let path = v.files.keys().next().cloned().unwrap_or_default();
    super::view(v, |v| v, |v| v, t, &path).unwrap_or_else(rune::el)
}

#[test]
fn the_pdf_view_draws_its_pages_in_a_window() {
    let mut v = Viewers { images: Some(rune::ImageCache::new(std::sync::Arc::new(|| {}))), ..Viewers::default() };
    v.open("/p/doc.pdf", MediaKind::Pdf);
    v.loaded("/p/doc.pdf", Ok(PDF.to_vec()));
    settle(&mut v);
    let mut rt = rune::Runtime::new(v, draw, (1000.0, 800.0), rune::Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    assert!(rt.find("pdf-page-1").is_some(), "page 1 shows");
    assert!(rt.find("pdf-page-10").is_none(), "page 10 is outside the window");
}

#[test]
fn a_pdf_opens_and_rasterises_off_the_ui_thread() {
    let mut v = Viewers { images: Some(rune::ImageCache::new(std::sync::Arc::new(|| {}))), ..Viewers::default() };
    v.open("/p/doc.pdf", MediaKind::Pdf);
    v.loaded("/p/doc.pdf", Ok(PDF.to_vec()));
    assert!(matches!(v.files["/p/doc.pdf"].load, Load::Loading), "the parse runs on its own thread");
    settle(&mut v);
    let until = Instant::now() + Duration::from_secs(60);
    let image = loop {
        let Load::Ready { content: Content::Pdf(doc), .. } = &v.files["/p/doc.pdf"].load else { panic!("the PDF opens") };
        if let Some(image) = doc.page(v.images.as_ref().unwrap(), 4, 0.5, 2.0) {
            break image;
        }
        assert!(Instant::now() < until, "page 5 lands");
        std::thread::sleep(Duration::from_millis(2));
        v.pump();
    };
    assert_eq!(image.wait().ok(), Some([612, 792]));
}

#[test]
fn a_reopened_file_takes_only_its_latest_read() {
    let mut v = Viewers::default();
    v.open("/p/doc.pdf", MediaKind::Pdf);
    v.loaded("/p/doc.pdf", Ok(b"stale".to_vec()));
    v.close("/p/doc.pdf");
    v.open("/p/doc.pdf", MediaKind::Pdf);
    v.loaded("/p/doc.pdf", Ok(PDF.to_vec()));
    settle(&mut v);
    std::thread::sleep(Duration::from_millis(50));
    v.pump();
    assert!(matches!(&v.files["/p/doc.pdf"].load, Load::Ready { content: Content::Pdf(_), .. }), "the damaged first read does not land on the second viewer");
}

#[test]
fn only_the_pages_drawn_last_stay_rasterised() {
    // At 0.25 pt per point on a 2x screen a 612 x 792 page is 306 x 396 pixels.
    let page = 306 * 396 * 4;
    let doc = PdfDoc::open(PDF.to_vec()).expect("fixture parses").with_budget(5 * page);
    let cache = rune::ImageCache::new(std::sync::Arc::new(|| {}));
    for index in 0..10 {
        assert!(doc.page(&cache, index, 0.25, 2.0).is_some(), "page {} rasterises", index + 1);
    }
    assert_eq!(doc.resident(), [5, 6, 7, 8, 9], "scrolling through ten pages keeps the last five");
    assert!(doc.page(&cache, 2, 0.25, 2.0).is_some());
    assert_eq!(doc.resident(), [2, 6, 7, 8, 9], "scrolling back to page 3 drops page 6, drawn longest ago");
}

#[test]
fn a_huge_page_rasterises_no_larger_than_the_side_cap() {
    let doc = PdfDoc::open(blank_pdf(&[(612, 792), (30_000, 15_000)])).expect("the generated PDF parses");
    let px = doc.fit_scale(1000.0) * 2.0;
    assert!(30_000.0 * px > 65_535.0, "uncapped, page 2 would be {} px wide", 30_000.0 * px);
    assert!((page_px((612.0, 792.0), px) - px).abs() < 1e-6, "page 1 keeps its scale");
    let (w, h, rgba) = doc.rasterise(1, px).expect("page 2 rasterises");
    assert_eq!((w, h), (MAX_SIDE as u32, MAX_SIDE as u32 / 2));
    assert_eq!(rgba.len(), 8192 * 4096 * 4);
}

#[cfg(not(target_arch = "wasm32"))]
mod video {
    use std::time::{Duration, Instant};

    use model::editor::media::MediaKind;

    use super::super::video::{Player, stage};
    use super::super::{Content, Load, Viewers};
    use super::settle;

    /// The clips staged in the temporary folder with this extension; each test uses its own.
    fn staged(ext: &str) -> std::collections::BTreeSet<std::path::PathBuf> {
        std::fs::read_dir(std::env::temp_dir())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("troupe-video-") && n.ends_with(ext)))
            .collect()
    }

    #[test]
    fn a_clip_that_does_not_open_leaves_no_staged_file() {
        let before = staged(".mp4");
        assert!(Player::open("/p/clip.mp4", b"not a video".to_vec(), None).is_err());
        assert_eq!(staged(".mp4"), before);
    }

    #[test]
    fn staged_clips_are_private_and_never_shared() {
        let (a, b) = (stage("/p/clip.mkv", b"same bytes").unwrap(), stage("/p/other/clip.mkv", b"same bytes").unwrap());
        assert_ne!(a.to_path_buf(), b.to_path_buf(), "one file per player, even for the same bytes");
        assert_eq!(std::fs::read(&a).unwrap(), b"same bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&a).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let odd = stage("/p/dir.v1/clip", b"x").unwrap();
        assert!(odd.to_string_lossy().ends_with(".bin"), "an extension with a slash in it is not used: {}", odd.display());
        let path = a.to_path_buf();
        drop(a);
        assert!(!path.exists(), "dropping the player's file removes it");
    }

    /// Opens a one-second clip off the UI thread, plays it to the end, and closes it. Needs
    /// ffmpeg on the PATH, as the desktop player does; without it the test says so and stops.
    #[test]
    fn a_clip_opens_off_the_ui_thread_plays_to_its_end_and_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "testsrc=size=64x48:rate=10", "-t", "1", "-pix_fmt", "yuv420p"])
            .arg(&clip)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            eprintln!("ffmpeg is not on the PATH; the clip test did not run");
            return;
        }
        let before = staged(".mov");
        let mut v = Viewers::default();
        v.open("/p/clip.mov", MediaKind::Video);
        v.loaded("/p/clip.mov", Ok(std::fs::read(&clip).unwrap()));
        assert!(matches!(v.files["/p/clip.mov"].load, Load::Loading), "ffprobe and the first frame run on their own thread");
        settle(&mut v);
        let Load::Ready { content: Content::Video(player), .. } = &v.files["/p/clip.mov"].load else { panic!("the clip opens") };
        assert_eq!((player.meta().width, player.meta().height), (64, 48));
        assert_eq!(staged(".mov").len(), before.len() + 1, "the clip is staged while it plays");
        player.toggle();
        let until = Instant::now() + Duration::from_secs(30);
        while player.playing() {
            assert!(Instant::now() < until, "the clip ends");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(player.position() > 0.8, "it played to the end: {}", player.position());
        v.close("/p/clip.mov");
        assert_eq!(staged(".mov"), before, "closing removes the staged clip");
    }
}
