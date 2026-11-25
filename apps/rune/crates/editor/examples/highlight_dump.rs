//! Prints the style of every char of each file named on the command line, as the editor tags
//! it (a trailing `.txt` is ignored for the language): `{"<path>": ["keyword", "", ...]}`, one entry per char, `""` for plain text and line
//! breaks. `tools/parity/editor-highlight/parity.ts` compares it with CodeMirror's.

use troupe_editor::highlight::whole;

fn main() {
    let mut out = Vec::new();
    for path in std::env::args().skip(1) {
        let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        // `sample.rs.txt` is a Rust sample.
        let lines = whole(path.trim_end_matches(".txt"), &source);
        let mut tags = Vec::new();
        for (spans, line) in lines.iter().zip(source.split('\n')) {
            let mut per_char = vec![""; line.chars().count()];
            for (range, tag) in spans {
                let from = line[..range.start].chars().count();
                let to = line[..range.end].chars().count();
                for slot in &mut per_char[from..to] {
                    *slot = tag.name();
                }
            }
            tags.extend(per_char.iter().map(|t| format!("\"{t}\"")));
            tags.push("\"\"".to_owned());
        }
        tags.pop();
        out.push(format!("\"{path}\": [{}]", tags.join(",")));
    }
    println!("{{{}}}", out.join(","));
}
