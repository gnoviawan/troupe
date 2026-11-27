//! The canvas element: cards at canvas points under a camera, edges drawn between their ports,
//! and the gestures. A drag on the background pans and the wheel pans or zooms, through the
//! camera's `Live` values only, so neither runs the view. A press on a card or an edge's middle
//! selects it. In edit mode a card drags to a new place and a drag from an output port to a
//! card draws an edge; the canvas reports both and changes no graph itself.

use std::rc::Rc;

use rune::{Color, Drag, El, LineCap, MagnifyEvent, Phase, Shape, Tx, WheelEvent, canvas, el};

use crate::camera::Camera;
use crate::edge;

/// How far a press travels (view points) before it drags rather than clicks.
const DRAG_THRESHOLD: f32 = 3.0;
pub const HANDLE: f32 = 10.0;
/// The square that selects an edge, over its middle.
const WIRE_HIT: f32 = 16.0;
/// Room the edge drawing keeps around its curves, for the stroke.
const INK_PAD: f32 = 4.0;

pub type Lens<S> = fn(&mut S) -> &mut FlowState;
pub type OnEvent<S> = fn(&mut S, FlowEvent, &mut Tx);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    Card(String),
    Wire(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
enum Gesture {
    #[default]
    Idle,
    Pan {
        from: [f32; 2],
    },
    Move {
        id: String,
        by: [f32; 2],
    },
    Connect {
        from: String,
        port: String,
        start: [f32; 2],
        end: [f32; 2],
    },
}

#[derive(Debug, Default)]
pub struct FlowState {
    pub camera: Camera,
    pub selection: Option<Selection>,
    gesture: Gesture,
    /// The canvas's size at its last layout, in view points.
    pub size: [f32; 2],
    /// The first view is placed (`Camera::start`); later layouts keep the camera.
    pub started: bool,
}

impl FlowState {
    /// The card being dragged and how far, in canvas points.
    pub fn moving(&self) -> Option<(&str, [f32; 2])> {
        match &self.gesture {
            Gesture::Move { id, by } => Some((id, *by)),
            _ => None,
        }
    }

    /// The port an edge is being drawn from.
    pub fn connecting(&self) -> Option<(&str, &str)> {
        match &self.gesture {
            Gesture::Connect { from, port, .. } => Some((from, port)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FlowEvent {
    Select(Option<Selection>),
    /// A card was dropped with its top-left corner at `at`.
    Moved {
        id: String,
        at: [f32; 2],
    },
    /// An edge was drawn from a port onto a card.
    Connect {
        from: String,
        port: String,
        to: String,
    },
}

pub struct OutPort {
    pub key: String,
    /// The handle's centre below the card's top.
    pub y: f32,
    pub color: Color,
    /// Drawn right of the handle (an `error` port's name).
    pub label: Option<El>,
}

pub struct Card {
    pub id: String,
    /// Top-left corner, in canvas points.
    pub at: [f32; 2],
    pub size: [f32; 2],
    pub body: El,
    /// The input handle's centre below the card's top; none for a card nothing enters.
    pub input_y: Option<f32>,
    pub outputs: Vec<OutPort>,
}

pub struct Wire {
    pub key: String,
    pub from: String,
    pub port: String,
    pub to: String,
    pub color: Color,
    pub width: f32,
    /// `[on, off]` points.
    pub dash: Option<[f32; 2]>,
    /// Drawn over the curve's middle.
    pub label: Option<El>,
}

#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub background: Color,
    pub handle: Color,
    pub handle_border: Color,
    /// The edge being drawn.
    pub preview: Color,
}

pub struct Canvas<S> {
    key: String,
    lens: Lens<S>,
    on: OnEvent<S>,
    look: Look,
    editable: bool,
    wheel_pans: bool,
    readable: f32,
    cards: Vec<Card>,
    wires: Vec<Wire>,
}

fn contains(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] <= r[0] + r[2] && p[1] >= r[1] && p[1] <= r[1] + r[3]
}

fn select<S>(s: &mut S, lens: Lens<S>, on: OnEvent<S>, selection: Option<Selection>, tx: &mut Tx) {
    lens(s).selection = selection.clone();
    on(s, FlowEvent::Select(selection), tx);
}

impl<S: 'static> Canvas<S> {
    pub fn new(key: impl Into<String>, lens: Lens<S>, on: OnEvent<S>, look: Look) -> Canvas<S> {
        Canvas {
            key: key.into(),
            lens,
            on,
            look,
            editable: false,
            wheel_pans: false,
            readable: 0.75,
            cards: Vec::new(),
            wires: Vec::new(),
        }
    }

    /// Cards drag and output ports draw edges.
    pub fn editable(mut self, on: bool) -> Self {
        self.editable = on;
        self
    }

    /// The wheel pans the canvas; off, it is left to a scrolling page around it (zoom still
    /// takes a pinch or Ctrl + wheel).
    pub fn wheel_pans(mut self, on: bool) -> Self {
        self.wheel_pans = on;
        self
    }

    /// The least zoom the first view fits the graph at.
    pub fn readable(mut self, zoom: f32) -> Self {
        self.readable = zoom;
        self
    }

    pub fn cards(mut self, cards: Vec<Card>) -> Self {
        self.cards = cards;
        self
    }

    pub fn wires(mut self, wires: Vec<Wire>) -> Self {
        self.wires = wires;
        self
    }

    pub fn build(self, state: &FlowState) -> El {
        let Canvas {
            key,
            lens,
            on,
            look,
            editable,
            wheel_pans,
            readable,
            cards,
            wires,
        } = self;
        let moving = state.moving().map(|(id, by)| (id.to_owned(), by));
        let place = |card: &Card| match &moving {
            Some((id, by)) if *id == card.id => [card.at[0] + by[0], card.at[1] + by[1]],
            _ => card.at,
        };
        let rects: Rc<Vec<(String, [f32; 4])>> = Rc::new(
            cards
                .iter()
                .map(|c| {
                    let at = place(c);
                    (c.id.clone(), [at[0], at[1], c.size[0], c.size[1]])
                })
                .collect(),
        );
        let rect_of = |id: &str| rects.iter().find(|(i, _)| i == id).map(|(_, r)| *r);
        let out_at = |id: &str, port: &str| {
            let card = cards.iter().find(|c| c.id == id)?;
            let r = rect_of(id)?;
            let p = card.outputs.iter().find(|p| p.key == port)?;
            Some([r[0] + r[2], r[1] + p.y])
        };
        let in_at = |id: &str| {
            let card = cards.iter().find(|c| c.id == id)?;
            let r = rect_of(id)?;
            Some([r[0], r[1] + card.input_y?])
        };

        let mut world: Vec<El> = Vec::new();

        let mut curves: Vec<Stroke> = Vec::new();
        let mut overlays: Vec<El> = Vec::new();
        for wire in wires {
            let (Some(a), Some(b)) = (out_at(&wire.from, &wire.port), in_at(&wire.to)) else {
                continue;
            };
            let curve = edge::bezier(a, b);
            let selected = state.selection == Some(Selection::Wire(wire.key.clone()));
            curves.push(Stroke {
                curve,
                color: wire.color,
                width: wire.width + if selected { 1.0 } else { 0.0 },
                dash: wire.dash,
            });
            let mid = edge::midpoint(&curve);
            if let Some(label) = wire.label {
                overlays.push(
                    el().absolute(mid[0] - 60.0, mid[1] - 10.0)
                        .size(120.0, 20.0)
                        .center()
                        .child(label)
                        .a11y_hidden(),
                );
            }
            let id = wire.key.clone();
            overlays.push(
                el().key(format!("{key}-wire-{}", wire.key))
                    .absolute(mid[0] - WIRE_HIT / 2.0, mid[1] - WIRE_HIT / 2.0)
                    .size(WIRE_HIT, WIRE_HIT)
                    .label(format!("Edge {}", wire.key))
                    .on_press_tx(move |s: &mut S, tx| {
                        select(s, lens, on, Some(Selection::Wire(id.clone())), tx)
                    }),
            );
        }
        if let Gesture::Connect { start, end, .. } = &state.gesture {
            curves.push(Stroke {
                curve: edge::bezier(*start, *end),
                color: look.preview,
                width: 1.5,
                dash: Some([5.0, 4.0]),
            });
        }
        if let Some(ink) = ink(&curves) {
            world.push(ink);
        }

        for card in cards {
            let at = place(&card);
            let [w, h] = card.size;
            let mut node = el()
                .key(format!("{key}-card-{}", card.id))
                .absolute(at[0], at[1])
                .size(w, h)
                .child(card.body);
            if let Some(y) = card.input_y {
                node = node.child(handle(look, HANDLE).absolute(-HANDLE / 2.0, y - HANDLE / 2.0));
            }
            for port in card.outputs {
                let anchor = [at[0] + w, at[1] + port.y];
                let mut dot = handle(look, HANDLE)
                    .key(format!("{key}-port-{}-{}", card.id, port.key))
                    .absolute(w - HANDLE / 2.0, port.y - HANDLE / 2.0)
                    .border(1.0, port.color);
                if editable {
                    let (from, key_) = (card.id.clone(), port.key.clone());
                    let targets = Rc::clone(&rects);
                    dot = dot.on_drag(move |s: &mut S, d: &mut Drag, tx| {
                        connect(s, lens, on, d, tx, &from, &key_, anchor, &targets);
                    });
                }
                node = node.child(dot);
                if let Some(label) = port.label {
                    node = node.child(
                        el().absolute(w + HANDLE / 2.0 + 3.0, port.y - 8.0)
                            .h(16.0)
                            .center_y()
                            .child(label),
                    );
                }
            }
            let id = card.id.clone();
            let origin = card.at;
            node = node.on_drag(move |s: &mut S, d: &mut Drag, tx| {
                drag_card(s, lens, on, d, tx, &id, origin, editable);
            });
            world.push(node);
        }
        world.extend(overlays);

        let content = bounds(&rects);
        let anchor_y = rects.first().map_or(0.0, |(_, r)| r[1] + r[3] / 2.0);
        let camera = &state.camera;
        let layer = el()
            .key(format!("{key}-world"))
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .origin(0.0, 0.0)
            .live_offset(&camera.pan)
            .live_scale(&camera.zoom)
            .children(world);
        el().key(key)
            .w_full()
            .h_full()
            .fill(look.background)
            .clip_rounded(0.0)
            .label("Automation canvas")
            .on_layout(move |s: &mut S, r: [f32; 4]| {
                let st = lens(s);
                st.size = [r[2], r[3]];
                if !st.started && r[2] > 0.0 && r[3] > 0.0 {
                    if let Some(content) = content {
                        st.camera.start(content, anchor_y, st.size, readable);
                    }
                    st.started = true;
                }
            })
            .on_drag(move |s: &mut S, d: &mut Drag, tx| pan(s, lens, on, d, tx))
            .on_wheel(move |s: &mut S, e: WheelEvent, tx| {
                let camera = &lens(s).camera;
                if e.mods.ctrl || e.mods.cmd {
                    camera.zoom_about([e.x, e.y], (-e.dy / 200.0).exp());
                } else if wheel_pans {
                    camera.pan_by(e.dx, e.dy);
                } else {
                    return false;
                }
                tx.live_only();
                true
            })
            .on_magnify(move |s: &mut S, e: MagnifyEvent, tx| {
                lens(s)
                    .camera
                    .zoom_about([e.x, e.y], (1.0 + e.delta).max(0.01));
                tx.live_only();
                true
            })
            .child(layer)
    }
}

fn handle(look: Look, size: f32) -> El {
    el().size(size, size)
        .radius(size / 2.0)
        .fill(look.handle)
        .border(1.0, look.handle_border)
}

/// The box the cards cover, `[x, y, w, h]`.
fn bounds(rects: &[(String, [f32; 4])]) -> Option<[f32; 4]> {
    let first = rects.first()?.1;
    let mut b = [first[0], first[1], first[0] + first[2], first[1] + first[3]];
    for (_, r) in rects {
        b = [
            b[0].min(r[0]),
            b[1].min(r[1]),
            b[2].max(r[0] + r[2]),
            b[3].max(r[1] + r[3]),
        ];
    }
    Some([b[0], b[1], b[2] - b[0], b[3] - b[1]])
}

struct Stroke {
    curve: edge::Curve,
    color: Color,
    width: f32,
    dash: Option<[f32; 2]>,
}

/// Every curve in one shape element placed over their extent.
fn ink(strokes: &[Stroke]) -> Option<El> {
    let lines: Vec<(Vec<[f32; 2]>, &Stroke)> = strokes
        .iter()
        .map(|s| (edge::flatten(&s.curve), s))
        .collect();
    let mut b = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for (points, _) in &lines {
        for p in points {
            b = [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ];
        }
    }
    if !b[0].is_finite() {
        return None;
    }
    let origin = [b[0] - INK_PAD, b[1] - INK_PAD];
    let local = |p: &[f32; 2]| [p[0] - origin[0], p[1] - origin[1]];
    let mut shape = Shape::new();
    for (points, stroke) in &lines {
        let points: Vec<[f32; 2]> = points.iter().map(local).collect();
        match stroke.dash {
            Some([on, off]) => {
                for piece in edge::dashes(&points, on, off) {
                    shape =
                        shape.stroke_with(piece, false, stroke.width, LineCap::Butt, stroke.color);
                }
            }
            None => shape = shape.stroke(points, stroke.width, stroke.color),
        }
    }
    Some(
        canvas(shape)
            .absolute(origin[0], origin[1])
            .size(b[2] - b[0] + 2.0 * INK_PAD, b[3] - b[1] + 2.0 * INK_PAD)
            .a11y_hidden(),
    )
}

fn travelled(d: &Drag) -> bool {
    d.dx.hypot(d.dy) >= DRAG_THRESHOLD
}

fn pan<S>(s: &mut S, lens: Lens<S>, on: OnEvent<S>, d: &mut Drag, tx: &mut Tx) {
    let st = lens(s);
    match d.phase {
        Phase::Down => {
            st.gesture = Gesture::Pan {
                from: st.camera.pan.get(),
            }
        }
        Phase::Move => {
            if let Gesture::Pan { from } = st.gesture {
                st.camera.pan.set([from[0] + d.dx, from[1] + d.dy]);
                tx.live_only();
            }
        }
        Phase::Up => {
            let gesture = std::mem::take(&mut st.gesture);
            if d.cancelled() {
                if let Gesture::Pan { from } = gesture {
                    st.camera.pan.set(from);
                }
            } else if !travelled(d) {
                select(s, lens, on, None, tx);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn drag_card<S>(
    s: &mut S,
    lens: Lens<S>,
    on: OnEvent<S>,
    d: &mut Drag,
    tx: &mut Tx,
    id: &str,
    origin: [f32; 2],
    movable: bool,
) {
    let st = lens(s);
    let zoom = st.camera.zoom.get();
    match d.phase {
        Phase::Down => {
            if movable {
                st.gesture = Gesture::Move {
                    id: id.to_owned(),
                    by: [0.0, 0.0],
                };
            }
        }
        Phase::Move => {
            if let Gesture::Move { by, .. } = &mut st.gesture
                && travelled(d)
            {
                *by = [d.dx / zoom, d.dy / zoom];
            }
        }
        Phase::Up => {
            let gesture = std::mem::take(&mut st.gesture);
            if d.cancelled() {
                return;
            }
            if !travelled(d) {
                select(s, lens, on, Some(Selection::Card(id.to_owned())), tx);
            } else if let Gesture::Move { by, .. } = gesture {
                on(
                    s,
                    FlowEvent::Moved {
                        id: id.to_owned(),
                        at: [origin[0] + by[0], origin[1] + by[1]],
                    },
                    tx,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn connect<S>(
    s: &mut S,
    lens: Lens<S>,
    on: OnEvent<S>,
    d: &mut Drag,
    tx: &mut Tx,
    from: &str,
    port: &str,
    anchor: [f32; 2],
    targets: &[(String, [f32; 4])],
) {
    let st = lens(s);
    let zoom = st.camera.zoom.get();
    match d.phase {
        Phase::Down => {
            st.gesture = Gesture::Connect {
                from: from.to_owned(),
                port: port.to_owned(),
                start: anchor,
                end: anchor,
            };
        }
        Phase::Move => {
            if let Gesture::Connect { start, end, .. } = &mut st.gesture {
                *end = [start[0] + d.dx / zoom, start[1] + d.dy / zoom];
            }
        }
        Phase::Up => {
            let gesture = std::mem::take(&mut st.gesture);
            let Gesture::Connect { end, .. } = gesture else {
                return;
            };
            if d.cancelled() {
                return;
            }
            if let Some((to, _)) = targets
                .iter()
                .find(|(id, r)| id != from && contains(*r, end))
            {
                on(
                    s,
                    FlowEvent::Connect {
                        from: from.to_owned(),
                        port: port.to_owned(),
                        to: to.clone(),
                    },
                    tx,
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "canvas_tests.rs"]
mod tests;
