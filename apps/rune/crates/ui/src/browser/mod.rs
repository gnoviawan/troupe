//! Browser pane components for the Rune client: JPEG screencast decoding
//! (`decode_jpeg`, `decode_base64_jpeg`), Rune-to-CDP input mapping (`input`),
//! the interactive `El::pixels` viewport and custom overlay context menu (`pane`).

use base64::Engine;

pub mod input;
pub mod pane;

pub use pane::{
    CONTEXT_MENU, ContextAction, ContextMenuProps, KeyDisposition, PaneCallbacks, PaneSurfaceProps,
    build_context_menu, surface, surface_key,
};

/// One decoded frame: straight-alpha sRGB RGBA, top row first, `width * 4` bytes per row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decodes a raw JPEG byte slice into straight RGBA, or `None` if malformed or unsupported.
///
/// Chromium (libjpeg-turbo with optimal Huffman coding) emits baseline JPEGs with
/// three non-interleaved scans, one per component. `jpeg-decoder` decodes all scans
/// accurately on both native and wasm32 (`default-features = false`).
#[must_use]
pub fn decode_jpeg(bytes: &[u8]) -> Option<Frame> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if width == 0 || height == 0 {
        return None;
    }
    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            let mut rgba = Vec::with_capacity(pixels.len() / 3 * 4);
            for p in pixels.chunks_exact(3) {
                rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            rgba
        }
        jpeg_decoder::PixelFormat::L8 => {
            let mut rgba = Vec::with_capacity(pixels.len() * 4);
            for &v in &pixels {
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
            rgba
        }
        jpeg_decoder::PixelFormat::CMYK32 => {
            let mut rgba = Vec::with_capacity(pixels.len() / 4 * 4);
            for p in pixels.chunks_exact(4) {
                let k = 255 - u16::from(p[3]);
                let c = |v: u8| (u16::from(v) * k / 255) as u8;
                rgba.extend_from_slice(&[c(p[0]), c(p[1]), c(p[2]), 255]);
            }
            rgba
        }
        jpeg_decoder::PixelFormat::L16 => return None,
    };
    let expected = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
    if rgba.len() != expected {
        return None;
    }
    Some(Frame { width, height, rgba })
}

/// Decodes a base64-encoded JPEG screencast frame payload into straight RGBA.
#[must_use]
pub fn decode_base64_jpeg(encoded: &str) -> Option<Frame> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    decode_jpeg(&bytes)
}

#[cfg(test)]
mod tests;
