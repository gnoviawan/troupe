//! Watches on the `/ws` machine: request, continuity of revisions, and the
//! bounded retry of a watch the host refused for a passing reason.

use rune_core::machine::Attempt;
use serde_json::{Value, json};
use troupe_protocol::ws::{WsError, WsErrorCode, WsRequestType, WsWatchFrame};

use super::{Fx, Output, Purpose, WatchKey, WatchSync, WsClient};
use crate::machine::Effect;

/// Retries of a watch the host refused for a passing reason (`rate_limited`),
/// waiting `WATCH_RETRY_S * 2^n` before retry `n`.
pub const WATCH_RETRIES: u32 = 4;
pub const WATCH_RETRY_S: f64 = 1.0;

impl WsClient {
    /// Ask again for every watch whose retry came due.
    pub(super) fn retry_watches(&mut self, now: f64, fx: &mut Fx) {
        let Some(socket) = self.link.live() else {
            return;
        };
        let due: Vec<(WatchKey, u32)> = self
            .watches
            .iter()
            .filter_map(|(key, sync)| match sync {
                WatchSync::Retrying { tries, due } if *due <= now => Some((key.clone(), *tries)),
                _ => None,
            })
            .collect();
        for (key, tries) in due {
            self.request_watch(socket, key, tries, fx);
        }
    }

    /// A newer watch request or an unwatch replaces any watch request still in
    /// flight, so its late reply is ignored.
    pub(super) fn forget_watch(&mut self, key: &WatchKey) {
        self.inflight
            .retain(|_, p| !matches!(p, Purpose::Watch(k) if k == key));
    }

    pub(super) fn watch_refused(&mut self, key: WatchKey, error: WsError, now: f64, fx: &mut Fx) {
        let refused = [WsErrorCode::Unsupported, WsErrorCode::NotFound]
            .iter()
            .any(|c| c.as_str() == error.code);
        if let Some(sync) = self.watches.get_mut(&key) {
            let tries = match sync {
                WatchSync::Requesting { tries } => *tries,
                _ => WATCH_RETRIES,
            };
            *sync = if refused {
                WatchSync::Refused
            } else if tries < WATCH_RETRIES {
                let due = now + WATCH_RETRY_S * 2f64.powi(tries as i32);
                fx.push(Effect::WakeAt(due));
                WatchSync::Retrying {
                    tries: tries + 1,
                    due,
                }
            } else {
                WatchSync::Offline
            };
        }
        log::warn!("[client] watch {:?} refused: {}", key.0, error.code);
        let (watch, scope) = key;
        fx.push(Effect::Output(Output::WatchRefused {
            watch,
            scope,
            error,
        }));
    }

    pub(super) fn request_watch(
        &mut self,
        socket: Attempt,
        key: WatchKey,
        tries: u32,
        fx: &mut Fx,
    ) {
        let payload = json!({ "watch": key.0, "scope": key.1 });
        self.forget_watch(&key);
        self.watches
            .insert(key.clone(), WatchSync::Requesting { tries });
        self.send(
            socket,
            Purpose::Watch(key),
            WsRequestType::Watch,
            payload,
            None,
            fx,
        );
    }

    pub(super) fn watch_frame(&mut self, frame: WsWatchFrame, fx: &mut Fx) {
        let Some(socket) = self.link.live() else {
            return;
        };
        match frame {
            WsWatchFrame::State(state) => {
                let key = (state.watch, state.scope);
                let Some(sync) = self.watches.get_mut(&key) else {
                    return;
                };
                *sync = WatchSync::Live(state.revision);
                let (watch, scope) = key;
                fx.push(Effect::Output(Output::WatchState {
                    watch,
                    scope,
                    state: state.state,
                }));
            }
            WsWatchFrame::Delta(delta) => {
                let key = (delta.watch, delta.scope);
                let Some(WatchSync::Live(last)) = self.watches.get_mut(&key) else {
                    return;
                };
                let rev = delta.revision;
                if rev.host_epoch == last.host_epoch && rev.n <= last.n {
                    return;
                }
                let resync = delta.delta.get("resync") == Some(&Value::Bool(true));
                if rev.host_epoch != last.host_epoch || rev.n != last.n + 1 || resync {
                    log::info!(
                        "[client] watch {:?} lost continuity at n {}; rewatching",
                        key.0,
                        rev.n
                    );
                    return self.request_watch(socket, key, 0, fx);
                }
                *last = rev;
                let (watch, scope) = key;
                fx.push(Effect::Output(Output::WatchDelta {
                    watch,
                    scope,
                    delta: delta.delta,
                }));
            }
        }
    }
}
