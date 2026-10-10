// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the engine's facts settle (R4 A11): the extra-operation question offers only the tasks
//! they leave open and is never asked when none is; a write named through a constant resolves
//! as the runtime resolves it. A whole-request rejection nothing else located, with no run of
//! these bytes, is asked once where it is: a part or an open task it names is localized, and
//! nothing named leaves it unresolved, never READY.

use super::*;

/// When no part is missing, one question asks which task, if any, does something the request
/// does not ask, offered only the tasks the engine's facts leave open (here the write of a path
/// the request never names): the task named is a defect whose note names it, never contested; a
/// task the facts settle is no option, so naming one is no admitted choice; a question left
/// without a choice stays unknown in its own words, and the rejected request contested. A call
/// that got no answer stops: the extra question is unknown as unanswered (never as a choice the
/// judge did not make), the request unknown, never contested.
#[tokio::test]
async fn the_extra_question_names_an_open_task_as_a_defect_or_stays_unknown() {
    let save = "the judge points to the task save, which does something the request does not ask";
    let disputed =
        |unknown: &str| found(&[], &[unknown], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
    let cases = [
        (
            Choose("task-save"),
            found(&[(EXTRA_DEFECT, save)], &[], &[], &["unfaithful"], &[]),
            (5, 5, 5),
        ),
        (Choose("task-load"), disputed(EXTRA_UNSETTLED), (5, 5, 4)),
        (Choose("none"), disputed(EXTRA_UNSETTLED), (5, 5, 4)),
        (
            Fail,
            found(&[], &[EXTRA_UNANSWERED, ORDERS], &[], &["unfaithful"], &[]),
            (5, 4, 4),
        ),
    ];
    let candidate = elsewhere();
    for (reply, expected, calls) in cases {
        let mut script = vec![(Request, Choose("unfaithful"))];
        script.extend(repeat_n((Part, Choose("carried")), 3));
        script.push((Extra, reply));
        let judge = Scripted::new(script);
        let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
        let provider = Judge::Provider(&policy, &judge);
        let request = CompileRequest::create(ORDERS);
        let Judged { verdict, .. } = judged(ORDERS, &request, &candidate, &provider, None).await;
        assert_eq!(ids(&verdict).last(), Some(&"verify-extra"), "{reply:?}");
        let asked = record(&verdict, "verify-extra");
        assert_eq!(asked["role"], "judge_extra");
        let options = json!(["only_requested", "task-save", "none"]);
        assert_eq!(asked["options"], options, "{reply:?}");
        assert_eq!(asked.get("clause"), None);
        assert_eq!(lists(&verdict), expected, "{reply:?}");
        assert_eq!(counts(&verdict), calls, "{reply:?}");
        assert_eq!(verdict.stopped, matches!(reply, Fail), "{reply:?}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert!(verdict.engine.is_empty(), "{reply:?}");
        assert_eq!(judge.left(), 0);
    }
    let shown = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("carried")),
        (Part, Choose("carried")),
        (Part, Choose("carried")),
        (Extra, Choose("only_requested")),
        (Locate, Choose("unlocated")),
    ]);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, &shown);
    let request = CompileRequest::create(ORDERS);
    let _ = judged(ORDERS, &request, &candidate, &provider, None).await;
    let asked = shown.sent.lock().unwrap();
    let extra = (asked.iter().find(|sent| sent.kind == Extra)).expect("the extra question");
    assert!(extra.told.contains(EXTRA), "{}", extra.told);
    assert!(
        extra.told.contains("`effects` lists what each task"),
        "{}",
        extra.told
    );
    let save = &extra.state["effects"][2];
    assert_eq!(save["task"], "save");
    assert_eq!(save["writes"], json!(["./out/elsewhere.json"]));
    assert_eq!(save["settled"], Value::Null, "left open: {save}");
    assert_eq!(
        extra.state["effects"][0]["settled"]["reads"],
        json!(["./data/orders.json"])
    );
}

/// When the engine's facts settle every task (the read is the request's source, the jq program
/// has no effect, the write is the output a carried part states, a path no task reads), no extra
/// question is asked: the verdict records what settled it, never a judge's answer. A write through
/// a constant is the same write: the checker resolves `const:` as the runtime does.
#[tokio::test]
async fn the_engines_facts_settle_the_requested_write_and_ask_no_extra_question() {
    let through = CANDIDATE
        .replace("permits:", "const:\n  output: ./out/open.json\npermits:")
        .replace(
            "args: { path: \"./out/open.json\", content",
            "args: { path: \"${{ const.output }}\", content",
        );
    assert!(through.contains("${{ const.output }}"), "{through}");
    for candidate in [CANDIDATE.to_owned(), through] {
        let script = undisputed("unfaithful", 3, Some((Locate, Choose("unlocated"))));
        let judge = Scripted::new(script);
        let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
        let provider = Judge::Provider(&policy, &judge);
        let request = CompileRequest::create(ORDERS);
        let Judged { verdict, .. } = judged(ORDERS, &request, &candidate, &provider, None).await;
        let asked = [
            "verify-request",
            "verify-part-0",
            "verify-part-1",
            "verify-part-2",
            "verify-doubt",
        ];
        assert_eq!(ids(&verdict), asked, "{candidate}");
        assert_eq!(verdict.engine.len(), 1, "{candidate}");
        let settled = &verdict.engine[0];
        assert_eq!(settled["question"], "verify-extra");
        assert_eq!(settled["settled"], "only_requested");
        assert_eq!(settled["by"], "engine");
        let save = &settled["effects"][2];
        let output = json!([{"path": "./out/open.json", "part": ORDER_PARTS[2]}]);
        assert_eq!(save["settled"]["writes"], output, "{candidate}");
        let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
        assert_eq!(lists(&verdict), disputed, "{candidate}");
        assert_eq!(counts(&verdict), (5, 5, 5), "{candidate}");
        assert_eq!(judge.left(), 0);
    }
}

/// A write is judged on the path it resolves to: a write through a constant to a path the request
/// never names, or over the source a task reads, stays a task the judge may name, and named it is
/// the defect a repair starts from (one task alone, read without its workflow's `const:`, once
/// passed for a task with no effect whatever path it wrote).
#[tokio::test]
async fn a_constant_write_elsewhere_or_over_the_source_stays_open_and_named_is_a_defect() {
    let through = |path: &str| {
        CANDIDATE
            .replace("permits:", &format!("const:\n  output: {path}\npermits:"))
            .replace(
                "write: [\"./out/open.json\"]",
                &format!("write: [\"{path}\"]"),
            )
            .replace(
                "args: { path: \"./out/open.json\", content",
                "args: { path: \"${{ const.output }}\", content",
            )
    };
    let save = "the judge points to the task save, which does something the request does not ask";
    for candidate in [
        through("./out/elsewhere.json"),
        through("./data/orders.json"),
    ] {
        let mut script = vec![(Request, Choose("unfaithful"))];
        script.extend(repeat_n((Part, Choose("carried")), 3));
        script.push((Extra, Choose("task-save")));
        let judge = Scripted::new(script);
        let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
        let provider = Judge::Provider(&policy, &judge);
        let request = CompileRequest::create(ORDERS);
        let Judged { verdict, .. } = judged(ORDERS, &request, &candidate, &provider, None).await;
        let asked = record(&verdict, "verify-extra");
        let options = json!(["only_requested", "task-save", "none"]);
        assert_eq!(asked["options"], options, "{candidate}");
        let located = found(&[(EXTRA_DEFECT, save)], &[], &[], &["unfaithful"], &[]);
        assert_eq!(lists(&verdict), located, "{candidate}");
        assert!(verdict.rejected() && verdict.engine.is_empty());
        assert_eq!(judge.left(), 0);
    }
}

/// A doubt nothing located (the request unfaithful, every part carried, nothing extra, no run of
/// these bytes) is asked once where it is: shown its own answer to each part and what each task
/// touches, the judge names a part, an open task or nothing. A part named asks its task next and
/// is the defect a repair starts from, or contested when no task fails it; an open task named is
/// the defect; nothing named, or no choice, leaves the request unresolved: held, never READY, no
/// option carrying it. A call that gets no answer stops, the request unknown.
#[tokio::test]
async fn a_doubt_nothing_located_is_asked_where_it_is_once() {
    let keep = points("keep");
    let unresolved = found(&[], &[], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
    let cases = [
        (
            vec![(Locate, Choose("part-1")), (Point, Choose("task-keep"))],
            found(
                &[(ORDER_PARTS[1], keep.as_str())],
                &[],
                &[],
                &["unfaithful"],
                &[],
            ),
            (6, 6, 6),
            false,
        ),
        (
            vec![(Locate, Choose("part-1")), (Point, Choose("no_task"))],
            found(&[], &[], &[ORDER_PARTS[1]], &["unfaithful"], &[]),
            (6, 6, 6),
            false,
        ),
        (
            vec![(Locate, Choose("unlocated"))],
            unresolved.clone(),
            (5, 5, 5),
            true,
        ),
        (vec![(Locate, Choose("none"))], unresolved, (5, 5, 4), true),
        (
            vec![(Locate, Fail)],
            found(&[], &[ORDERS], &[], &["unfaithful"], &[]),
            (5, 4, 4),
            false,
        ),
    ];
    for (located, expected, calls, open) in cases {
        let mut script = undisputed("unfaithful", 3, None);
        script.extend(located.iter().copied());
        let judge = Scripted::new(script);
        let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
        let asked = record(&verdict, "verify-doubt");
        assert_eq!(asked["role"], "judge_doubt");
        let options = json!(["part-0", "part-1", "part-2", "unlocated", "none"]);
        assert_eq!(asked["options"], options, "{located:?}");
        assert_eq!(lists(&verdict), expected, "{located:?}");
        assert_eq!(counts(&verdict), calls, "{located:?}");
        assert_eq!(verdict.unresolved(), open, "{located:?}");
        assert_eq!(verdict.judgments, NO_JUDGMENT);
        assert!(!verdict.settled() && verdict.settled_by.is_none());
        assert_eq!(judge.left(), 0);
    }
    // What the question shows: its own answers, what each task touches, the request's framing.
    let judge = Scripted::new(undisputed(
        "unfaithful",
        3,
        Some((Locate, Choose("unlocated"))),
    ));
    let _ = provided(ORDERS, &judge, None).await;
    {
        let sent = judge.sent.lock().unwrap();
        let (shown, _) = sent.split_last().expect("the localizing question");
        assert_eq!(shown.kind, Locate);
        let framed = shown.told.contains(LOCATE) && shown.told.contains(CREATED);
        assert!(framed, "{}", shown.told);
        let answers: Vec<Value> = (ORDER_PARTS.iter().enumerate())
            .map(|(k, part)| json!({"part": k, "text": part, "answer": "carried"}))
            .collect();
        assert_eq!(shown.state["parts"], json!(answers));
        assert_eq!(shown.state["effects"][2]["task"], "save");
        assert_eq!(shown.state["candidate_nika"], CANDIDATE);
    }
    // An open task named is the defect it does.
    let mut script = undisputed("unfaithful", 3, Some((Extra, Choose("only_requested"))));
    script.push((Locate, Choose("task-save")));
    let judge = Scripted::new(script);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, &judge);
    let request = CompileRequest::create(ORDERS);
    let Judged { verdict, .. } = judged(ORDERS, &request, &elsewhere(), &provider, None).await;
    let options = json!([
        "part-0",
        "part-1",
        "part-2",
        "task-save",
        "unlocated",
        "none"
    ]);
    assert_eq!(record(&verdict, "verify-doubt")["options"], options);
    let save = "the judge points to the task save, which does something the request does not ask";
    let located = found(&[(EXTRA_DEFECT, save)], &[], &[], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), located);
    assert_eq!(judge.left(), 0);
}
