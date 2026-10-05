use super::*;

fn image_frame(view: &mut RunView, ms: u64, iteration: u32, path: &str) {
    let image = serde_json::json!({
        "schema": "nika/harness-image-observation@1",
        "tool_call_id": format!("image-{iteration}"),
        "reported_saved_path": path,
    })
    .to_string();
    view.apply(&ev_at(
        EventKind::AgentImageObserved,
        ms,
        &[
            ("task", s("draw")),
            ("harness_image", s(&image)),
            ("attempt", Value::Int(1)),
            ("iteration", Value::Int(i64::from(iteration))),
        ],
    ));
}

fn media(view: &RunView) -> Option<serde_json::Value> {
    view.harness_media("draw")
        .map(|raw| serde_json::from_str(raw).unwrap())
}

#[test]
fn image_frames_fold_beside_text_and_a_new_attempt_forgets_them() {
    let mut view = RunView::new();
    view.apply(&ev_at(EventKind::TaskStarted, 1, &[("task", s("draw"))]));
    image_frame(&mut view, 2, 0, "/unverified.png");
    view.apply(&ev_at(
        EventKind::TaskCompleted,
        3,
        &[
            ("task", s("draw")),
            ("output", s("\"unchanged text\"")),
            ("harness_media_count", Value::Int(1)),
        ],
    ));
    let value = media(&view).unwrap();
    assert_eq!(value["observed"], 1);
    assert_eq!(value["complete"], true);
    assert_eq!(
        value["images"][0]["image"]["reported_saved_path"],
        "/unverified.png"
    );
    assert_eq!(
        view.rows()[0].output_json.as_deref(),
        Some("\"unchanged text\"")
    );
    view.apply(&ev_at(EventKind::TaskStarted, 4, &[("task", s("draw"))]));
    assert_eq!(
        view.harness_media("draw"),
        None,
        "a new attempt starts clean"
    );
    image_frame(&mut view, 5, 0, "/second.png");
    view.apply(&ev_at(
        EventKind::TaskFailed,
        6,
        &[("task", s("draw")), ("harness_media_count", Value::Int(1))],
    ));
    assert_eq!(media(&view).unwrap()["complete"], true);
    view.apply(&ev_at(EventKind::TaskCacheHit, 7, &[("task", s("draw"))]));
    assert_eq!(
        view.harness_media("draw"),
        None,
        "no artifact invented by resume"
    );
}

#[test]
fn many_image_frames_keep_a_bounded_sample_and_detect_an_incomplete_sequence() {
    for observed in [255_u32, 256] {
        let mut view = RunView::new();
        view.apply(&ev_at(EventKind::TaskStarted, 1, &[("task", s("draw"))]));
        for iteration in 0..observed {
            image_frame(&mut view, 2, iteration, &"x".repeat(4096));
        }
        view.apply(&ev_at(
            EventKind::TaskCompleted,
            3,
            &[
                ("task", s("draw")),
                ("harness_media_count", Value::Int(256)),
            ],
        ));
        let raw = view.harness_media("draw").unwrap();
        assert!(
            raw.len() < 32 * 1024,
            "the view never retains 256 long paths"
        );
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(value["images"].as_array().unwrap().len(), 4);
        assert_eq!(value["observed"], observed);
        assert_eq!(value["expected"], 256);
        assert_eq!(value["complete"], observed == 256);
    }
}

#[test]
fn malformed_frames_or_a_missing_count_never_read_as_complete() {
    let mut view = RunView::new();
    view.apply(&ev_at(EventKind::TaskStarted, 1, &[("task", s("draw"))]));
    image_frame(&mut view, 2, 0, "/a.png");
    view.apply(&ev_at(
        EventKind::AgentImageObserved,
        3,
        &[("task", s("draw")), ("harness_image", s("not json"))],
    ));
    view.apply(&ev_at(
        EventKind::TaskCompleted,
        4,
        &[("task", s("draw")), ("harness_media_count", Value::Int(2))],
    ));
    let value = media(&view).unwrap();
    assert_eq!(value["observed"], 2, "the malformed frame still counts");
    assert_eq!(value["images"].as_array().unwrap().len(), 1);
    assert_eq!(value["complete"], false);

    let mut view = RunView::new();
    view.apply(&ev_at(EventKind::TaskStarted, 1, &[("task", s("draw"))]));
    image_frame(&mut view, 2, 0, "/a.png");
    view.apply(&ev_at(EventKind::TaskCompleted, 3, &[("task", s("draw"))]));
    assert_eq!(media(&view).unwrap()["complete"], false);
    assert_eq!(
        RunView::new().harness_media("draw"),
        None,
        "a task without image evidence has no media entry"
    );
}
