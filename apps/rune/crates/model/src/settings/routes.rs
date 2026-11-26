//! The HTTP routes the settings call, as the web transport sends them (`ws-transport.ts`). None
//! is ledgered: they carry no `X-Command-Id`.

use serde_json::{Value, json};

/// One settings request.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// `getAgentIdleWindow`.
    ReadIdleWindow,
    /// `setAgentIdleWindow(secs)`.
    PushIdleWindow { secs: Value },
}

/// Method, path and JSON body.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub post: bool,
    pub path: &'static str,
    pub body: Option<Value>,
}

impl Request {
    /// The `acpApi` method and its arguments, as the route recorder called it.
    #[must_use]
    pub fn api(&self) -> (&'static str, Vec<Value>) {
        match self {
            Request::ReadIdleWindow => ("getAgentIdleWindow", vec![]),
            Request::PushIdleWindow { secs } => ("setAgentIdleWindow", vec![secs.clone()]),
        }
    }

    /// The request for a recorded `acpApi` call.
    #[must_use]
    pub fn from_api(op: &str, args: &[Value]) -> Option<Request> {
        match op {
            "getAgentIdleWindow" => Some(Request::ReadIdleWindow),
            "setAgentIdleWindow" => Some(Request::PushIdleWindow {
                secs: args.first().cloned().unwrap_or(Value::Null),
            }),
            _ => None,
        }
    }

    #[must_use]
    pub fn route(&self) -> Route {
        const PATH: &str = "/host/agent-idle-window";
        match self {
            Request::ReadIdleWindow => Route {
                post: false,
                path: PATH,
                body: None,
            },
            Request::PushIdleWindow { secs } => Route {
                post: true,
                path: PATH,
                body: Some(json!({ "secs": secs })),
            },
        }
    }
}
