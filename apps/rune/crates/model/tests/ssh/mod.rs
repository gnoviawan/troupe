//! Replays the SSH store's vectors (`tools/parity/vectors/stores/ssh.ts`) against
//! `model::ssh::State`, whose methods carry the store actions' names.

use model::ssh::{EditingFile, Session, State, Transfer};
use serde_json::Value;

use crate::common::{Replay, parse, to_value};
use crate::Store;

pub struct Ssh;

fn arg(event: &Value, index: usize) -> &Value {
    &event["args"][index]
}

fn text(event: &Value, index: usize) -> Result<&str, String> {
    arg(event, index).as_str().ok_or_else(|| format!("argument {index} is not a string"))
}

fn maybe_text(event: &Value, index: usize) -> Option<&str> {
    arg(event, index).as_str()
}

impl Store for Ssh {
    type State = State;

    fn setup(_: &Value, initial: &Value) -> Result<State, String> {
        parse(initial)
    }

    fn step(state: &mut State, event: &Value, _: &mut Replay) -> Result<(Vec<model::Effect>, Option<Value>), String> {
        match event["action"].as_str().unwrap_or_default() {
            "addSession" => state.add_session(parse::<Session>(arg(event, 0))?),
            "removeSession" => state.remove_session(text(event, 0)?),
            "patchSession" => {
                let tab = text(event, 0)?;
                if let Some(index) = state.sessions.iter().position(|s| s.id == tab) {
                    let mut merged = to_value(&state.sessions[index]);
                    for (key, value) in arg(event, 1).as_object().ok_or("patch is not an object")? {
                        merged[key] = value.clone();
                    }
                    state.sessions[index] = parse(&merged)?;
                }
            }
            "setActiveTab" => state.set_active_tab(text(event, 0)?),
            "selectProfile" => state.select_profile(maybe_text(event, 0)),
            "openWorkspace" => state.open_workspace(),
            "setPendingClose" => state.set_pending_close(maybe_text(event, 0)),
            "toggleShowAddresses" => state.toggle_show_addresses(),
            "updateTransferProgress" => state.update_transfer(parse::<Transfer>(arg(event, 0))?),
            "clearCompletedTransfers" => state.clear_completed_transfers(maybe_text(event, 0)),
            "setEditingFile" => state.set_editing_file(parse::<Option<EditingFile>>(arg(event, 0))?),
            "setEditingContent" => state.set_editing_content(text(event, 0)?),
            other => return Err(format!("no ssh action {other}")),
        }
        Ok((Vec::new(), None))
    }

    fn query(_: &State, event: &Value) -> Result<Value, String> {
        Err(format!("no ssh query {}", event["action"]))
    }
}
