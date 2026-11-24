//! Pictures for images in replies. A `data:image/...;base64,` source decodes in process;
//! a file path or a web address shows the broken-image glyph and its alt text until the
//! client can read media over HTTP (a follow-up: `/fs/raw` needs the bearer header).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use rune::{Image, ImageCache, Waker};

pub struct ChatImages {
    cache: ImageCache,
    seen: RefCell<HashMap<String, Option<Image>>>,
}

impl ChatImages {
    pub fn new(wake: Waker) -> Rc<Self> {
        Rc::new(ChatImages { cache: ImageCache::new(wake), seen: RefCell::new(HashMap::new()) })
    }

    /// The picture for `src`, decoded once per source.
    pub fn get(&self, src: &str) -> Option<Image> {
        if let Some(found) = self.seen.borrow().get(src) {
            return found.clone();
        }
        let image = data_url_bytes(src).map(|bytes| self.cache.load(bytes));
        if image.is_none() && !src.starts_with("data:") {
            log::info!("[chat] no picture for an image source that is not a data URL");
        }
        self.seen.borrow_mut().insert(src.to_owned(), image.clone());
        image
    }
}

/// The bytes of a base64 `data:image/*` URL.
pub fn data_url_bytes(src: &str) -> Option<Vec<u8>> {
    let rest = src.strip_prefix("data:image/")?;
    let (meta, payload) = rest.split_once(',')?;
    if !meta.split(';').any(|p| p == "base64") {
        return None;
    }
    base64(payload)
}

fn base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let digits: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').map(value).collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &d)| n | u32::from(d) << (18 - 6 * i));
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
#[path = "chat_images_tests.rs"]
mod tests;
