//! The web client's DOM host for FigCraft iframes (`CanvasMountLayer.tsx`, `CanvasShell.tsx`).
//!
//! Each mounted project canvas owns one `<iframe>` appended to `document.body` outside the tab
//! tree so closing or switching tabs never reloads it (AD-19). While the canvas tab is visible
//! and no Rune overlay covers the viewport, the iframe is positioned over the measured slot rect;
//! otherwise (hidden tab, phone width, or any Rune menu/dialog open) it moves to the 1×1 offscreen
//! position (`left: -10000px`), keeping its wasm message loop alive while leaving Rune menus over
//! it completely readable.

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    use rune::Waker;
    use serde_json::Value;
    use ui::canvas::OpenInfo;
    use ui::canvas::bridge::{build_embed_url, canvas_origin_of};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use web_sys::{HtmlIFrameElement, MessageEvent};

    struct Frame {
        mount_seq: u64,
        iframe: HtmlIFrameElement,
        canvas_origin: String,
        inbox: Rc<RefCell<Vec<Value>>>,
        _listener: Closure<dyn FnMut(MessageEvent)>,
        last_rect: Option<[i32; 4]>,
    }

    impl Drop for Frame {
        fn drop(&mut self) {
            if let Some(window) = web_sys::window() {
                let _ = window.remove_event_listener_with_callback("message", self._listener.as_ref().unchecked_ref());
            }
            self.iframe.remove();
        }
    }

    #[derive(Default)]
    pub struct WebCanvases {
        frames: RefCell<BTreeMap<String, Frame>>,
    }

    impl WebCanvases {
        pub fn mount(&self, project_id: &str, doc_name: &str, info: &OpenInfo, wake: Waker) {
            let Some(window) = web_sys::window() else { return };
            let Some(document) = window.document() else { return };
            let Some(body) = document.body() else { return };
            let page_origin = window.location().origin().unwrap_or_default();
            let canvas_origin = canvas_origin_of(&info.base_url, &page_origin);
            let Ok(el) = document.create_element("iframe") else { return };
            let Ok(iframe) = el.dyn_into::<HtmlIFrameElement>() else { return };

            iframe.set_title(&format!("Canvas — {doc_name}"));
            let _ = iframe.set_attribute("allow", "clipboard-read; clipboard-write");
            let _ = iframe.set_attribute("data-canvas-project", project_id);
            let _ = iframe.set_attribute("data-canvas-mount-seq", &info.mount_seq.to_string());
            apply_offscreen(&iframe);

            let inbox = Rc::new(RefCell::new(Vec::<Value>::new()));
            let listener_inbox = Rc::clone(&inbox);
            let listener_origin = canvas_origin.clone();
            let listener_iframe = iframe.clone();
            let listener = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
                if event.origin() != listener_origin {
                    return;
                }
                let expected = listener_iframe.content_window();
                let source = event.source();
                let same_source = match (expected, source) {
                    (Some(a), Some(b)) => <web_sys::Window as AsRef<wasm_bindgen::JsValue>>::as_ref(&a) == <js_sys::Object as AsRef<wasm_bindgen::JsValue>>::as_ref(&b),
                    _ => false,
                };
                if !same_source {
                    return;
                }
                let Ok(json_str) = js_sys::JSON::stringify(&event.data()) else {
                    return;
                };
                let Some(text) = json_str.as_string() else {
                    return;
                };
                if let Ok(value) = serde_json::from_str::<Value>(&text) {
                    listener_inbox.borrow_mut().push(value);
                    wake();
                }
            });
            if let Err(e) = window.add_event_listener_with_callback("message", listener.as_ref().unchecked_ref()) {
                log::warn!("[canvas] web addEventListener(message) failed: {e:?}");
                return;
            }
            if let Err(e) = body.append_child(&iframe) {
                log::warn!("[canvas] web appendChild(iframe) failed: {e:?}");
                let _ = window.remove_event_listener_with_callback("message", listener.as_ref().unchecked_ref());
                return;
            }
            let url = build_embed_url(&info.base_url, &info.canvas_id, &info.canvas_token, &page_origin, info.mount_seq);
            iframe.set_src(&url);
            log::info!("[canvas] web iframe mounted project={project_id} mount_seq={}", info.mount_seq);

            self.frames.borrow_mut().insert(
                project_id.to_owned(),
                Frame {
                    mount_seq: info.mount_seq,
                    iframe,
                    canvas_origin,
                    inbox,
                    _listener: listener,
                    last_rect: None,
                },
            );
        }

        pub fn drop_editor(&self, project_id: &str) {
            if self.frames.borrow_mut().remove(project_id).is_some() {
                log::info!("[canvas] web iframe unmounted project={project_id}");
            }
        }

        pub fn post(&self, project_id: &str, messages: &[Value]) {
            let frames = self.frames.borrow();
            let Some(frame) = frames.get(project_id) else { return };
            let Some(win) = frame.iframe.content_window() else { return };
            for message in messages {
                let Ok(text) = serde_json::to_string(message) else { continue };
                let Ok(js_obj) = js_sys::JSON::parse(&text) else { continue };
                if let Err(e) = win.post_message(&js_obj, &frame.canvas_origin) {
                    log::warn!("[canvas] web postMessage failed for project {project_id}: {e:?}");
                }
            }
        }

        pub fn take_messages(&self) -> Vec<(String, Vec<Value>)> {
            let frames = self.frames.borrow();
            let mut out = Vec::new();
            for (project_id, frame) in frames.iter() {
                let msgs = std::mem::take(&mut *frame.inbox.borrow_mut());
                if !msgs.is_empty() {
                    out.push((project_id.clone(), msgs));
                }
            }
            out
        }

        /// Places each mounted iframe over its visible slot rect, or offscreen when hidden or
        /// covered by a Rune menu/dialog.
        pub fn sync_positions(&self, rects: &BTreeMap<String, [f32; 4]>) {
            let mut frames = self.frames.borrow_mut();
            for (project_id, frame) in frames.iter_mut() {
                let next = rects
                    .get(project_id)
                    .filter(|r| r[2] >= 1.0 && r[3] >= 1.0)
                    .map(|r| [r[0].round() as i32, r[1].round() as i32, r[2].round() as i32, r[3].round() as i32]);
                if frame.last_rect == next {
                    continue;
                }
                frame.last_rect = next;
                match next {
                    Some([left, top, width, height]) => {
                        let style = frame.iframe.style();
                        let _ = style.set_property("position", "fixed");
                        let _ = style.set_property("left", &format!("{left}px"));
                        let _ = style.set_property("top", &format!("{top}px"));
                        let _ = style.set_property("width", &format!("{width}px"));
                        let _ = style.set_property("height", &format!("{height}px"));
                        let _ = style.set_property("border", "0");
                        let _ = style.set_property("z-index", "10");
                        let _ = style.set_property("display", "block");
                        let _ = frame.iframe.remove_attribute("tabindex");
                        let _ = frame.iframe.remove_attribute("aria-hidden");
                        let _ = frame.iframe.set_attribute("data-canvas-frame-state", "visible");
                    }
                    None => apply_offscreen(&frame.iframe),
                }
            }
        }
    }

    fn apply_offscreen(iframe: &HtmlIFrameElement) {
        let style = iframe.style();
        let _ = style.set_property("position", "fixed");
        let _ = style.set_property("left", "-10000px");
        let _ = style.set_property("top", "0px");
        let _ = style.set_property("width", "1px");
        let _ = style.set_property("height", "1px");
        let _ = style.set_property("border", "0");
        let _ = style.set_property("z-index", "10");
        let _ = style.set_property("display", "block");
        let _ = iframe.set_attribute("tabindex", "-1");
        let _ = iframe.set_attribute("aria-hidden", "true");
        let _ = iframe.set_attribute("data-canvas-frame-state", "hidden");
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::collections::BTreeMap;

    #[derive(Default)]
    pub struct WebCanvases;

    impl WebCanvases {
        pub fn sync_positions(&self, _rects: &BTreeMap<String, [f32; 4]>) {}
    }
}

pub use imp::WebCanvases;
