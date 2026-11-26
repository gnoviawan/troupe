//! Replays the Report a Bug dialog's reads against `model::bug_report`.

use model::bug_report::{self, Context};
use serde_json::{Value, json};

use crate::common::{diff, parse};

pub fn replay(vector: &Value) -> Result<usize, String> {
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let event = &step["event"];
        let args = &event["args"];
        let context: Context = parse(&args[0])?;
        let text = |i: usize| args[i].as_str().unwrap_or_default();
        let got = match event["action"].as_str().unwrap_or_default() {
            "markdown" => json!(bug_report::markdown(&context, text(1), text(2), args[3].as_u64().unwrap_or(0) as usize)),
            "issueUrl" => json!(bug_report::issue_url(&context, text(1), text(2), args[3].as_bool().unwrap_or(false))),
            other => return Err(format!("step {index}: no read {other}")),
        };
        if let Some(problem) = diff(&format!("step {index} result"), &step["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}
