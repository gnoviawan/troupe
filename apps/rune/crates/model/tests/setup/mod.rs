//! Replays first run's reads against `model::setup`: the host spelling, the restart URLs (the
//! recorder's page host is jsdom's `localhost`), the onboarding decision and its landing.

use model::setup::{self, Facts};
use serde_json::{Value, json};

use crate::common::{diff, parse, to_value};

const PAGE_HOST: &str = "localhost";

pub fn replay(vector: &Value) -> Result<usize, String> {
    let steps = vector["steps"].as_array().ok_or("no steps")?;
    for (index, step) in steps.iter().enumerate() {
        let event = &step["event"];
        let args = &event["args"];
        let text = |i: usize| args[i].as_str().unwrap_or_default();
        let port = |i: usize| args[i].as_u64().map_or(0, |p| p as u16);
        let got = match event["action"].as_str().unwrap_or_default() {
            "normalizeHost" => json!(setup::normalize_host(text(0))),
            "healthUrl" => json!(setup::restart_health_url(text(0), port(1), PAGE_HOST)),
            "landingUrl" => json!(setup::restart_landing_url(text(0), port(1), args[2].as_str(), PAGE_HOST, "/")),
            "decide" => to_value(&setup::decide(&parse::<Facts>(&args[0])?)),
            "landing" => {
                let choice = &args[0];
                to_value(&setup::landing(choice["agent"]["id"].as_str(), choice["projectId"].as_str()))
            }
            other => return Err(format!("step {index}: no read {other}")),
        };
        if let Some(problem) = diff(&format!("step {index} result"), &step["result"], &got) {
            return Err(problem);
        }
    }
    Ok(steps.len())
}
