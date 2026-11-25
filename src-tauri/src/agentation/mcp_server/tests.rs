use super::*;

fn test_server() -> AgentationMcpServer {
    AgentationMcpServer::new(Arc::new(SqliteStore::open_in_memory().unwrap()))
}

async fn setup_with_annotation() -> (AgentationMcpServer, String, String) {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let session = store.create_session("https://test.com", None);
    let ann = store
        .add_annotation(
            &session.id,
            &AnnotationInput {
                x: 10.0,
                y: 20.0,
                comment: "Fix button".to_string(),
                element: "button".to_string(),
                element_path: "body > button".to_string(),
                timestamp: 12345,
                ..Default::default()
            },
        )
        .unwrap();
    let server = AgentationMcpServer::new(store);
    (server, session.id, ann.id)
}

#[tokio::test]
async fn test_list_sessions() {
    let server = test_server();
    let result = server.list_sessions().await;
    assert!(result.contains("sessions"));
}

#[tokio::test]
async fn test_get_session() {
    let (server, sid, _) = setup_with_annotation().await;
    let result = server
        .get_session(Parameters(GetSessionInput {
            session_id: sid.clone(),
        }))
        .await;
    assert!(result.contains(&sid));
}

#[tokio::test]
async fn test_get_pending() {
    let (server, sid, _) = setup_with_annotation().await;
    let result = server
        .get_pending(Parameters(GetPendingInput { session_id: sid }))
        .await;
    assert!(result.contains("count"));
    assert!(result.contains("1"));
}

#[tokio::test]
async fn test_get_all_pending() {
    let (server, _, _) = setup_with_annotation().await;
    let result = server.get_all_pending().await;
    assert!(result.contains("count"));
    assert!(result.contains("1"));
}

#[tokio::test]
async fn test_acknowledge() {
    let (server, _, aid) = setup_with_annotation().await;
    let result = server
        .acknowledge(Parameters(AcknowledgeInput { annotation_id: aid }))
        .await;
    assert!(result.contains("acknowledged"));
    assert!(result.contains("true"));
}

#[tokio::test]
async fn test_resolve() {
    let (server, _, aid) = setup_with_annotation().await;
    let result = server
        .resolve(Parameters(ResolveInput {
            annotation_id: aid,
            summary: Some("Fixed the button color".to_string()),
        }))
        .await;
    assert!(result.contains("resolved"));
    assert!(result.contains("true"));
}

#[tokio::test]
async fn test_dismiss() {
    let (server, _, aid) = setup_with_annotation().await;
    let result = server
        .dismiss(Parameters(DismissInput {
            annotation_id: aid,
            reason: "Not a real issue".to_string(),
        }))
        .await;
    assert!(result.contains("dismissed"));
    assert!(result.contains("true"));
}

#[tokio::test]
async fn test_reply() {
    let (server, _, aid) = setup_with_annotation().await;
    let result = server
        .reply(Parameters(ReplyInput {
            annotation_id: aid,
            message: "Looking into this".to_string(),
        }))
        .await;
    assert!(result.contains("replied"));
    assert!(result.contains("true"));
}

#[tokio::test]
async fn test_watch_annotations_drain() {
    let (server, sid, _) = setup_with_annotation().await;
    let result = server
        .watch_annotations(Parameters(WatchAnnotationsInput {
            session_id: Some(sid),
            batch_window_seconds: Some(1),
            timeout_seconds: Some(2),
        }))
        .await;
    // Should drain pending immediately
    assert!(result.contains("count"));
    assert!(result.contains("1"));
}

#[tokio::test]
async fn test_get_ruler_guides_filters_measurements_and_guides() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let session = store.create_session("http://localhost:5173/settings", None);
    // Plain feedback annotation should not appear in ruler guides
    store
        .add_annotation(
            &session.id,
            &AnnotationInput {
                x: 10.0,
                y: 20.0,
                comment: "Plain feedback".to_string(),
                element: "button".to_string(),
                element_path: "body > button".to_string(),
                timestamp: 100,
                ..Default::default()
            },
        )
        .unwrap();
    // Alignment guide annotation with axis + offset delta + crop path
    store
        .add_annotation(
            &session.id,
            &AnnotationInput {
                x: 32.0,
                y: 180.0,
                comment: "Align card left edge at x = 32px".to_string(),
                element: "div.project-url-card".to_string(),
                element_path: "main > div.project-url-card".to_string(),
                timestamp: 200,
                kind: Some(AnnotationKind::Guide),
                measurement: Some(RulerMeasurement {
                    axis: Some(GuideAxis::Vertical),
                    position_px: Some(32.0),
                    from_selector: Some("h1.workspace-title".to_string()),
                    to_selector: Some("div.project-url-card".to_string()),
                    gap_px: None,
                    delta_px: Some(8.0),
                }),
                screenshot_crop_path: Some("/tmp/crop-2.png".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

    store
        .add_annotation(
            &session.id,
            &AnnotationInput {
                x: 48.0,
                y: 220.0,
                comment: "Increase gap to 24px".to_string(),
                element: "section.cards".to_string(),
                element_path: "main > section.cards".to_string(),
                timestamp: 250,
                kind: Some(AnnotationKind::Measurement),
                measurement: Some(RulerMeasurement {
                    axis: Some(GuideAxis::Horizontal),
                    position_px: Some(220.0),
                    from_selector: Some("h1.workspace-title".to_string()),
                    to_selector: Some("section.cards".to_string()),
                    gap_px: Some(12.0),
                    delta_px: None,
                }),
                ..Default::default()
            },
        )
        .unwrap();

    // Closed session guide should be excluded when session_id is omitted
    let closed_session = store.create_session("http://localhost:5173/old", None);
    store
        .add_annotation(
            &closed_session.id,
            &AnnotationInput {
                x: 10.0,
                y: 10.0,
                comment: "Old guide".to_string(),
                element: "header".to_string(),
                element_path: "header".to_string(),
                timestamp: 300,
                kind: Some(AnnotationKind::Guide),
                ..Default::default()
            },
        )
        .unwrap();
    store.update_session_status(&closed_session.id, SessionStatus::Closed);

    let server = AgentationMcpServer::new(store);
    let raw = server
        .get_ruler_guides(Parameters(GetRulerGuidesInput {
            session_id: Some(session.id),
        }))
        .await;
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(parsed["count"], 2);
    assert_eq!(parsed["guides"][0]["kind"], "guide");
    assert_eq!(parsed["guides"][0]["measurement"]["axis"], "vertical");
    assert_eq!(parsed["guides"][0]["measurement"]["positionPx"], 32.0);
    assert_eq!(parsed["guides"][0]["measurement"]["deltaPx"], 8.0);
    assert_eq!(parsed["guides"][0]["screenshotCropPath"], "/tmp/crop-2.png");
    assert_eq!(parsed["guides"][1]["kind"], "measurement");
    assert_eq!(parsed["guides"][1]["measurement"]["gapPx"], 12.0);

    // Unscoped lookup returns active session items only (excludes closed session)
    let unscoped_raw = server
        .get_ruler_guides(Parameters(GetRulerGuidesInput { session_id: None }))
        .await;
    let unscoped: serde_json::Value = serde_json::from_str(&unscoped_raw).unwrap();
    assert_eq!(unscoped["count"], 2);
}
