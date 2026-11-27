use serde_json::Value;

use super::*;

const SPACING: Spacing = Spacing {
    node: 40.0,
    rank: 64.0,
};
const CARD: (f64, f64) = (192.0, 64.0);
const SLOTS: f64 = 56.0;

/// The automation fixture both the host and the web client check
/// (`src/shared/types/__fixtures__/automation.json`): six steps behind the trigger, three
/// cards with saved positions (the trigger, `fetch` and `extract`) and four without.
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../../../src/shared/types/__fixtures__/automation.json"
    ))
    .expect("automation.json")
}

struct Drawn {
    ids: Vec<String>,
    heights: Vec<f64>,
    links: Vec<(String, String)>,
    stored: Vec<Option<Pos>>,
}

fn drawn(automation: &Value) -> Drawn {
    let graph = &automation["graph"];
    let mut ids = vec!["trigger".to_owned()];
    let mut heights = vec![CARD.1];
    for node in graph["nodes"].as_array().expect("nodes") {
        ids.push(node["id"].as_str().expect("id").to_owned());
        let slots = matches!(node["spec"]["kind"].as_str(), Some("agent" | "mcp_tool"));
        heights.push(CARD.1 + if slots { SLOTS } else { 0.0 });
    }
    let links = graph["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|e| {
            (
                e["from"].as_str().unwrap_or_default().to_owned(),
                e["to"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    let stored = ids
        .iter()
        .map(|id| {
            let at = &automation["layout"][id];
            at.is_object().then(|| Pos {
                x: at["x"].as_f64().expect("x"),
                y: at["y"].as_f64().expect("y"),
            })
        })
        .collect();
    Drawn {
        ids,
        heights,
        links,
        stored,
    }
}

fn run(d: &Drawn, stored: &[Option<Pos>]) -> Vec<Pos> {
    let nodes: Vec<Node> = d
        .ids
        .iter()
        .zip(&d.heights)
        .map(|(id, &height)| Node {
            id,
            width: CARD.0,
            height,
        })
        .collect();
    let links: Vec<Link> = d.links.iter().map(|(from, to)| Link { from, to }).collect();
    place(&nodes, &links, stored, SPACING)
}

fn at(d: &Drawn, placed: &[Pos], id: &str) -> Pos {
    placed[d.ids.iter().position(|i| i == id).expect("id")]
}

#[test]
fn saved_positions_are_kept_exactly() {
    let d = drawn(&fixture()["automation"]);
    let placed = run(&d, &d.stored);
    assert_eq!(at(&d, &placed, "trigger"), Pos { x: 0.0, y: 40.0 });
    assert_eq!(at(&d, &placed, "fetch"), Pos { x: 240.0, y: 40.5 });
    assert_eq!(at(&d, &placed, "extract"), Pos { x: 480.0, y: -16.0 });
    for (i, saved) in d.stored.iter().enumerate() {
        if let Some(saved) = saved {
            assert_eq!(placed[i], *saved, "{} moved", d.ids[i]);
        }
    }
}

#[test]
fn a_fully_saved_graph_is_drawn_where_it_was_saved() {
    let d = drawn(&fixture()["automation"]);
    let stored: Vec<Option<Pos>> = (0..d.ids.len())
        .map(|i| {
            Some(Pos {
                x: i as f64 * 13.25 - 7.0,
                y: -(i as f64) * 3.5,
            })
        })
        .collect();
    let placed = run(&d, &stored);
    assert_eq!(placed, stored.into_iter().flatten().collect::<Vec<_>>());
}

#[test]
fn an_unsaved_step_keeps_its_laid_out_offset_from_its_saved_parent() {
    let d = drawn(&fixture()["automation"]);
    let placed = run(&d, &d.stored);
    let auto = run(&d, &vec![None; d.ids.len()]);
    // `check` follows `extract`, which is saved; `alert` follows `fetch`, also saved.
    for (child, parent) in [("check", "extract"), ("alert", "fetch")] {
        let (c, p) = (at(&d, &placed, child), at(&d, &placed, parent));
        let (ac, ap) = (at(&d, &auto, child), at(&d, &auto, parent));
        assert_eq!(
            (c.x - p.x, c.y - p.y),
            (ac.x - ap.x, ac.y - ap.y),
            "{child} beside {parent}"
        );
    }
    // `shape` hangs off `check`, which was itself unsaved: it resolves after it.
    let (s, c) = (at(&d, &placed, "shape"), at(&d, &placed, "check"));
    let (as_, ac) = (at(&d, &auto, "shape"), at(&d, &auto, "check"));
    assert_eq!((s.x - c.x, s.y - c.y), (as_.x - ac.x, as_.y - ac.y));
}

#[test]
fn an_unconnected_step_hangs_off_the_first_saved_card() {
    let ids = ["trigger", "a", "lonely"];
    let nodes: Vec<Node> = ids
        .iter()
        .map(|id| Node {
            id,
            width: 192.0,
            height: 64.0,
        })
        .collect();
    let links = [Link {
        from: "trigger",
        to: "a",
    }];
    let stored = [
        Some(Pos { x: 100.0, y: 100.0 }),
        Some(Pos { x: 400.0, y: 90.0 }),
        None,
    ];
    let placed = place(&nodes, &links, &stored, SPACING);
    let auto = auto_place(&nodes, &links, SPACING);
    assert_eq!(placed[2].x - 100.0, auto[2].x - auto[0].x);
    assert_eq!(placed[2].y - 100.0, auto[2].y - auto[0].y);
}

#[test]
fn ranks_run_left_to_right_one_card_and_one_gap_apart() {
    let d = drawn(&fixture()["automation"]);
    let auto = run(&d, &vec![None; d.ids.len()]);
    let column = |id: &str| (at(&d, &auto, id).x / (CARD.0 + SPACING.rank)).round();
    assert_eq!(
        [
            "trigger", "fetch", "extract", "alert", "check", "shape", "post"
        ]
        .map(column),
        [0.0, 1.0, 2.0, 2.0, 3.0, 4.0, 5.0]
    );
    for p in &auto {
        assert_eq!(p.x % (CARD.0 + SPACING.rank), 0.0, "{p:?} is on a column");
    }
}

#[test]
fn cards_in_one_rank_never_overlap() {
    let d = drawn(&fixture()["automation"]);
    let auto = run(&d, &vec![None; d.ids.len()]);
    for i in 0..auto.len() {
        for j in 0..auto.len() {
            if i == j || auto[i].x != auto[j].x || auto[i].y > auto[j].y {
                continue;
            }
            assert!(
                auto[j].y - auto[i].y >= d.heights[i] + SPACING.node - 1e-9,
                "{} and {} overlap: {:?} {:?}",
                d.ids[i],
                d.ids[j],
                auto[i],
                auto[j]
            );
        }
    }
    let top = auto.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let left = auto.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    assert_eq!((top, left), (0.0, 0.0), "the content starts at the origin");
}

#[test]
fn a_chain_lines_up_on_one_row() {
    let ids = ["trigger", "a", "b", "c"];
    let nodes: Vec<Node> = ids
        .iter()
        .map(|id| Node {
            id,
            width: 192.0,
            height: 64.0,
        })
        .collect();
    let links: Vec<Link> = ids
        .windows(2)
        .map(|w| Link {
            from: w[0],
            to: w[1],
        })
        .collect();
    let auto = auto_place(&nodes, &links, SPACING);
    assert!(auto.iter().all(|p| p.y == 0.0), "{auto:?}");
    assert_eq!(
        auto.iter().map(|p| p.x).collect::<Vec<_>>(),
        [0.0, 256.0, 512.0, 768.0]
    );
}

#[test]
fn cycles_self_edges_and_missing_ends_still_lay_out() {
    let ids = ["trigger", "a", "b"];
    let nodes: Vec<Node> = ids
        .iter()
        .map(|id| Node {
            id,
            width: 192.0,
            height: 64.0,
        })
        .collect();
    let links = [
        Link {
            from: "trigger",
            to: "a",
        },
        Link { from: "a", to: "b" },
        Link { from: "b", to: "a" },
        Link { from: "b", to: "b" },
        Link {
            from: "a",
            to: "gone",
        },
    ];
    let auto = auto_place(&nodes, &links, SPACING);
    assert_eq!(auto.len(), 3);
    assert!(auto.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    assert!(auto[0].x < auto[1].x && auto[1].x < auto[2].x, "{auto:?}");
}

#[test]
fn a_fit_keeps_order_and_gaps_closest_to_the_wish() {
    assert_eq!(fit_with_gaps(&[0.0, 0.0], &[10.0]), vec![-5.0, 5.0]);
    assert_eq!(
        fit_with_gaps(&[-100.0, 100.0], &[10.0]),
        vec![-100.0, 100.0]
    );
    // Shifted targets 5, -2, -4 pool to their mean, -1/3.
    let fit = fit_with_gaps(&[5.0, 0.0, 0.0], &[2.0, 2.0]);
    let want = [-1.0 / 3.0, 5.0 / 3.0, 11.0 / 3.0];
    assert!(
        fit.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-12),
        "{fit:?}"
    );
}
