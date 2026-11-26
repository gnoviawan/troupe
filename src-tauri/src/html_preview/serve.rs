//! The one serving function behind `/preview/<token>/<rest>` on every
//! listener.

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path};

use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};

use super::{log_file_name, Registry, MAX_FILE_BYTES};

/// Scripts, forms, dialogs and popups run, but never with the app's origin:
/// no `allow-same-origin`, so the page cannot read the web client's storage
/// even when opened top-level.
pub const PREVIEW_CSP: &str = "sandbox allow-scripts allow-forms allow-modals allow-popups";

const HTML: &str = "text/html; charset=utf-8";

/// The file types a previewed page may load, and the type each is served as.
const PREVIEW_TYPES: &[(&str, &str)] = &[
    ("html", HTML),
    ("htm", HTML),
    ("css", "text/css; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("json", "application/json"),
    ("map", "application/json"),
    ("txt", "text/plain; charset=utf-8"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("ico", "image/x-icon"),
    ("bmp", "image/bmp"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("ttf", "font/ttf"),
    ("otf", "font/otf"),
    ("mp4", "video/mp4"),
    ("webm", "video/webm"),
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
    ("ogg", "audio/ogg"),
];

pub fn preview_type(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    PREVIEW_TYPES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(ext))
        .map(|&(_, mime)| mime)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    NotFound,
    OutsideRoot,
    Unsupported,
    TooLarge,
    ReadFailed,
}

impl Refusal {
    const fn reason(self) -> &'static str {
        match self {
            Self::NotFound => "NOT_FOUND",
            Self::OutsideRoot => "PATH_TRAVERSAL",
            Self::Unsupported => "UNSUPPORTED_FILE",
            Self::TooLarge => "FILE_TOO_LARGE",
            Self::ReadFailed => "READ_ERROR",
        }
    }

    const fn status(self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::OutsideRoot => StatusCode::FORBIDDEN,
            Self::Unsupported => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::ReadFailed => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn io(err: &io::Error) -> Self {
        if err.kind() == io::ErrorKind::NotFound {
            Self::NotFound
        } else {
            Self::ReadFailed
        }
    }
}

impl Registry {
    /// Answer one `GET`/`HEAD` of `/preview/<token>/<rest>`. `rest` is the
    /// percent-decoded path under the session; empty redirects to the entry.
    pub fn serve(&self, token: &str, rest: &str, method: &Method) -> Response {
        if method != Method::GET && method != Method::HEAD {
            return secured(StatusCode::METHOD_NOT_ALLOWED.into_response());
        }
        let Some((root, entry, buffer)) = self.lookup(token) else {
            log::warn!("[html-preview] serve refused: unknown or closed session");
            return secured(
                (StatusCode::NOT_FOUND, "the preview session is closed").into_response(),
            );
        };
        if rest.is_empty() {
            return secured(redirect_to(&entry));
        }
        if let Some(html) = buffer.filter(|_| rest == entry) {
            return secured(body(HTML, html.into_bytes()));
        }
        match read_under(&root, rest) {
            Ok((mime, bytes)) => secured(body(mime, bytes)),
            Err(refusal) => {
                log::warn!(
                    "[html-preview] serve refused file={} reason={}",
                    log_file_name(rest),
                    refusal.reason()
                );
                secured((refusal.status(), refusal.reason()).into_response())
            }
        }
    }
}

/// `rest` read from under `root`, refused when it names anything outside it
/// (a `..` or absolute component, or a symlink that leaves), a type off the
/// allowlist, or a file over [`MAX_FILE_BYTES`].
fn read_under(root: &Path, rest: &str) -> Result<(&'static str, Vec<u8>), Refusal> {
    let relative = Path::new(rest);
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(Refusal::OutsideRoot);
    }
    let canonical = root
        .join(relative)
        .canonicalize()
        .map_err(|err| Refusal::io(&err))?;
    if !canonical.starts_with(root) {
        return Err(Refusal::OutsideRoot);
    }
    let mime = preview_type(&canonical).ok_or(Refusal::Unsupported)?;
    let metadata = fs::metadata(&canonical).map_err(|err| Refusal::io(&err))?;
    if !metadata.is_file() {
        return Err(Refusal::NotFound);
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(Refusal::TooLarge);
    }
    let file = fs::File::open(&canonical).map_err(|err| Refusal::io(&err))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| Refusal::io(&err))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Refusal::TooLarge);
    }
    Ok((mime, bytes))
}

fn body(mime: &'static str, bytes: Vec<u8>) -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static(mime))],
        bytes,
    )
        .into_response()
}

/// A redirect relative to `/preview/<token>/`, so the token never has to be
/// echoed into a header.
fn redirect_to(entry: &str) -> Response {
    let mut response = StatusCode::TEMPORARY_REDIRECT.into_response();
    if let Ok(location) = HeaderValue::from_str(&format!("./{}", encode_segment(entry))) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
}

/// Percent-encode everything but RFC 3986 unreserved characters.
fn encode_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The headers every preview response carries. CORS is open because the
/// sandboxed page has an opaque origin, and module scripts and `fetch` from it
/// are CORS requests.
fn secured(mut response: Response) -> Response {
    let headers: &mut HeaderMap = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(PREVIEW_CSP),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}
