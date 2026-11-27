//! Pane input: what a Rune pane sends in `browser_tab_input`, parsed at
//! the `/ws` boundary and mapped to one CDP `Input.*` call. The user's own
//! input needs no consent ticket and skips the agent path's actionability
//! checks; it is still subject to the write capability the request needs.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::browser_automation::engine::cdp::CdpMethod;

/// Longest text one input event may carry.
const MAX_TEXT: usize = 4096;
/// Most touch points one event may carry.
const MAX_TOUCH_POINTS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointerAction {
    Down,
    Up,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Button {
    None,
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyAction {
    Down,
    Up,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TouchAction {
    Start,
    Move,
    End,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TouchPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub id: u32,
}

/// One input event from a pane. Coordinates are CSS px in the tab's
/// viewport. `modifiers` is CDP's bit set (Alt 1, Ctrl 2, Meta 4, Shift 8).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum PaneInput {
    Pointer {
        action: PointerAction,
        x: f64,
        y: f64,
        #[serde(default = "left")]
        button: Button,
        #[serde(default)]
        buttons: u32,
        #[serde(default = "one", rename = "clickCount")]
        click_count: u32,
        #[serde(default)]
        modifiers: u32,
    },
    Wheel {
        x: f64,
        y: f64,
        #[serde(rename = "deltaX")]
        delta_x: f64,
        #[serde(rename = "deltaY")]
        delta_y: f64,
        #[serde(default)]
        modifiers: u32,
    },
    Key {
        action: KeyAction,
        key: String,
        #[serde(default)]
        code: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(default, rename = "keyCode")]
        key_code: u32,
        #[serde(default)]
        modifiers: u32,
    },
    /// Committed text, an IME commit included.
    Text { text: String },
    /// An IME composition in progress.
    Ime {
        text: String,
        #[serde(rename = "selectionStart")]
        selection_start: u32,
        #[serde(rename = "selectionEnd")]
        selection_end: u32,
    },
    Touch {
        action: TouchAction,
        points: Vec<TouchPoint>,
        #[serde(default)]
        modifiers: u32,
    },
}

fn left() -> Button {
    Button::Left
}

fn one() -> u32 {
    1
}

/// Why an input event was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRefusal {
    TextTooLong,
    TooManyTouchPoints,
    NotFinite,
}

impl InputRefusal {
    pub fn message(self) -> &'static str {
        match self {
            Self::TextTooLong => "input text is too long",
            Self::TooManyTouchPoints => "too many touch points",
            Self::NotFinite => "coordinates must be finite numbers",
        }
    }
}

fn finite(values: &[f64]) -> Result<(), InputRefusal> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(InputRefusal::NotFinite)
    }
}

fn short(text: &str) -> Result<(), InputRefusal> {
    if text.len() <= MAX_TEXT {
        Ok(())
    } else {
        Err(InputRefusal::TextTooLong)
    }
}

impl PaneInput {
    /// The one CDP call this event becomes.
    pub fn to_cdp(&self) -> Result<(CdpMethod, Value), InputRefusal> {
        Ok(match self {
            Self::Pointer {
                action,
                x,
                y,
                button,
                buttons,
                click_count,
                modifiers,
            } => {
                finite(&[*x, *y])?;
                let kind = match action {
                    PointerAction::Down => "mousePressed",
                    PointerAction::Up => "mouseReleased",
                    PointerAction::Move => "mouseMoved",
                };
                let button = match button {
                    Button::None => "none",
                    Button::Left => "left",
                    Button::Middle => "middle",
                    Button::Right => "right",
                };
                (
                    CdpMethod::InputDispatchMouseEvent,
                    json!({
                        "type": kind, "x": x, "y": y, "button": button, "buttons": buttons,
                        "clickCount": click_count, "modifiers": modifiers,
                    }),
                )
            }
            Self::Wheel {
                x,
                y,
                delta_x,
                delta_y,
                modifiers,
            } => {
                finite(&[*x, *y, *delta_x, *delta_y])?;
                (
                    CdpMethod::InputDispatchMouseEvent,
                    json!({
                        "type": "mouseWheel", "x": x, "y": y, "deltaX": delta_x, "deltaY": delta_y,
                        "modifiers": modifiers,
                    }),
                )
            }
            Self::Key {
                action,
                key,
                code,
                text,
                key_code,
                modifiers,
            } => {
                short(key)?;
                short(code)?;
                let mut params = json!({
                    "key": key, "code": code, "windowsVirtualKeyCode": key_code,
                    "modifiers": modifiers,
                });
                match (action, text) {
                    (KeyAction::Down, Some(text)) => {
                        short(text)?;
                        params["type"] = json!("keyDown");
                        params["text"] = json!(text);
                        params["unmodifiedText"] = json!(text);
                    }
                    (KeyAction::Down, None) => params["type"] = json!("rawKeyDown"),
                    (KeyAction::Up, _) => params["type"] = json!("keyUp"),
                }
                (CdpMethod::InputDispatchKeyEvent, params)
            }
            Self::Text { text } => {
                short(text)?;
                (CdpMethod::InputInsertText, json!({ "text": text }))
            }
            Self::Ime {
                text,
                selection_start,
                selection_end,
            } => {
                short(text)?;
                (
                    CdpMethod::InputImeSetComposition,
                    json!({
                        "text": text, "selectionStart": selection_start, "selectionEnd": selection_end,
                    }),
                )
            }
            Self::Touch {
                action,
                points,
                modifiers,
            } => {
                if points.len() > MAX_TOUCH_POINTS {
                    return Err(InputRefusal::TooManyTouchPoints);
                }
                for point in points {
                    finite(&[point.x, point.y])?;
                }
                let kind = match action {
                    TouchAction::Start => "touchStart",
                    TouchAction::Move => "touchMove",
                    TouchAction::End => "touchEnd",
                    TouchAction::Cancel => "touchCancel",
                };
                let points: Vec<Value> = points
                    .iter()
                    .map(|p| json!({ "x": p.x, "y": p.y, "id": p.id }))
                    .collect();
                (
                    CdpMethod::InputDispatchTouchEvent,
                    json!({ "type": kind, "touchPoints": points, "modifiers": modifiers }),
                )
            }
        })
    }
}
