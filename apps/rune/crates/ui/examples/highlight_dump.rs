//! Prints the colour the chat's code blocks give each character of each sample, as
//! `tools/parity/highlight/parity.ts` compares them with the web client's Shiki output.
//!
//! `cargo run -p ui --example highlight_dump < samples.json > rune.json`, where the input is
//! `[{ "lang": "ts", "code": "..." }]` and the output is one hex colour per character.

use std::io::Read;

use rune_kit::syntax::{self, Paint};
use serde_json::{Value, json};

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).expect("samples on stdin");
    let samples: Vec<Value> = serde_json::from_str(&input).expect("a JSON array of samples");
    let t = theme::tokens("termul", true);
    let out: Vec<Value> = samples
        .iter()
        .map(|sample| {
            let lang = sample["lang"].as_str().unwrap_or("");
            let code = sample["code"].as_str().unwrap_or("");
            let mut colours = vec![t.colors.foreground; code.len()];
            for (range, paint) in ui::markdown::recut(code, syntax::builtin(Some(lang), code)) {
                let colour = match paint {
                    Paint::Kind(kind) => ui::markdown::syntax_colour(t, kind),
                    Paint::Color(c) => c,
                };
                for slot in &mut colours[range] {
                    *slot = colour;
                }
            }
            let per_char: Vec<String> = code.char_indices().map(|(i, _)| colours[i].hex()).collect();
            json!({ "lang": lang, "colours": per_char })
        })
        .collect();
    println!("{}", Value::Array(out));
}
