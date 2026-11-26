//! The host routes the web client's `webAutomationTransport` sends (`lib/automation-api.ts`).
//! None of them is ledgered, so no call carries `X-Command-Id`.

use serde::Serialize;
use serde_json::{Value, json};

use super::types::{Layout, Op};

const AUTOMATIONS: &str = "/api/automations";
const RUNS: &str = "/api/automation-runs";
const SECRETS: &str = "/api/automation-secrets";

/// One call of the automations facade.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    List {
        project_id: String,
    },
    Get {
        id: String,
    },
    Create {
        project_id: String,
        name: String,
    },
    ApplyOps {
        id: String,
        base_revision: u64,
        ops: Vec<Op>,
    },
    PreviewOps {
        id: String,
        base_revision: u64,
        ops: Vec<Op>,
    },
    SetLayout {
        id: String,
        layout: Layout,
    },
    Remove {
        id: String,
    },
    SetEnabled {
        id: String,
        enabled: bool,
    },
    Approve {
        id: String,
        revision: u64,
    },
    RunNow {
        id: String,
    },
    CancelRun {
        run_id: String,
    },
    DecideApproval {
        run_id: String,
        node_id: String,
        approved: bool,
    },
    ListRuns {
        id: String,
        limit: u32,
    },
    RunSteps {
        run_id: String,
    },
    MarkSeen {
        id: String,
    },
    ListAgents,
    ListSecrets {
        project_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Method {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
    #[serde(rename = "PUT")]
    Put,
    #[serde(rename = "DELETE")]
    Delete,
}

/// A request as the web client sends it: method, path with query, JSON body.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Route {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
}

/// `encodeURIComponent`.
pub fn encode(segment: &str) -> String {
    let mut out = String::new();
    for b in segment.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl Call {
    pub fn route(&self) -> Route {
        let get = |path: String| Route {
            method: Method::Get,
            path,
            body: None,
        };
        let post = |path: String, body: Value| Route {
            method: Method::Post,
            path,
            body: Some(body),
        };
        let one = |id: &str| format!("{AUTOMATIONS}/{}", encode(id));
        match self {
            Call::List { project_id } => {
                get(format!("{AUTOMATIONS}?project_id={}", encode(project_id)))
            }
            Call::Get { id } => get(one(id)),
            Call::Create { project_id, name } => post(
                AUTOMATIONS.to_owned(),
                json!({ "project_id": project_id, "name": name }),
            ),
            Call::ApplyOps {
                id,
                base_revision,
                ops,
            } => post(
                format!("{}/ops", one(id)),
                json!({ "base_revision": base_revision, "ops": ops }),
            ),
            Call::PreviewOps {
                id,
                base_revision,
                ops,
            } => post(
                format!("{}/ops/preview", one(id)),
                json!({ "base_revision": base_revision, "ops": ops }),
            ),
            Call::SetLayout { id, layout } => {
                post(format!("{}/layout", one(id)), json!({ "layout": layout }))
            }
            Call::Remove { id } => Route {
                method: Method::Delete,
                path: one(id),
                body: None,
            },
            Call::SetEnabled { id, enabled } => post(
                format!("{}/enabled", one(id)),
                json!({ "enabled": enabled }),
            ),
            Call::Approve { id, revision } => post(
                format!("{}/approve", one(id)),
                json!({ "revision": revision }),
            ),
            Call::RunNow { id } => post(format!("{}/run", one(id)), json!({})),
            Call::CancelRun { run_id } => {
                post(format!("{RUNS}/{}/cancel", encode(run_id)), json!({}))
            }
            Call::DecideApproval {
                run_id,
                node_id,
                approved,
            } => post(
                format!("{RUNS}/{}/approval", encode(run_id)),
                json!({ "node_id": node_id, "approved": approved }),
            ),
            Call::ListRuns { id, limit } => get(format!("{}/runs?limit={limit}", one(id))),
            Call::RunSteps { run_id } => get(format!("{RUNS}/{}/steps", encode(run_id))),
            Call::MarkSeen { id } => post(format!("{}/seen", one(id)), json!({})),
            Call::ListAgents => get("/api/automation-agents".to_owned()),
            Call::ListSecrets { project_id } => {
                get(format!("{SECRETS}?project_id={}", encode(project_id)))
            }
        }
    }
}
