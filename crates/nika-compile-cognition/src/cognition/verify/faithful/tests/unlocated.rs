// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A decision seat's weighed rejection of the whole request that nothing locates, with no run of
//! these bytes at all, holds them only at the bar such a rejection needs
//! ([`Doubt::UNLOCATED_HOLDING_PROBABILITY`]): below it the parts carry the request and the doubt
//! is stated; the probability, the bar and the outcome are recorded either way. Every other path
//! keeps its rule, each witnessed here with a rejection weighed under that bar: a run read in
//! part, a whole run, an answer with no distribution, the provider judge, a located doubt, a part
//! left open and an extra effect.

use super::*;
use nika_compile_seats::judge::Doubt;

/// Why a run read only in part decides nothing.
const PARTIAL: &str = "the trial run wrote nothing it was read for, or was read only in part: it proves no whole output";

/// What the verdict states when its parts carry a rejection weighed at 0.85 that nothing located.
const STATED: &str = "The verifier doubted that this workflow carries the request as a whole (reported at 0.85), a question that decides nothing on its own: no part of the request, judged alone, is missing and no task does anything the request does not ask, so the workflow is proposed with this doubt stated. Read it before you consent.";

/// The doubt over [`ORDERS`] and [`CANDIDATE`] of a seat weighing its rejection at `p`: the whole
/// request rejected, its three parts carried (the engine's facts settle every task: no extra
/// question), and, asked where the doubt is, nowhere.
fn unlocated_at(p: f64) -> Seated {
    Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
        ("verify-doubt", Ok("unlocated")),
    ])
    .weighing("verify-request", p)
}

/// The decision recorded for a rejection located nowhere with no run: the probability its seat
/// reported, the bar, and whether it held.
fn decision(p: f64, outcome: &str) -> Value {
    json!({"probability": p, "bar": Doubt::UNLOCATED_HOLDING_PROBABILITY, "outcome": outcome})
}

/// [`Seated`] declaring that it weighs every answer, as a decision service does: an answer it
/// reports with no distribution is a missing distribution, never an answer taken at its word.
struct Weighing(Seated);

impl DecisionSeat for Weighing {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        self.0.choose(question)
    }

    fn weighs(&self) -> bool {
        true
    }
}

/// A rejection weighed over the holding probability but under the bar, located nowhere, with no
/// run of these bytes (correct candidates of a request whose effect no run rehearses read 0.82 to
/// 0.88 so): the parts carry the request and the doubt is stated with its probability, never
/// held; the decision is recorded.
#[tokio::test]
async fn a_weighed_rejection_located_nowhere_under_its_bar_is_stated() {
    let seat = unlocated_at(0.85);
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged {
        verdict, binding, ..
    } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
    assert_eq!(seat.left(), 0);
    assert_eq!(ids(&verdict).last(), Some(&"verify-doubt"));
    assert_eq!(lists(&verdict), found(&[], &[], &[], &["unfaithful"], &[]));
    let judgment = carried(ORDERS, "verify-parts", SEAT, &binding);
    assert_eq!(verdict.judgments, [judgment]);
    assert_eq!(verdict.settled_by, Some("verify-parts"));
    assert!(verdict.settled() && !verdict.unresolved());
    assert_eq!(verdict.stated.as_deref(), Some(STATED));
    assert_eq!(verdict.unlocated, Some(decision(0.85, "stated")));
}

/// The bar decides at its own probability: 0.899 is stated, 0.900 held, each recorded with the
/// probability exactly as its seat reported it.
#[tokio::test]
async fn the_bar_holds_from_exactly_its_probability() {
    for (p, outcome) in [(0.899, "stated"), (0.9, "held")] {
        let seat = unlocated_at(p);
        let request = CompileRequest::create(ORDERS);
        let judge = Judge::<Scripted>::Seat(&seat);
        let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
        assert_eq!(seat.left(), 0, "{p}");
        assert_eq!(verdict.settled(), outcome == "stated", "{p}");
        assert_eq!(verdict.unlocated, Some(decision(p, outcome)), "{p}");
    }
}

/// Above its bar, the same rejection still holds what nothing located: contested, unresolved,
/// never READY, the decision recorded. A broken candidate (a check that always fails) read 0.99
/// so, and every confident mutant rejection of the balanced labels 0.90 or more.
#[tokio::test]
async fn a_rejection_located_nowhere_over_its_bar_still_holds() {
    for p in [0.927, 0.99] {
        let seat = unlocated_at(p);
        let request = CompileRequest::create(ORDERS);
        let judge = Judge::<Scripted>::Seat(&seat);
        let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
        assert_eq!(seat.left(), 0, "{p}");
        let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
        assert_eq!(lists(&verdict), disputed, "{p}");
        assert!(verdict.unresolved() && !verdict.settled(), "{p}");
        assert_eq!(verdict.judgments, NO_JUDGMENT, "{p}");
        assert_eq!(verdict.unlocated, Some(decision(p, "held")), "{p}");
    }
}

/// A run of these bytes read only in part is still a run of them, never an absent one: under the
/// bar, the rejection located nowhere keeps holding, with why the run could not decide it, and no
/// decision on a rejection with no run is recorded.
#[tokio::test]
async fn a_run_read_in_part_keeps_a_rejection_under_its_bar_held() {
    let seat = unlocated_at(0.85);
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let run = observed(false, true);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, Some(&run)).await;
    assert_eq!(seat.left(), 0);
    let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[PARTIAL]);
    assert_eq!(lists(&verdict), disputed);
    assert!(!verdict.settled());
    assert_eq!(verdict.unlocated, None);
}

/// A whole run of these bytes still settles the request on its own: the question over the run
/// carries it, the doubt question is never asked, and no decision is recorded.
#[tokio::test]
async fn a_whole_run_still_settles_a_rejection_under_its_bar() {
    let seat = Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
        ("verify-observed", Ok("consistent")),
    ])
    .weighing("verify-request", 0.85);
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let run = observed(true, true);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, Some(&run)).await;
    assert_eq!(seat.left(), 0);
    assert_eq!(verdict.settled_by, Some("verify-observed"));
    assert!(verdict.settled());
    assert_eq!(verdict.unlocated, None);
}

/// A seat that weighs its answers but reported no distribution for this rejection weighed
/// nothing: no bar applies, the rejection located nowhere holds as before, and nothing is
/// recorded. Weighing, it is also asked the candidate's one sink in both option orders, which a
/// program computes.
#[tokio::test]
async fn a_rejection_with_no_distribution_still_holds() {
    let seat = Weighing(Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-sink-0", Ok("computed")),
        ("verify-sink-0-r", Ok("computed")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
        ("verify-doubt", Ok("unlocated")),
    ]));
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
    assert_eq!(seat.0.left(), 0);
    let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), disputed);
    assert!(verdict.unresolved() && !verdict.settled());
    assert_eq!(verdict.unlocated, None);
}

/// The provider judge's rejection located nowhere with no run keeps holding: it weighs nothing,
/// so no bar applies and nothing is recorded.
#[tokio::test]
async fn a_provider_judges_rejection_located_nowhere_still_holds() {
    let located = Some((Locate, Choose("unlocated")));
    let judge = Scripted::new(undisputed("unfaithful", 3, located));
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    assert_eq!(judge.left(), 0);
    let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), disputed);
    assert!(verdict.unresolved() && !verdict.settled());
    assert_eq!(verdict.unlocated, None);
}

/// What a weighed rejection's doubt locates still decides under the bar: a part it names and pins
/// on a task is the defect a repair starts from, and one no task fails stays contested. Neither
/// records a decision on a rejection located nowhere.
#[tokio::test]
async fn a_located_doubt_under_the_bar_still_decides() {
    let keep = points("keep");
    let cases = [
        (
            "task-keep",
            found(
                &[(ORDER_PARTS[1], keep.as_str())],
                &[],
                &[],
                &["unfaithful"],
                &[],
            ),
        ),
        (
            "no_task",
            found(&[], &[], &[ORDER_PARTS[1]], &["unfaithful"], &[]),
        ),
    ];
    for (pointed, expected) in cases {
        let seat = Seated::new([
            ("verify-request", Ok("unfaithful")),
            ("verify-part-0", Ok("carried")),
            ("verify-part-1", Ok("carried")),
            ("verify-part-2", Ok("carried")),
            ("verify-doubt", Ok("part-1")),
            ("verify-doubt-point-1", Ok(pointed)),
        ])
        .weighing("verify-request", 0.85);
        let request = CompileRequest::create(ORDERS);
        let judge = Judge::<Scripted>::Seat(&seat);
        let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
        assert_eq!(seat.left(), 0, "{pointed}");
        assert_eq!(lists(&verdict), expected, "{pointed}");
        assert!(!verdict.settled(), "{pointed}");
        assert_eq!(verdict.unlocated, None, "{pointed}");
    }
}

/// A part left open (the seat abstained on it) keeps the doubt where it is: the judge is never
/// asked where the doubt is, the part stays unknown and the request held, and nothing is
/// recorded.
#[tokio::test]
async fn an_open_part_keeps_a_rejection_under_its_bar_held() {
    let seat = Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("none")),
        ("verify-part-2", Ok("carried")),
    ])
    .weighing("verify-request", 0.85);
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
    assert_eq!(seat.left(), 0);
    assert!(!ids(&verdict).contains(&"verify-doubt"));
    let open = found(
        &[],
        &[ORDER_PARTS[1]],
        &[ORDERS],
        &["unfaithful"],
        &[UNOBSERVED],
    );
    assert_eq!(lists(&verdict), open);
    assert_eq!(verdict.unlocated, None);
}

/// An extra effect still decides under the bar: the open task the seat names is the defect,
/// before any question of where the doubt is, and nothing is recorded.
#[tokio::test]
async fn an_extra_effect_under_the_bar_is_still_the_defect() {
    let save = "the judge points to the task save, which does something the request does not ask";
    let seat = Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
        ("verify-extra", Ok("task-save")),
    ])
    .weighing("verify-request", 0.85);
    let candidate = elsewhere();
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged { verdict, .. } = judged(ORDERS, &request, &candidate, &judge, None).await;
    assert_eq!(seat.left(), 0);
    let defect = found(&[(EXTRA_DEFECT, save)], &[], &[], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), defect);
    assert!(!verdict.settled());
    assert_eq!(verdict.unlocated, None);
}
