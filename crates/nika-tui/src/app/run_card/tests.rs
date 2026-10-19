// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::expect_used)]

use nika_display::run_story::RunFrame;

use super::*;

/// The English voice in the ASCII glyph column.
const ASCII: Voice = Voice {
    lang: crate::lang::Lang::En,
    ascii: true,
};

const EXEC: &str = "01a0ef11-0212-70de-a8b3-99de9427fccc";

/// A runtime event `n` of `kind` in the run, with `fields`.
fn event(n: u32, kind: &str, fields: &str) -> RunFrame {
    let line = format!(
        r#"{{"correlation":null,"execution":{{"uuid":"{EXEC}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
    );
    RunFrame::decode(&line).expect("a runtime event")
}

fn task(id: &str, extra: &str) -> String {
    format!(r#"{{"key":"task","value":"{id}"}}{extra}"#)
}

/// A digest run: `fetch` done, `pick` done on a priced model, `write` writing
/// a file, `notify` failing, `tidy` never run; settled `status`.
fn digest(status: &str) -> LiveRun {
    let mut leg = LiveRun::asked("veille-tech.nika".to_owned(), false, true, None);
    let model = r#",{"key":"model","value":"anthropic/claude-sonnet-5-5"}"#;
    let frames = [
        event(
            1,
            "workflow_started",
            r#"{"key":"workflow","value":"veille-tech"}"#,
        ),
        event(2, "task_scheduled", &task("fetch", "")),
        event(3, "task_scheduled", &task("pick", "")),
        event(4, "task_scheduled", &task("write", "")),
        event(5, "task_scheduled", &task("notify", "")),
        event(6, "task_scheduled", &task("tidy", "")),
        event(7, "task_started", &task("fetch", "")),
        event(
            8,
            "task_completed",
            &task("fetch", r#",{"key":"duration_ms","value":1200}"#),
        ),
        event(9, "task_started", &task("pick", model)),
        event(
            10,
            "task_completed",
            &task(
                "pick",
                r#",{"key":"duration_ms","value":3400},{"key":"cost_usd","value":0.034}"#,
            ),
        ),
        event(
            11,
            "task_started",
            &task("write", r#",{"key":"note","value":"invoke · nika:write"}"#),
        ),
        event(
            12,
            "task_completed",
            &task(
                "write",
                r#",{"key":"duration_ms","value":3},{"key":"output","value":"\"./out/digest.md\""}"#,
            ),
        ),
        event(13, "task_started", &task("notify", "")),
        event(
            14,
            "task_failed",
            &task(
                "notify",
                r#",{"key":"detail","value":"the webhook answered 500\nmore"}"#,
            ),
        ),
        event(15, "task_cancelled", &task("tidy", "")),
    ];
    for frame in frames {
        leg.apply(frame);
    }
    let settled = format!(
        r#"{{"kind":"run_settled","status":"{status}","cause":"normal","elapsed_ms":16540,"execution":{{"uuid":"{EXEC}"}},"evidence":"unsealed","spend":{{"total_cost_usd":0.034,"priced_calls":1,"qualifier":"priced","unpriced_calls":0}},"tasks":{{"cancelled":1,"failed":1,"never_started":1,"ok":3,"recovered":0,"skipped":0,"total":5}},"outputs":{{}}}}"#
    );
    leg.apply(RunFrame::decode(&settled).expect("a settlement"));
    leg
}

/// The card lists the steps only, each in its state's glyph with its facts (a failure with
/// why, a step never run said so, a model step with its model and its cost): no head line, no
/// totals and no files, which the Session's summary of the run says once.
#[test]
fn the_card_lists_its_steps_and_leaves_the_summary_to_the_session() {
    let card = card(&digest("failed"), Voice::default()).expect("a failed step");
    let lines: Vec<&str> = card.lines().collect();
    assert_eq!(
        lines,
        [
            "✔ fetch · 1.2s",
            "✔ pick · 3.4s · claude-sonnet-5-5 · $0.0340",
            "✔ write · 0.0s",
            "✖ notify · 0.0s · failed: the webhook answered 500",
            "⊘ tidy · not run",
        ],
        "{card}"
    );
    let ascii = card_ascii(&digest("succeeded"));
    assert!(ascii.starts_with("ok fetch · 1.2s"), "{ascii}");
    assert!(ascii.contains("\nX notify · "), "{ascii}");
    assert!(!ascii.contains("->") && !ascii.contains("Ran"), "{ascii}");
}

/// A run with no model and no failure has no card: the Session's summary says it all, and the
/// run object (F6) and its story (F2) keep its steps.
#[test]
fn a_run_with_no_model_and_no_failure_has_no_card() {
    let mut leg = LiveRun::asked("copie.nika".to_owned(), false, true, None);
    let frames = [
        event(
            1,
            "workflow_started",
            r#"{"key":"workflow","value":"copie"}"#,
        ),
        event(2, "task_started", &task("read_source", "")),
        event(
            3,
            "task_completed",
            &task("read_source", r#",{"key":"duration_ms","value":4}"#),
        ),
        event(4, "task_started", &task("write_output", "")),
        event(
            5,
            "task_completed",
            &task(
                "write_output",
                r#",{"key":"duration_ms","value":3},{"key":"output","value":"\"./out/copie.md\""}"#,
            ),
        ),
    ];
    for frame in frames {
        leg.apply(frame);
    }
    let settled = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","elapsed_ms":1300,"execution":{{"uuid":"{EXEC}"}},"evidence":"unsealed","spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"tasks":{{"cancelled":0,"failed":0,"never_started":0,"ok":2,"recovered":0,"skipped":0,"total":2}},"outputs":{{}}}}"#
    );
    leg.apply(RunFrame::decode(&settled).expect("a settlement"));
    let french = Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    assert_eq!(card(&leg, Voice::default()), None);
    assert_eq!(card(&leg, french), None);
    let paused = LiveRun::asked("copie.nika".to_owned(), false, true, None);
    assert_eq!(card(&paused, Voice::default()), None, "not settled");
}

/// The card says the shell's own words in the person's language: how the run
/// ended, each step's state, each model's calls. Ids, times, costs, model
/// names, paths and the run's own failure words keep their form.
#[test]
fn the_card_speaks_the_persons_language() {
    let french = Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    let card = card(&digest("failed"), french).expect("a failed step");
    let lines: Vec<&str> = card.lines().collect();
    assert_eq!(
        lines,
        [
            "✔ fetch · 1.2s",
            "✔ pick · 3.4s · claude-sonnet-5-5 · $0.0340",
            "✔ write · 0.0s",
            "✖ notify · 0.0s · échouée : the webhook answered 500",
            "⊘ tidy · non exécutée",
        ],
        "{card}"
    );
}

/// A failed step reads red where the transcript paints it, its words beside its glyph, never
/// colour alone, and the Session's summary of the same run gives the cause: the card says
/// which step failed and why, never how the run ended (live gauntlet 5).
#[test]
fn a_failed_step_reads_red_and_the_sessions_summary_gives_the_cause() {
    use crate::model::{Committed, Kind};
    use nika_display::theme::Role;
    let french = Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    let card = card(&digest("failed"), french).expect("a failed step");
    let block = Committed::new(Kind::Report, card);
    let lines = crate::render::block_lines(&block, true, false);
    let failed = (lines.iter())
        .find(|line| line.to_string().contains("notify"))
        .expect("the failed step's row");
    let bad = crate::visual::role::style(Role::Bad, true);
    assert!(
        (failed.spans.iter()).any(|span| span.content.starts_with("✖ notify") && span.style == bad),
        "{failed:?}"
    );
    let run = format!(r#""execution":{{"uuid":"{EXEC}"}}"#);
    let journal = [
        format!(r#"{{"kind":"workflow_started",{run},"fields":[{{"key":"workflow","value":"veille-tech"}}]}}"#),
        format!(r#"{{"kind":"task_completed",{run},"fields":[{{"key":"task","value":"fetch"}}]}}"#),
        format!(r#"{{"kind":"task_failed",{run},"fields":[{{"key":"task","value":"notify"}},{{"key":"detail","value":"the webhook answered 500"}}]}}"#),
        format!(r#"{{"kind":"workflow_failed",{run},"fields":[{{"key":"status","value":"failed"}}]}}"#),
    ]
    .join("\n");
    let facts = nika_trace::run_view::RunFacts::of(std::path::Path::new("t.ndjson"), &journal)
        .expect("the run's facts");
    let summary = facts.result_in(
        std::path::Path::new("."),
        std::path::Path::new("veille-tech.nika"),
        true,
    );
    assert!(
        summary.starts_with("Échoué · `veille-tech.nika` · `notify` a échoué"),
        "{summary}"
    );
    assert!(summary.contains("the webhook answered 500"), "{summary}");
}

/// One frame of execution `exec`: an event `n` of `kind` with `fields`, as the runner streams it.
fn frame_of(exec: &str, n: u32, kind: &str, fields: &str) -> String {
    format!(
        r#"{{"correlation":null,"execution":{{"uuid":"{exec}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#
    )
}

/// A French desk that observed `publier.nika`'s run: the leg its gate `approve` paused, then
/// the leg the person's answer resumed, the gate decided `decision` (`deny` for « non »), the
/// step it gated skipped, settled succeeded as the runtime settles it. With that leg's journal,
/// as the Session reads the run's trace.
fn gated_desk(decision: &str) -> (crate::workspace::desk::Desk, String) {
    use crate::session::feed::Observed;
    let (paused, resumed) = (
        "01a148ad-2830-70de-b8b3-99de9427fcc1",
        "01a148b1-75c0-70de-b8b3-99de9427fcc2",
    );
    let asked = |resume| Observed::Asked {
        workflow: "publier.nika".to_owned(),
        resume,
        typed: true,
        look: None,
        world: None,
    };
    let frames = |lines: &[String]| -> Vec<Observed> {
        (lines.iter())
            .map(|line| Observed::Frame(RunFrame::decode(line).expect("a frame")))
            .collect()
    };
    let (gate, name) = (
        r#"{"key":"task","value":"approve"}"#,
        r#"{"key":"workflow","value":"publier"}"#,
    );
    let unmetered = r#""spend":{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}"#;
    let first = [
        frame_of(paused, 1, "workflow_started", name),
        frame_of(paused, 2, "task_started", gate),
        frame_of(paused, 3, "workflow_paused", gate),
        format!(
            r#"{{"kind":"run_settled","status":"paused","cause":"normal","execution":{{"uuid":"{paused}"}},"evidence":"unsealed",{unmetered}}}"#
        ),
    ];
    let from = paused.replace('-', "");
    let decided = format!(
        r#"{gate},{{"key":"decision","value":"{decision}"}},{{"key":"source","value":"resume"}}"#
    );
    let journal = [
        frame_of(
            resumed,
            1,
            "workflow_started",
            &format!(r#"{name},{{"key":"resumed_from","value":"{from}"}}"#),
        ),
        // The runtime attests the answer between the gate's start and its end.
        frame_of(resumed, 2, "task_started", gate),
        frame_of(resumed, 3, "approval_decided", &decided),
        frame_of(resumed, 4, "task_completed", gate),
        frame_of(
            resumed,
            5,
            "task_skipped",
            r#"{"key":"task","value":"publish"}"#,
        ),
        frame_of(
            resumed,
            6,
            "workflow_completed",
            r#"{"key":"status","value":"succeeded"}"#,
        ),
    ];
    let settled = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","elapsed_ms":40,"execution":{{"uuid":"{resumed}"}},"evidence":"unsealed",{unmetered},"resumed_from":"{from}"}}"#
    );
    let mut desk = crate::workspace::desk::Desk::new();
    desk.view = Some(crate::model::demo_project());
    desk.lang = crate::lang::Lang::Fr;
    desk.observe(std::iter::once(asked(false)).chain(frames(&first)));
    let second: Vec<String> = journal.iter().cloned().chain([settled]).collect();
    desk.observe(std::iter::once(asked(true)).chain(frames(&second)));
    (desk, journal.join("\n"))
}

/// The rows of the whole workspace `desk` and `state` draw on a 150 by 44 screen, in Unicode
/// and without colour, its reveal over.
fn screen_of(desk: &mut crate::workspace::desk::Desk, state: &crate::model::UiState) -> String {
    let (width, height) = (150, 44);
    desk.prepare_for(state, false, false);
    let composer = crate::composer::Composer::new();
    let paint = crate::workspace::object::Paint {
        ascii: false,
        color: false,
        elapsed: crate::visual::logomark::REVEAL_ENDS,
        reduced_motion: true,
    };
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).expect("a terminal");
    terminal
        .draw(|f| {
            let drawn = crate::workspace::desk::draw(f, desk, paint, state, &composer);
            assert!(drawn, "the workspace draws");
        })
        .expect("a frame");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

/// A run the person stopped at its gate with « non », drawn whole in French (live gauntlet 5,
/// r04 B9: « succeeded », « ✔ Exécuté » and « terminé · réussi » after a « non »): its story
/// folds to its name and no card repeats the Session's summary; the runs list, the run object,
/// its gate's row and the pinned row say it stopped at the person's « non », the gate in the
/// stop glyph; nothing the TUI draws says it succeeded or ran. The summary is the Session's
/// own, from the run's facts (« Terminé · arrêté à votre « non » »); the runner line the
/// Session adds under it and its status row are the Session's to say, not drawn here.
#[test]
fn nothing_the_tui_draws_reads_a_run_stopped_at_its_gate_as_a_success() {
    use crate::model::{Committed, Kind, Presentation, UiState};
    let french = Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    let (mut desk, journal) = gated_desk("deny");
    let leg = desk.live.as_ref().expect("the resumed leg");
    assert!(leg.refused_gate(), "the « non » is observed");
    assert_eq!(card(leg, french), None, "no card repeats the summary");
    let facts = nika_trace::run_view::RunFacts::of(std::path::Path::new("t.ndjson"), &journal)
        .expect("the run's facts");
    let summary = facts.result_in(
        std::path::Path::new("."),
        std::path::Path::new("publier.nika"),
        true,
    );
    let mut state = UiState::new(Presentation::Workspace, false, (150, 44));
    state.lang = crate::lang::Lang::Fr;
    state.transcript.push(Committed::new(Kind::Human, "non"));
    let mut story = Committed::new(Kind::Run, "running · publier\n  ✔ approve\nsucceeded");
    story.run = desk.folded();
    assert!(story.run.is_some(), "the story folds");
    state.transcript.push(story);
    state.transcript.push(Committed::new(Kind::Reply, summary));
    let screen = screen_of(&mut desk, &state);
    for said in [
        "Terminé · arrêté à votre « non » · `publier.nika`",
        "run 17/10 07:04:30 · F2",
        "publier.nika · une reprise · terminé · arrêté à votre « non »",
        "⊘ approve · 0 ms · répondu non",
        "run 17/10 07:… arrêté",
        "run 17/10 07:04:30 · ⊘ terminé · arrêté à votre « non »",
    ] {
        assert!(screen.contains(said), "{said}\n{screen}");
    }
    for claim in ["succeeded", "réussi", "Exécuté", "✔ approve", "Ran in"] {
        assert!(!screen.contains(claim), "{claim} on screen:\n{screen}");
    }
    // A gate the person allowed reads as the success it is.
    let (mut desk, _) = gated_desk("allow");
    let screen = screen_of(
        &mut desk,
        &UiState::new(Presentation::Workspace, false, (150, 44)),
    );
    assert!(screen.contains("✔ approve · 0 ms"), "{screen}");
    assert!(!screen.contains("répondu non"), "{screen}");
}

/// A run that called a model and stopped at the person's « non » lists its steps: the gate in
/// the stop glyph with their answer, never done, the model step with its cost.
#[test]
fn the_card_of_a_run_stopped_at_its_gate_says_the_persons_no() {
    let mut leg = LiveRun::asked("publier.nika".to_owned(), false, true, None);
    let model = r#",{"key":"model","value":"anthropic/claude-sonnet-5-5"}"#;
    let gate = task("approve", "");
    let frames = [
        event(
            1,
            "workflow_started",
            r#"{"key":"workflow","value":"publier"}"#,
        ),
        event(2, "task_started", &task("draft", model)),
        event(
            3,
            "task_completed",
            &task(
                "draft",
                r#",{"key":"duration_ms","value":900},{"key":"cost_usd","value":0.0046}"#,
            ),
        ),
        event(4, "task_started", &gate),
        event(
            5,
            "approval_decided",
            &task(
                "approve",
                r#",{"key":"decision","value":"deny"},{"key":"source","value":"session"}"#,
            ),
        ),
        event(6, "task_completed", &gate),
        event(7, "task_skipped", &task("publish", "")),
    ];
    for frame in frames {
        leg.apply(frame);
    }
    let settled = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","elapsed_ms":1300,"execution":{{"uuid":"{EXEC}"}},"evidence":"unsealed","spend":{{"total_cost_usd":0.0046,"priced_calls":1,"qualifier":"priced","unpriced_calls":0}}}}"#
    );
    leg.apply(RunFrame::decode(&settled).expect("a settlement"));
    let french = Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    let card = card(&leg, french).expect("a model was called");
    let lines: Vec<&str> = card.lines().collect();
    assert_eq!(
        lines,
        [
            "✔ draft · 0.9s · claude-sonnet-5-5 · $0.0046",
            "⊘ approve · 0.0s · répondu non",
            "↷ publish · ignorée",
        ],
        "{card}"
    );
    let ascii = card_ascii(&leg);
    assert!(
        ascii.contains("\nx approve · 0.0s · answered no\n"),
        "{ascii}"
    );
    let ran = super::super::pulse::ran(&leg, french).expect("settled");
    assert!(
        ran.starts_with("Arrêté à votre « non » après 1.3s"),
        "{ran}"
    );
}

fn card_ascii(leg: &LiveRun) -> String {
    card(leg, ASCII).expect("a settled run")
}

/// A long run lists its first steps and every failure, then says how many
/// it did not list; nothing is cut silently.
#[test]
fn a_long_run_lists_every_failure_and_counts_the_rest() {
    let mut leg = LiveRun::asked("long.nika".to_owned(), false, true, None);
    let mut n = 1;
    let mut next = |kind: &str, fields: &str| {
        n += 1;
        event(n, kind, fields)
    };
    let mut frames = vec![next(
        "workflow_started",
        r#"{"key":"workflow","value":"long"}"#,
    )];
    for at in 0..20 {
        let id = format!("t{at}");
        frames.push(next("task_scheduled", &task(&id, "")));
        frames.push(next("task_started", &task(&id, "")));
        let kind = if at == 17 {
            "task_failed"
        } else {
            "task_completed"
        };
        frames.push(next(kind, &task(&id, "")));
    }
    for frame in frames {
        leg.apply(frame);
    }
    let settled = format!(
        r#"{{"kind":"run_settled","status":"failed","cause":"normal","elapsed_ms":900,"execution":{{"uuid":"{EXEC}"}},"evidence":"unsealed","spend":{{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}},"outputs":{{}}}}"#
    );
    leg.apply(RunFrame::decode(&settled).expect("a settlement"));
    let card = card(&leg, Voice::default()).expect("settled");
    let lines: Vec<&str> = card.lines().collect();
    let has = |head: &str| lines.iter().any(|line| line.starts_with(head));
    assert!(has("✖ t17 · ") && card.contains("failed"), "{card}");
    assert!(has("✔ t11 · "), "{card}");
    assert!(!has("✔ t12 · "), "{card}");
    assert_eq!(lines.last(), Some(&"… 7 more steps not listed"), "{card}");
    assert!(!card.contains('$'), "nothing metered, no amount: {card}");
}

/// A run that called a model lists its steps, the model step with its own cost, and no head
/// line; the live line that says how the run ended counts and costs it as its result does
/// (live gauntlet r02, r09 B12: « 1 task ran (1 from cache) · 1 skipped », never the
/// settlement's records beside them, and its spend at the result's precision, « $0.0046 »,
/// never « $0.005 »; « $0.0004 », never « <$0.001 »).
#[test]
fn a_run_that_called_a_model_lists_its_steps_with_their_cost() {
    let mut leg = LiveRun::asked("watch.nika".to_owned(), false, true, None);
    let model = r#",{"key":"model","value":"anthropic/claude-sonnet-5-5"}"#;
    let frames = [
        event(
            1,
            "workflow_started",
            r#"{"key":"workflow","value":"watch"}"#,
        ),
        event(2, "task_scheduled", &task("fetch", "")),
        event(3, "task_scheduled", &task("pick", "")),
        event(4, "task_scheduled", &task("ping", "")),
        event(5, "task_cache_hit", &task("fetch", "")),
        event(6, "task_started", &task("pick", model)),
        event(
            7,
            "task_completed",
            &task("pick", r#",{"key":"cost_usd","value":0.0046}"#),
        ),
        event(8, "task_skipped", &task("ping", "")),
    ];
    for frame in frames {
        leg.apply(frame);
    }
    let settled = format!(
        r#"{{"kind":"run_settled","status":"succeeded","cause":"normal","elapsed_ms":1700,"execution":{{"uuid":"{EXEC}"}},"evidence":"unsealed","spend":{{"total_cost_usd":0.0046,"priced_calls":1,"qualifier":"priced","unpriced_calls":0}},"tasks":{{"cancelled":0,"failed":0,"never_started":0,"ok":2,"recovered":0,"skipped":1,"total":20}},"outputs":{{}}}}"#
    );
    leg.apply(RunFrame::decode(&settled).expect("a settlement"));
    let card = card(&leg, Voice::default()).expect("a model was called");
    assert!(
        (card.lines()).any(|line| line == "✔ pick · 0.0s · claude-sonnet-5-5 · $0.0046"),
        "{card}"
    );
    assert!(
        !card.contains("Ran") && !card.contains("1 call"),
        "no head, no totals: {card}"
    );
    let ran = super::super::pulse::ran(&leg, Voice::default()).expect("settled");
    assert_eq!(
        ran, "Ran in 1.7s · 1 task ran (1 from cache) · 1 skipped · $0.0046 · 1 model call",
        "the live line"
    );
    assert!(!ran.contains("20 tasks"), "{ran}");
    assert_eq!(super::super::pulse::run_money(0.0004), "$0.0004");
}

/// The card lists the steps in the order the run executed them (live gauntlet 4, r08 B20: «
/// admit / aggregate / validate », alphabetical, while the run ran validate, then admit,
/// then aggregate), never in the frames' scheduling order.
#[test]
fn the_card_lists_the_steps_in_the_order_the_run_ran_them() {
    let mut leg = LiveRun::asked("demo.nika".to_owned(), false, true, None);
    // A frame `n` milliseconds in (the stream's stamps are nanoseconds).
    let event = |n: u32, kind: &str, fields: &str| {
        let line = format!(
            r#"{{"correlation":null,"execution":{{"uuid":"{EXEC}"}},"fields":[{fields}],"id":{{"uuid":"01a0ef11-03a7-74fb-bba0-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{}}}"#,
            u64::from(n) * 1_000_000
        );
        RunFrame::decode(&line).expect("a runtime event")
    };
    let frames = [
        event(
            1,
            "workflow_started",
            r#"{"key":"workflow","value":"demo"}"#,
        ),
        event(2, "task_scheduled", &task("admit", "")),
        event(3, "task_scheduled", &task("aggregate", "")),
        event(4, "task_scheduled", &task("validate", "")),
        event(5, "task_started", &task("validate", "")),
        event(
            6,
            "task_completed",
            &task("validate", r#",{"key":"duration_ms","value":1}"#),
        ),
        event(7, "task_started", &task("admit", "")),
        event(
            8,
            "task_completed",
            &task("admit", r#",{"key":"duration_ms","value":0}"#),
        ),
        event(9, "task_started", &task("aggregate", "")),
        event(
            10,
            "task_completed",
            &task("aggregate", r#",{"key":"duration_ms","value":2}"#),
        ),
    ];
    for frame in frames {
        leg.apply(frame);
    }
    let settled = r#"{"kind":"run_settled","status":"succeeded","cause":"normal","elapsed_ms":3,"execution":{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"},"evidence":"unsealed","spend":{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0},"tasks":{"cancelled":0,"failed":0,"never_started":0,"ok":3,"recovered":0,"skipped":0,"total":3},"outputs":{}}"#;
    leg.apply(RunFrame::decode(settled).expect("a settlement"));
    let listed = super::steps(&leg, &leg.rows_in_order(), ASCII);
    let steps: Vec<&str> = (listed.iter())
        .filter_map(|line| line.split(' ').nth(1))
        .collect();
    assert_eq!(steps, ["validate", "admit", "aggregate"], "{listed:?}");
}

/// A run a gate split is one run (live gauntlet 4, r04 B1, W16): the live line under its
/// resumed leg counts the paused leg's priced call and its cost, as the Session's summary of
/// the same run does, never « no cost » for a run that called a model before its gate.
#[test]
fn the_live_line_of_a_resumed_leg_counts_the_leg_before_its_gate() {
    let paused_exec = "01a1453e-d896-74a2-808f-b8f8ae95a870";
    let settle = |exec: &str, status: &str, spend: &str, link: &str| {
        let line = format!(
            r#"{{"kind":"run_settled","status":"{status}","cause":"normal","elapsed_ms":1300,"execution":{{"uuid":"{exec}"}},"evidence":"unsealed","spend":{spend},"tasks":{{"cancelled":0,"failed":0,"never_started":0,"ok":1,"recovered":0,"skipped":0,"total":1}},"outputs":{{}}{link}}}"#
        );
        RunFrame::decode(&line).expect("a settlement")
    };
    let mut paused = LiveRun::asked("hn.nika".to_owned(), false, true, None);
    paused.apply(settle(
        paused_exec,
        "paused",
        r#"{"priced_calls":1,"qualifier":"priced","total_cost_usd":0.043776,"unpriced_calls":0}"#,
        "",
    ));
    let mut resumed = LiveRun::asked("hn.nika".to_owned(), true, true, None);
    resumed.apply(settle(
        EXEC,
        "succeeded",
        r#"{"priced_calls":0,"qualifier":"unmetered","unpriced_calls":0}"#,
        r#","resumed_from":"01a1453ed89674a2808fb8f8ae95a870""#,
    ));
    let before = super::super::pulse::before(&resumed, std::slice::from_ref(&paused));
    assert_eq!(before.calls, 1);
    let head = super::super::pulse::ran_after(&resumed, before, ASCII).expect("settled");
    assert!(
        head.contains("$0.0438") && head.contains("1 model call"),
        "{head}"
    );
    let alone = super::super::pulse::ran(&resumed, ASCII).expect("settled");
    assert!(!alone.contains('$'), "{alone}");
}
