//! Random ids and the wall clock, per target: the browser's `Math.random` and `Date.now`, or
//! the process's seeded hasher and `SystemTime` natively.

/// A UUID v4 in its 8-4-4-4-12 text form, as `crypto.randomUUID` prints it.
pub fn uuid() -> String {
    let mut b = [0u8; 16];
    for chunk in b.chunks_mut(4) {
        chunk.copy_from_slice(&random_u32().to_le_bytes());
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(target_arch = "wasm32")]
fn random_u32() -> u32 {
    (js_sys::Math::random() * f64::from(u32::MAX)) as u32
}

#[cfg(not(target_arch = "wasm32"))]
fn random_u32() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.write_u128(now_ms() as u128);
    h.finish() as u32
}

/// Milliseconds since the epoch.
#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

/// A chat row's age, as `formatRailRelativeTime` prints it: `now`, `5m`, `3h`, `2d`, `4mo`, `1y`.
pub fn age(now_ms: f64, then_ms: f64) -> String {
    let minutes = ((now_ms - then_ms) / 60_000.0).max(0.0).floor() as u64;
    match minutes {
        0 => "now".to_owned(),
        m if m < 60 => format!("{m}m"),
        m if m < 60 * 24 => format!("{}h", m / 60),
        m if m < 60 * 24 * 30 => format!("{}d", m / (60 * 24)),
        m if m < 60 * 24 * 360 => format!("{}mo", m / (60 * 24 * 30)),
        m => format!("{}y", (m / (60 * 24 * 365)).max(1)),
    }
}

/// A time of day as the phone's Recents rows print it (`4:30 AM`), in the page's locale
/// time zone on the web and in UTC natively.
pub fn clock_time(ms: f64) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
        let options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&options, &"hour".into(), &"numeric".into());
        let _ = js_sys::Reflect::set(&options, &"minute".into(), &"2-digit".into());
        String::from(date.to_locale_time_string_with_options("en-US", &options))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let minutes = (ms / 60_000.0).floor() as i64;
        let (h, m) = ((minutes / 60).rem_euclid(24), minutes.rem_euclid(60));
        let (h12, half) = match h {
            0 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        format!("{h12}:{m:02} {half}")
    }
}
