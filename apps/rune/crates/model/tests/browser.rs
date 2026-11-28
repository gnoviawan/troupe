use model::browser::{
    ControlEvent, ControlState, SessionEvent, SessionState, ViewportOrientation, ViewportPreset,
    clamp_browser_zoom, display_host, next_zoom_step, normalize_browser_input_url,
    parse_localhost_port, resolve_viewport_dimensions, split_display_url, step_control,
    step_session,
};
use model::Effect;
use serde_json::{Value, json};

use crate::Store;
use crate::common::{Replay, call, parse, to_value};

pub struct BrowserSession;

impl Store for BrowserSession {
    type State = SessionState;

    fn setup(_: &Value, initial: &Value) -> Result<Self::State, String> {
        parse(initial)
    }

    fn step(
        state: &mut Self::State,
        event: &Value,
        _: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        let ev: SessionEvent = call(event)?;
        let reply = step_session(state, ev, 0);
        Ok((Vec::new(), reply))
    }

    fn query(_: &Self::State, event: &Value) -> Result<Value, String> {
        let args = &event["args"];
        Ok(match event["action"].as_str().unwrap_or_default() {
            "normalizeBrowserInputUrl" => {
                json!(normalize_browser_input_url(args[0].as_str().unwrap_or_default()))
            }
            "splitDisplayUrl" => to_value(&split_display_url(args[0].as_str().unwrap_or_default())),
            "displayHost" => json!(display_host(args[0].as_str().unwrap_or_default())),
            "parseLocalhostPort" => {
                let parsed = parse_localhost_port(args[0].as_str().unwrap_or_default())
                    .map(|(host, port)| json!({ "host": host, "port": port }));
                to_value(&parsed)
            }
            "clampBrowserZoom" => json!(clamp_browser_zoom(args[0].as_f64().unwrap_or(1.0))),
            "nextZoomStep" => json!(next_zoom_step(
                args[0].as_f64().unwrap_or(1.0),
                args[1].as_str() == Some("in"),
            )),
            "resolveViewportDimensions" => {
                let preset: ViewportPreset = parse(&args[0])?;
                let orientation: ViewportOrientation = parse(&args[1])?;
                let custom: Option<u32> = parse(&args[2])?;
                to_value(&resolve_viewport_dimensions(preset, orientation, custom))
            }
            other => return Err(format!("no browser-session query {other}")),
        })
    }
}

pub struct BrowserControl;

impl Store for BrowserControl {
    type State = ControlState;

    fn setup(_: &Value, initial: &Value) -> Result<Self::State, String> {
        parse(initial)
    }

    fn step(
        state: &mut Self::State,
        event: &Value,
        _: &mut Replay,
    ) -> Result<(Vec<Effect>, Option<Value>), String> {
        let ev: ControlEvent = call(event)?;
        step_control(state, ev);
        Ok((Vec::new(), None))
    }

    fn query(_: &Self::State, event: &Value) -> Result<Value, String> {
        Err(format!("no browser-control query {}", event["action"]))
    }
}
