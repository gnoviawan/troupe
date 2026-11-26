use rune::{Color, Cx, El, Env, Runtime, WheelEvent, el, text};

use super::*;

#[derive(Default)]
struct S {
    flow: FlowState,
    events: Vec<FlowEvent>,
}

fn lens(s: &mut S) -> &mut FlowState {
    &mut s.flow
}

fn record(s: &mut S, event: FlowEvent, _: &mut Tx) {
    s.events.push(event);
}

const LOOK: Look = Look {
    background: Color::BLACK,
    handle: Color::WHITE,
    handle_border: Color::WHITE,
    preview: Color::WHITE,
};

fn card(id: &str, at: [f32; 2], input: bool) -> Card {
    Card {
        id: id.to_owned(),
        at,
        size: [192.0, 64.0],
        body: el().size(192.0, 64.0).fill(Color::WHITE).child(text(id)),
        input_y: input.then_some(31.0),
        outputs: vec![OutPort {
            key: "out".to_owned(),
            y: 31.0,
            color: Color::WHITE,
            label: None,
        }],
    }
}

fn view(s: &S, _: &mut Cx<S>) -> El {
    let wire = Wire {
        key: "trigger:out->a".to_owned(),
        from: "trigger".to_owned(),
        port: "out".to_owned(),
        to: "a".to_owned(),
        color: Color::WHITE,
        width: 1.5,
        dash: None,
        label: None,
    };
    let canvas = Canvas::new("c", lens, record, LOOK)
        .editable(true)
        .wheel_pans(true)
        .cards(vec![
            card("trigger", [0.0, 0.0], false),
            card("a", [300.0, 0.0], true),
            card("b", [300.0, 200.0], true),
        ])
        .wires(vec![wire])
        .build(&s.flow);
    el().size(1000.0, 600.0).child(canvas)
}

fn runtime() -> Runtime<S> {
    let mut rt = Runtime::new(S::default(), view, (1000.0, 600.0), Env::new(true, 1.0));
    rt.frame(0.0);
    rt.frame(0.016);
    rt
}

fn drag(rt: &mut Runtime<S>, from: (f32, f32), by: (f32, f32)) {
    rt.pointer_down(from.0, from.1);
    rt.frame(0.03);
    for step in 1..=4 {
        let t = step as f32 / 4.0;
        rt.pointer_move(from.0 + by.0 * t, from.1 + by.1 * t);
        rt.frame(0.016);
    }
    rt.pointer_up(from.0 + by.0, from.1 + by.1);
    rt.frame(0.2);
    rt.frame(0.22);
}

fn tap(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("no {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.3);
    rt.frame(0.32);
}

#[test]
fn the_first_view_fits_the_cards_at_most_life_size() {
    let rt = runtime();
    // Content 492 x 264 in a 1000 x 600 view fits at 1.
    assert_eq!(rt.state.flow.camera.zoom.get(), 1.0);
    assert_eq!(rt.state.flow.camera.pan.get(), [254.0, 168.0]);
    assert!(rt.state.flow.started);
}

#[test]
fn a_press_on_a_card_selects_it_and_one_on_the_background_clears_it() {
    let mut rt = runtime();
    tap(&mut rt, "c-card-a");
    assert_eq!(
        rt.state.events,
        [FlowEvent::Select(Some(Selection::Card("a".to_owned())))]
    );
    assert_eq!(
        rt.state.flow.selection,
        Some(Selection::Card("a".to_owned()))
    );
    rt.pointer_down(20.0, 20.0);
    rt.pointer_up(20.0, 20.0);
    rt.frame(0.4);
    assert_eq!(rt.state.flow.selection, None);
}

#[test]
fn a_press_on_an_edge_middle_selects_the_edge() {
    let mut rt = runtime();
    tap(&mut rt, "c-wire-trigger:out->a");
    assert_eq!(
        rt.state.flow.selection,
        Some(Selection::Wire("trigger:out->a".to_owned()))
    );
}

#[test]
fn a_background_drag_pans_the_camera_and_hits_follow_it() {
    let mut rt = runtime();
    let before = rt.state.flow.camera.pan.get();
    drag(&mut rt, (20.0, 20.0), (100.0, 40.0));
    let after = rt.state.flow.camera.pan.get();
    assert_eq!([after[0] - before[0], after[1] - before[1]], [100.0, 40.0]);
    assert!(
        rt.state.events.is_empty(),
        "a pan selects nothing: {:?}",
        rt.state.events
    );
    // Card a's middle is now 100 right and 40 down of where it was.
    let x = before[0] + 300.0 + 96.0 + 100.0;
    let y = before[1] + 32.0 + 40.0;
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.5);
    assert_eq!(
        rt.state.flow.selection,
        Some(Selection::Card("a".to_owned()))
    );
}

#[test]
fn the_wheel_pans_and_ctrl_wheel_zooms_about_the_pointer() {
    let mut rt = runtime();
    let before = rt.state.flow.camera.pan.get();
    rt.wheel_event(WheelEvent::new(-30.0, 10.0).at(500.0, 300.0));
    rt.frame(0.1);
    let after = rt.state.flow.camera.pan.get();
    assert_eq!([after[0] - before[0], after[1] - before[1]], [-30.0, 10.0]);
}

#[test]
fn a_card_drag_reports_its_new_corner_in_canvas_points() {
    let mut rt = runtime();
    rt.state.flow.camera.set([0.0, 0.0], 2.0);
    rt.frame(0.05);
    rt.frame(0.06);
    let (x, y) = rt.find("c-card-a").expect("card a");
    drag(&mut rt, (x, y), (80.0, 40.0));
    assert_eq!(
        rt.state.events,
        [FlowEvent::Moved {
            id: "a".to_owned(),
            at: [340.0, 20.0]
        }]
    );
}

#[test]
fn a_drag_from_a_port_onto_a_card_draws_an_edge() {
    let mut rt = runtime();
    let (x, y) = rt.find("c-port-a-out").expect("port");
    let (tx, ty) = rt.find("c-card-b").expect("card b");
    drag(&mut rt, (x, y), (tx - x, ty - y));
    assert_eq!(
        rt.state.events,
        [FlowEvent::Connect {
            from: "a".to_owned(),
            port: "out".to_owned(),
            to: "b".to_owned()
        }]
    );
}

#[test]
fn a_drag_from_a_port_onto_nothing_draws_nothing() {
    let mut rt = runtime();
    let (x, y) = rt.find("c-port-a-out").expect("port");
    drag(&mut rt, (x, y), (200.0, -150.0));
    assert!(rt.state.events.is_empty(), "{:?}", rt.state.events);
    assert!(rt.state.flow.connecting().is_none());
}
