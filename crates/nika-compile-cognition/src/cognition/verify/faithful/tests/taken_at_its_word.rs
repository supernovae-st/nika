// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A decision seat taken at its word (it does not weigh its answers and weighed none here) holds
//! only what it locates: an unlocated rejection settles by its parts, its doubt stated with the
//! proposal. A weighing seat at the holding probability still holds it; what the doubt locates
//! still holds whoever answers.

use super::*;

/// The doubt over [`ORDERS`] and [`CANDIDATE`]: the whole request rejected, its three parts
/// carried (the engine's facts settle every task: no extra question), and, asked where the doubt
/// is, `located`.
fn doubting(located: &'static str) -> Seated {
    Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
        ("verify-doubt", Ok(located)),
    ])
}

/// What a seat taken at its word states when its parts settle a rejection it never located.
const STATED: &str = "The verifier doubted that this workflow carries the request as a whole, a question that decides nothing on its own: no part of the request, judged alone, is missing and no task does anything the request does not ask, so the workflow is proposed with this doubt stated. Read it before you consent.";

/// A seat taken at its word rejects the whole request, carries every part and locates nothing:
/// its parts carry the request, the doubt stated, never held.
#[tokio::test]
async fn a_seat_taken_at_its_word_settles_an_unlocated_rejection_by_its_parts() {
    let seat = doubting("unlocated");
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
}

/// The same seat weighing its rejection at the holding probability still holds what it cannot
/// locate: contested, unresolved, never READY.
#[tokio::test]
async fn a_weighed_seat_still_holds_an_unlocated_rejection() {
    let seat = doubting("unlocated").weighing("verify-request", 0.9);
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
    let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), disputed);
    assert!(verdict.unresolved() && !verdict.settled());
    assert_eq!(verdict.judgments, NO_JUDGMENT);
}

/// What the doubt of a seat taken at its word locates still holds: a part it names that no task
/// fails stays contested, never READY.
#[tokio::test]
async fn a_seat_taken_at_its_word_still_holds_what_it_locates() {
    let seat = Seated::new([
        ("verify-request", Ok("unfaithful")),
        ("verify-part-0", Ok("carried")),
        ("verify-part-1", Ok("carried")),
        ("verify-part-2", Ok("carried")),
        ("verify-doubt", Ok("part-1")),
        ("verify-doubt-point-1", Ok("no_task")),
    ]);
    let request = CompileRequest::create(ORDERS);
    let judge = Judge::<Scripted>::Seat(&seat);
    let Judged { verdict, .. } = judged(ORDERS, &request, CANDIDATE, &judge, None).await;
    assert_eq!(seat.left(), 0);
    let contested = found(&[], &[], &[ORDER_PARTS[1]], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), contested);
    assert!(!verdict.settled());
    assert_eq!(verdict.judgments, NO_JUDGMENT);
}
