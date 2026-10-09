// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Which part a task question may call an operation no task performs (R4 A11): every part but a
//! pure prohibition (a negation with nothing else asked, read in its own language) and a
//! structure law. A negation that demands what follows (« don't forget to … ») asks an operation
//! of its own: a missing write it states is a defect a repair starts from, never a part left
//! contested.

use super::*;

/// The task question of `part` judged missing, over [`CANDIDATE`], answered `omitted`: what it
/// named and the verdict that recorded it.
async fn pointed_omitted(part: &str, judge: &Scripted) -> (Option<Pointed>, Verdict) {
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, judge);
    let base = state(part, &CompileRequest::create(part), CANDIDATE);
    let tasks = task_ids(CANDIDATE);
    let (mut verdict, mut out) = (Verdict::default(), crate::initial());
    let asked = (&base, "fixture");
    let pointed = super::super::point(
        "verify-point-0",
        part,
        &tasks,
        asked,
        &provider,
        &mut verdict,
        &mut out,
    )
    .await;
    (pointed, verdict)
}

/// A pure prohibition asks no operation of its own, so its task question never offers
/// `omitted`, nor says what it means, and a judge choosing it anyway is refused on admission: the
/// field case in French (« … ne nécessitent aucune conversion de fuseau horaire », its « ne …
/// aucune » a negation) stays unpointed, never a defect no repair can satisfy. A negation that
/// demands what follows (« Don't forget to write the summary … ») asks the write: `omitted` is
/// offered, said, and names it.
#[tokio::test]
async fn omitted_is_withheld_from_a_pure_prohibition_and_offered_to_a_demand() {
    let zone = "Les heures sont locales et ne nécessitent aucune conversion de fuseau horaire";
    let demand = "Don't forget to write the summary to ./out/s.md";
    // A structure law asks no operation; an operation stated beside it is asked.
    let (law, beside) = (
        "Nothing else.",
        "Write the total to ./out/t.txt and nothing else",
    );
    let refused = "the seat chose `omitted`, which was not offered";
    for (part, options, omittable) in [
        (zone, &PROHIBITED[..], false),
        (demand, &POINTER[..], true),
        (law, &PROHIBITED[..], false),
        (beside, &POINTER[..], true),
    ] {
        let judge = Scripted::new([(Point, Choose("omitted"))]);
        let (pointed, verdict) = pointed_omitted(part, &judge).await;
        assert_eq!(ids(&verdict), ["verify-point-0"], "{part}");
        let asked = record(&verdict, "verify-point-0");
        assert_eq!(asked["role"], "judge_point");
        assert_eq!(asked["options"], json!(options), "{part}");
        assert_eq!(asked["clause"]["text"], part);
        if omittable {
            assert!(
                matches!(&pointed, Some(Pointed::Defect(note)) if note == OMITTED),
                "{part}"
            );
            assert_eq!(asked["choice"], "omitted");
            assert_eq!(counts(&verdict), (1, 1, 1));
        } else {
            assert!(matches!(pointed, Some(Pointed::Unsettled)), "{part}");
            assert_eq!(asked["error"], refused);
            assert_eq!(counts(&verdict), (1, 1, 0));
        }
        assert!(!verdict.stopped, "{part}");
        let sent = judge.sent.lock().unwrap();
        assert_eq!(sent[0].kind, Point);
        assert!(sent[0].told.contains(POINT), "{}", sent[0].told);
        let says = sent[0].told.contains(POINT_OMITTED);
        assert_eq!(says, omittable, "{part}: {}", sent[0].told);
        drop(sent);
        assert_eq!(judge.left(), 0);
    }
    // The French prohibition restricts, as its English twin does: it is told so.
    assert!(restricts(zone) && restricts(ZONE));
}

/// A negation that demands what follows restricts nothing of its own: « Don't forget to write
/// the summary … » asks a write, as « Write the summary … » does, so its part is framed as an
/// operation (no RESTRICTING instruction telling the judge it is carried when no task does what
/// it forbids, `no_operation` offered in a request of several parts). RED: `faithful::restricts`
/// reads the demand's negation as a restriction, though `pure_prohibition` reads it as a demand.
#[test]
fn a_demand_stated_with_a_negation_restricts_nothing() {
    for part in [
        "Don't forget to write the summary to ./out/s.md",
        "No olvides escribir el resumen",
        "N'oublie pas d'écrire le résumé",
    ] {
        assert!(!restricts(part), "{part}");
    }
}
