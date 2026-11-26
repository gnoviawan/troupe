//! The gallery has one scene per design-system registry entry, in the same order, with the same
//! name and frame size; every scene builds; UI zoom scales the whole view from one factor.

use gallery::{SCENES, State, view};
use rune::{Env, Runtime};
use serde_json::Value;

fn rect(rt: &Runtime<State>, key: &str) -> [f32; 4] {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key))
        .map(|e| e.rect)
        .unwrap_or_else(|| panic!("no element keyed {key}"))
}

#[test]
fn scenes_equal_the_design_system_registry() {
    let registry: Value = serde_json::from_str(include_str!("../registry.json")).unwrap();
    let want: Vec<(String, String, f64, f64)> = registry
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["id"].as_str().unwrap().to_owned(),
                e["name"].as_str().unwrap().to_owned(),
                e["width"].as_f64().unwrap(),
                e["height"].as_f64().unwrap(),
            )
        })
        .collect();
    let have: Vec<(String, String, f64, f64)> = SCENES
        .iter()
        .map(|s| (s.id.to_owned(), s.name.to_owned(), f64::from(s.width), f64::from(s.height)))
        .collect();
    assert_eq!(have, want);
}

#[test]
fn every_scene_builds_its_frame_at_the_registry_size() {
    for scene in SCENES {
        let mut state = State::default();
        state.show(scene.id);
        let mut rt = Runtime::new(state, view, (scene.width, scene.height), Env::new(true, 1.0));
        rt.frame(0.0);
        let [_, _, w, h] = rect(&rt, &format!("scene/{}", scene.id));
        assert_eq!((w, h), (scene.width, scene.height), "{}", scene.id);
    }
}

#[test]
fn zoom_scales_the_whole_view_from_one_factor() {
    let root_width = |zoom: f32| {
        let state = State { zoom: ui::Zoom::new(zoom), ..State::default() };
        let mut rt = Runtime::new(state, view, (1000.0, 800.0), Env::new(true, 1.0));
        rt.frame(0.0);
        rect(&rt, "gallery-root")[2]
    };
    assert_eq!(root_width(1.0), 1000.0);
    // The view lays out in 1 / 1.25 as many points and draws them 1.25 times as large.
    assert_eq!(root_width(1.25), 800.0);
    assert_eq!(ui::Zoom::DEFAULT.zoom_in().zoom_in().factor(), 1.2);
    assert_eq!(ui::Zoom::new(3.0).zoom_in().factor(), 3.0);
    assert_eq!(ui::Zoom::new(0.5).zoom_out().factor(), 0.5);
}
