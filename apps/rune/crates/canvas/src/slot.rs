//! The mounted editors and the compositor that draws them. Each editor has a mark: its slot is an
//! `El::effect(mark)` keyed [`slot_key`]. Each frame the compositor runs every editor whose slot is
//! on screen, has egui-wgpu draw it into the editor's own texture, and copies that texture into
//! the slot at its painter position, so Rune's menus and dialogs above it stay above it. An
//! editor that is still loading, with no slot on screen, runs unpainted frames until it is ready.
//!
//! The editors live on the window's thread, where `poll`, the view's handlers and the compositor
//! all run.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rune::compose::{Compositor, DeviceKey, Frame};
use rune::{Cursor, Waker};
use serde_json::Value;

use crate::blit::{Blit, Target};
use crate::engine::Engine;
use crate::input;

/// The effect marks editors take, from here up; below it a compositor wrapping this one may use
/// its own.
const FIRST_MARK: u32 = 0x00ca_0000;

/// Platform side-effects emitted by egui's `PlatformOutput` for the Rune shell to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlatformAction {
    CopyText(String),
    OpenUrl(String),
}

struct Mounted {
    engine: Engine,
    wake: Waker,
    dirty_repaint: Arc<AtomicBool>,
    input: Vec<egui::Event>,
    focused: bool,
    cursor: Cursor,
    actions: Vec<PlatformAction>,
    renderer: Option<(DeviceKey, egui_wgpu::Renderer)>,
    /// The editor's last picture and the size it was drawn at.
    picture: Option<Picture>,
    born: Instant,
    last_frame: Option<Instant>,
}

struct Picture {
    size: [u32; 2],
    target: Target,
}

thread_local! {
    static MOUNTED: RefCell<BTreeMap<u32, Mounted>> = const { RefCell::new(BTreeMap::new()) };
    static NEXT_MARK: std::cell::Cell<u32> = const { std::cell::Cell::new(FIRST_MARK) };
}

/// The key of the slot element that shows the editor `mark`.
pub fn slot_key(mark: u32) -> String {
    format!("canvas-editor-{mark}")
}

/// One mounted editor, owned by the host side; dropping it unmounts the editor.
#[derive(Debug)]
pub struct Handle {
    mark: u32,
}

impl Handle {
    /// Mounts an empty editor for the host mount `mount_seq`; `wake` runs the app's `poll`
    /// when the editor has messages for it or when FigCraft's render worker finishes a frame.
    pub fn mount(mount_seq: u64, wake: Waker) -> Handle {
        let mark = NEXT_MARK.with(|n| {
            let mark = n.get();
            n.set(mark.wrapping_add(1).max(FIRST_MARK));
            mark
        });
        let dirty_repaint = Arc::new(AtomicBool::new(true));
        let cb_dirty = Arc::clone(&dirty_repaint);
        let cb_wake = wake.clone();
        let engine = Engine::with_repaint(
            mount_seq,
            Some(Box::new(move || {
                cb_dirty.store(true, Ordering::Release);
                cb_wake();
            })),
        );
        let mounted = Mounted {
            engine,
            wake: wake.clone(),
            dirty_repaint,
            input: Vec::new(),
            focused: false,
            cursor: Cursor::Default,
            actions: Vec::new(),
            renderer: None,
            picture: None,
            born: Instant::now(),
            last_frame: None,
        };
        MOUNTED.with(|m| m.borrow_mut().insert(mark, mounted));
        log::info!("[canvas] editor mounted mark={mark} mount_seq={mount_seq}");
        wake();
        Handle { mark }
    }

    pub fn mark(&self) -> u32 {
        self.mark
    }

    /// A host message for the editor; its answers come back through [`Handle::take_messages`].
    pub fn post(&self, message: &Value) {
        if let Some(wake) = self.with(|m| {
            m.engine.receive(message);
            m.dirty_repaint.store(true, Ordering::Release);
            m.wake.clone()
        }) {
            wake();
        }
    }

    pub fn take_messages(&self) -> Vec<Value> {
        self.with(|m| m.engine.take_outbox()).unwrap_or_default()
    }

    pub fn take_actions(&self) -> Vec<PlatformAction> {
        self.with(|m| std::mem::take(&mut m.actions)).unwrap_or_default()
    }

    pub fn cursor(&self) -> Cursor {
        self.with(|m| m.cursor).unwrap_or(Cursor::Default)
    }

    pub fn set_focused(&self, focused: bool) {
        self.with(|m| {
            if m.focused != focused {
                m.focused = focused;
                m.dirty_repaint.store(true, Ordering::Release);
            }
        });
    }

    /// Input for the editor's next frame, in the slot's own points.
    pub fn input(&self, event: egui::Event) {
        if let Some(wake) = self.with(|m| {
            m.input.push(event);
            m.dirty_repaint.store(true, Ordering::Release);
            m.wake.clone()
        }) {
            wake();
        }
    }

    /// Queues several egui input events for the editor's next frame.
    pub fn inputs(&self, events: impl IntoIterator<Item = egui::Event>) {
        if let Some(wake) = self.with(|m| {
            let before = m.input.len();
            m.input.extend(events);
            if m.input.len() > before {
                m.dirty_repaint.store(true, Ordering::Release);
                Some(m.wake.clone())
            } else {
                None
            }
        })
        .flatten()
        {
            wake();
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut Mounted) -> R) -> Option<R> {
        MOUNTED.with(|m| m.borrow_mut().get_mut(&self.mark).map(f))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        MOUNTED.with(|m| m.borrow_mut().remove(&self.mark));
        log::info!("[canvas] editor unmounted mark={}", self.mark);
    }
}

/// Draws the frame with every visible editor in its slot. Wrap it in the live driver with
/// `automation::install_with`.
#[derive(Default)]
pub struct CanvasCompositor {
    blit: Option<(DeviceKey, Blit)>,
}

impl CanvasCompositor {
    pub fn new() -> CanvasCompositor {
        CanvasCompositor::default()
    }
}

impl<S: 'static> Compositor<S> for CanvasCompositor {
    fn render(&mut self, f: &mut Frame<'_, S>) {
        let empty = MOUNTED.with(|cell| cell.borrow().is_empty());
        if empty {
            f.draw_ui();
            return;
        }
        let key = f.device_key();
        if self.blit.as_ref().is_none_or(|(k, _)| *k != key) {
            self.blit = Some((key, Blit::new(f.device, f.format, f.srgb_blend())));
        }
        let Some((_, blit)) = &self.blit else { return };
        let window = f.rt.window_viewport();
        let ppp = if window.0 > 0.0 { f.size.0 as f32 / window.0 } else { 1.0 };
        let max_side = f.device.limits().max_texture_dimension_2d;
        MOUNTED.with(|cell| {
            let mut mounted = cell.borrow_mut();
            for (mark, m) in mounted.iter_mut() {
                let shown = f.rt.presented(&slot_key(*mark)).map(|(rect, _)| rect).filter(|r| r[2] >= 1.0 && r[3] >= 1.0);
                m.engine.set_visible(shown.is_some());
                let Some([_, _, w, h]) = shown else {
                    if !m.engine.ready() && !m.engine.crashed() {
                        m.dirty_repaint.store(false, Ordering::Release);
                        let raw = raw_input(m, [640.0, 480.0], ppp, max_side);
                        m.engine.frame_unpainted(raw);
                    }
                    continue;
                };
                m.dirty_repaint.store(false, Ordering::Release);
                let raw = raw_input(m, [w, h], ppp, max_side);
                let Some(out) = m.engine.frame(raw) else {
                    m.picture = None;
                    continue;
                };
                collect_platform_output(m, &out.platform_output);
                paint(m, key, f.device, f.queue, f.encoder, blit, [w, h], ppp, out);
            }
            for m in mounted.values() {
                if m.engine.has_messages() || !m.actions.is_empty() {
                    (m.wake)();
                }
            }
        });
        let clear = f.rt.clear_color();
        f.ui.set_fade_stops(f.rt.fade_stops());
        f.ui.prepare(f.device, f.queue, &mut f.rt.text.atlas, &f.rt.instances, f.size);
        let queue = f.queue;
        MOUNTED.with(|cell| {
            let mounted = cell.borrow();
            f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
                if let Some(picture) = mounted.get(&site.mark).and_then(|m| m.picture.as_ref()) {
                    blit.draw(queue, pass, site, &picture.target);
                }
            });
        });
    }

    fn animating(&self) -> bool {
        MOUNTED.with(|cell| {
            cell.borrow().values().any(|m| {
                !m.input.is_empty()
                    || m.dirty_repaint.load(Ordering::Acquire)
                    || m.engine.repaint_after().is_some_and(|after| m.last_frame.is_none_or(|t| t.elapsed() + Duration::from_millis(1) >= after))
            })
        })
    }
}

fn collect_platform_output(m: &mut Mounted, out: &egui::PlatformOutput) {
    m.cursor = input::cursor(out.cursor_icon);
    for cmd in &out.commands {
        match cmd {
            egui::OutputCommand::CopyText(text) if !text.is_empty() => {
                m.actions.push(PlatformAction::CopyText(text.clone()));
            }
            egui::OutputCommand::OpenUrl(open) if !open.url.is_empty() => {
                m.actions.push(PlatformAction::OpenUrl(open.url.clone()));
            }
            _ => {}
        }
    }
}

fn raw_input(m: &mut Mounted, size: [f32; 2], ppp: f32, max_side: u32) -> egui::RawInput {
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1]))),
        time: Some(m.born.elapsed().as_secs_f64()),
        max_texture_side: Some(max_side as usize),
        events: std::mem::take(&mut m.input),
        focused: m.focused,
        ..Default::default()
    };
    raw.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
    m.last_frame = Some(Instant::now());
    raw
}

#[allow(clippy::too_many_arguments)]
fn paint(
    m: &mut Mounted,
    key: DeviceKey,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    blit: &Blit,
    size: [f32; 2],
    ppp: f32,
    mut out: egui::FullOutput,
) {
    if m.renderer.as_ref().is_none_or(|(k, _)| *k != key) {
        m.renderer = Some((key, egui_wgpu::Renderer::new(device, Blit::SOURCE_FORMAT, egui_wgpu::RendererOptions::default())));
        m.picture = None;
    }
    let Some((_, renderer)) = m.renderer.as_mut() else { return };
    for (id, deltas) in &out.textures_delta.set {
        for delta in deltas.iter() {
            renderer.update_texture(device, queue, *id, delta);
        }
    }
    let px = [((size[0] * ppp).round() as u32).max(1), ((size[1] * ppp).round() as u32).max(1)];
    if m.picture.as_ref().is_none_or(|p| p.size != px) {
        m.picture = Some(Picture { size: px, target: blit.picture(device, px) });
    }
    let jobs = m.engine.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
    let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: px, pixels_per_point: out.pixels_per_point };
    let extra = renderer.update_buffers(device, queue, encoder, &jobs, &screen);
    if !extra.is_empty() {
        queue.submit(extra);
    }
    if let Some(picture) = &m.picture {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas editor"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &picture.target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        renderer.render(&mut pass, &jobs, &screen);
    }
    for id in &out.textures_delta.free {
        renderer.free_texture(id);
    }
    out.textures_delta.clear();
}
