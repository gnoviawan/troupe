use super::*;

#[derive(Default)]
struct RecordingIndicator {
    calls: Mutex<Vec<String>>,
}

impl ComuScreenIndicator for RecordingIndicator {
    fn show(&self, label: &str) {
        self.calls.lock().push(format!("show:{label}"));
    }
    fn hide(&self) {
        self.calls.lock().push("hide".to_string());
    }
    fn window_ids(&self) -> Vec<i64> {
        vec![7001]
    }
}

fn state_with_port() -> (IndicatorState, Arc<RecordingIndicator>) {
    let state = IndicatorState::default();
    let port = Arc::new(RecordingIndicator::default());
    state.install(port.clone());
    (state, port)
}

#[test]
fn label_names_one_agent_and_counts_several() {
    assert_eq!(indicator_label(&[]), None);
    assert_eq!(
        indicator_label(&["Claude".to_string()]).as_deref(),
        Some("Claude is using this computer")
    );
    assert_eq!(
        indicator_label(&["Claude".to_string(), "Codex".to_string()]).as_deref(),
        Some("2 agents are using this computer")
    );
}

#[test]
fn shows_while_any_session_drives_and_hides_after_the_last() {
    let (state, port) = state_with_port();
    state.session_driving("s1", "Claude", true);
    state.session_driving("s2", "Codex", true);
    state.session_released("s1");
    state.session_released("s2");
    // Releasing an unknown session changes nothing.
    state.session_released("s3");
    assert_eq!(
        *port.calls.lock(),
        vec![
            "show:Claude is using this computer",
            "show:2 agents are using this computer",
            "show:Codex is using this computer",
            "hide",
        ]
    );
}

#[test]
fn disabled_policy_hides_and_never_shows() {
    let (state, port) = state_with_port();
    state.session_driving("s1", "Claude", true);
    state.session_driving("s1", "Claude", false);
    state.session_driving("s1", "Claude", false);
    assert_eq!(
        *port.calls.lock(),
        vec!["show:Claude is using this computer", "hide"]
    );
}

#[test]
fn without_a_port_nothing_happens_and_no_ids_are_reported() {
    let state = IndicatorState::default();
    state.session_driving("s1", "Claude", true);
    state.session_released("s1");
    assert!(state.window_ids().is_empty());

    let (state, _port) = state_with_port();
    assert_eq!(state.window_ids(), vec![7001]);
}
