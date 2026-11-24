//! JavaScript's `encodeURIComponent`, `encodeURI` and `decodeURIComponent`, which the context
//! token and the file link use.

const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn encode(text: &str, keep: &[u8]) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || keep.contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
        }
    }
    out
}

pub fn encode_uri_component(text: &str) -> String {
    encode(text, b"-_.!~*'()")
}

pub fn encode_uri(text: &str) -> String {
    encode(text, b"-_.!~*'();/?:@&=+$,#")
}

/// None where JavaScript throws (a bad escape or bytes that are not UTF-8).
pub fn decode_uri_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes
                .get(i + 1..i + 3)
                .filter(|h| h.iter().all(u8::is_ascii_hexdigit))?;
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
