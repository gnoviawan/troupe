//! The Rune web client's wasm entry. The page is served by the Troupe server it talks to, so
//! the server is the page's own origin. The device id and the web token live in `localStorage`
//! under the React client's keys (`termul.deviceId`, `termul.webAuthToken`), so both clients
//! sign in as the same device. A hash route that names no screen shows "Page not found".

#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
mod page {
    use troupe_app::{Launch, Route};
    use web_sys::Storage;

    const DEVICE_KEY: &str = "termul.deviceId";
    const TOKEN_KEY: &str = "termul.webAuthToken";

    fn random_hex(words: usize) -> String {
        (0..words)
            .map(|_| {
                format!(
                    "{:08x}",
                    (js_sys::Math::random() * f64::from(u32::MAX)) as u32
                )
            })
            .collect()
    }

    fn read(storage: Option<&Storage>, key: &str) -> Option<String> {
        storage?
            .get_item(key)
            .ok()
            .flatten()
            .filter(|v| !v.is_empty())
    }

    /// This browser profile's device id, minted and saved on first use.
    fn device_id(storage: Option<&Storage>) -> String {
        if let Some(id) = read(storage, DEVICE_KEY) {
            return id;
        }
        let id = format!("web-{}", random_hex(4));
        if let Some(storage) = storage
            && storage.set_item(DEVICE_KEY, &id).is_err()
        {
            log::warn!("[web] could not save the device id; this tab uses a new one");
        }
        id
    }

    pub fn launch() -> Launch {
        let Some(window) = web_sys::window() else {
            return Launch::default();
        };
        let storage = window.local_storage().ok().flatten();
        let location = window.location();
        let origin = location.origin().unwrap_or_default();
        let hash = location.hash().unwrap_or_default();
        let route = Route::from_hash(&hash);
        let seed = u64::from_str_radix(&random_hex(2), 16).unwrap_or(1);
        Launch {
            config: Some(client::Config {
                base_url: origin,
                token: read(storage.as_ref(), TOKEN_KEY),
                device_id: device_id(storage.as_ref()),
                client_instance_id: format!("rune-web-{}", random_hex(2)),
                seed,
                host_local: None,
            }),
            route,
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn start() {
    #[cfg(target_arch = "wasm32")]
    let launch = page::launch();
    #[cfg(not(target_arch = "wasm32"))]
    let launch = troupe_app::Launch::default();
    troupe_app::app(launch).run();
}
