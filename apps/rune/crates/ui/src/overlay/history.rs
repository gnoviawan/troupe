//! The web build's back button: one history entry per open overlay, so system back on a phone
//! (and the browser's back button) closes the top overlay instead of leaving the page. Native
//! builds have no history; every call is a no-op there and [`take_back_presses`] stays 0.
//!
//! The app wires three calls: [`install`] from `AppSpec::setup`, [`sync`] with
//! [`Overlays::depth`](super::Overlays::depth) from its view, and [`take_back_presses`] from
//! `AppSpec::poll`, popping one overlay per press.

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use rune::Waker;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen::closure::Closure;
    use web_sys::PopStateEvent;

    use crate::overlay::stack::{HistorySentinels, HistoryStep};

    #[derive(Default)]
    struct Web {
        sentinels: HistorySentinels,
        backs: usize,
        wake: Option<Waker>,
    }

    thread_local! {
        static WEB: RefCell<Web> = RefCell::default();
    }

    const PREFIX: &str = "troupe-overlay:";

    /// The depth a history entry's state records: 0 for an entry that is not our sentinel.
    fn depth_of(state: &JsValue) -> usize {
        state.as_string().and_then(|s| s.strip_prefix(PREFIX)?.parse().ok()).unwrap_or(0)
    }

    pub fn install(wake: Waker) {
        let first = WEB.with_borrow_mut(|w| w.wake.replace(wake).is_none());
        if !first {
            return;
        }
        let Some(window) = web_sys::window() else { return };
        let on_pop = Closure::<dyn Fn(PopStateEvent)>::new(|event: PopStateEvent| {
            let landed = depth_of(&event.state());
            let wake = WEB.with_borrow_mut(|w| {
                let closes = w.sentinels.popstate(landed);
                if closes == 0 {
                    return None;
                }
                w.backs += closes;
                w.wake.clone()
            });
            if let Some(wake) = wake {
                wake();
            }
        });
        if let Err(e) = window.add_event_listener_with_callback("popstate", on_pop.as_ref().unchecked_ref()) {
            web_sys::console::warn_1(&format!("troupe overlay: no popstate listener: {e:?}").into());
        }
        on_pop.forget();
    }

    pub fn sync(depth: usize) {
        let Some(step) = WEB.with_borrow_mut(|w| w.wake.is_some().then(|| w.sentinels.reconcile(depth)).flatten()) else {
            return;
        };
        let Some(history) = web_sys::window().and_then(|w| w.history().ok()) else { return };
        let (done, reached) = match step.clone() {
            HistoryStep::Push(depths) => {
                let mut reached = depths.start - 1;
                let mut done = Ok(());
                for d in depths {
                    done = history.push_state(&JsValue::from_str(&format!("{PREFIX}{d}")), "");
                    if done.is_err() {
                        break;
                    }
                    reached = d;
                }
                (done, reached)
            }
            HistoryStep::Back(n) => (history.go_with_delta(-(n as i32)), depth + n),
        };
        if let Err(e) = done {
            web_sys::console::warn_1(&format!("troupe overlay: history {step:?} failed: {e:?}").into());
            WEB.with_borrow_mut(|w| w.sentinels.failed(reached));
        }
    }

    pub fn take_back_presses() -> usize {
        WEB.with_borrow_mut(|w| std::mem::take(&mut w.backs))
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::{install, sync, take_back_presses};

#[cfg(not(target_arch = "wasm32"))]
pub fn install(_wake: rune::Waker) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn sync(_depth: usize) {}

#[cfg(not(target_arch = "wasm32"))]
pub fn take_back_presses() -> usize {
    0
}
