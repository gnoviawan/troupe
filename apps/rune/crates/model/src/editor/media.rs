//! Which files open in a viewer instead of the text editor (`lib/media-kind.ts` and
//! `editorFileKindForPath`), and what a viewer read of `/fs/raw` came to.

use super::Unavailable;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    Pdf,
}

impl MediaKind {
    /// The largest file the host serves for this kind (`MEDIA_MAX_BYTES`).
    #[must_use]
    pub fn max_bytes(self) -> u64 {
        const MIB: u64 = 1024 * 1024;
        match self {
            Self::Image => 25 * MIB,
            Self::Video => 100 * MIB,
            Self::Pdf => 50 * MIB,
        }
    }
}

/// `MEDIA_TYPES`: each extension's kind and MIME type.
const TYPES: [(&str, MediaKind, &str); 15] = [
    ("png", MediaKind::Image, "image/png"),
    ("jpg", MediaKind::Image, "image/jpeg"),
    ("jpeg", MediaKind::Image, "image/jpeg"),
    ("gif", MediaKind::Image, "image/gif"),
    ("webp", MediaKind::Image, "image/webp"),
    ("bmp", MediaKind::Image, "image/bmp"),
    ("avif", MediaKind::Image, "image/avif"),
    ("ico", MediaKind::Image, "image/x-icon"),
    ("svg", MediaKind::Image, "image/svg+xml"),
    ("mp4", MediaKind::Video, "video/mp4"),
    ("m4v", MediaKind::Video, "video/mp4"),
    ("webm", MediaKind::Video, "video/webm"),
    ("mov", MediaKind::Video, "video/quicktime"),
    ("ogv", MediaKind::Video, "video/ogg"),
    ("pdf", MediaKind::Pdf, "application/pdf"),
];

fn extension(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let dot = name.rfind('.').filter(|d| *d > 0)?;
    Some(name[dot + 1..].to_ascii_lowercase())
}

fn media_type(path: &str) -> Option<(MediaKind, &'static str)> {
    let ext = extension(path)?;
    TYPES.iter().find(|(e, _, _)| *e == ext).map(|(_, kind, mime)| (*kind, *mime))
}

/// The viewer `path` opens in, or None for the text editor. An SVG opens as text, as
/// `editorFileKindForPath` has it.
#[must_use]
pub fn viewer_for_path(path: &str) -> Option<MediaKind> {
    if extension(path).as_deref() == Some("svg") {
        return None;
    }
    media_type(path).map(|(kind, _)| kind)
}

#[must_use]
pub fn mime_for_path(path: &str) -> Option<&'static str> {
    media_type(path).map(|(_, mime)| mime)
}

/// The host's refusal of a raw read (`previewUnavailableFromRead`).
#[must_use]
pub fn unavailable(code: &str, message: &str) -> Unavailable {
    match code {
        "FILE_TOO_LARGE" => Unavailable::from_refusal(code, message),
        "NOT_FOUND" => Unavailable::Missing,
        _ => Unavailable::Unreadable(message.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::{MediaKind, mime_for_path, unavailable, viewer_for_path};
    use crate::editor::Unavailable;

    #[test]
    fn media_files_open_in_their_viewer_and_svg_and_text_open_as_text() {
        assert_eq!(viewer_for_path("/p/shot.PNG"), Some(MediaKind::Image));
        assert_eq!(viewer_for_path("/p/clip.mp4"), Some(MediaKind::Video));
        assert_eq!(viewer_for_path("C:\\p\\doc.pdf"), Some(MediaKind::Pdf));
        assert_eq!(viewer_for_path("/p/logo.svg"), None);
        assert_eq!(viewer_for_path("/p/.png"), None);
        assert_eq!(viewer_for_path("/p/main.rs"), None);
        assert_eq!(mime_for_path("/p/a.mov"), Some("video/quicktime"));
    }

    #[test]
    fn raw_read_refusals_name_why_the_file_shows_no_preview() {
        assert_eq!(unavailable("NOT_FOUND", "gone"), Unavailable::Missing);
        assert_eq!(unavailable("UNSUPPORTED_MEDIA", "no"), Unavailable::Unreadable("no".into()));
        assert_eq!(MediaKind::Pdf.max_bytes(), 50 * 1024 * 1024);
    }
}
