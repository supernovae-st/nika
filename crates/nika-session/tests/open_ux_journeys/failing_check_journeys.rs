// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Bytes whose own check fails are never shown: the author reads the findings, repairs the
//! candidate, and only the repaired bytes reach the person (QA gauntlet q04).

use crate::scenarios::{CLAIMED_RUN, CLAUDE, PLAIN_ANSWER, TEMPERATURE};
use crate::{candidate, outcome_kind, run, work};

/// A temperature on a Claude 5 model fails the candidate's own check: the proposal is refused to
/// the author with the check's finding, nothing offers to save or run those bytes, and the
/// person is shown only the repaired candidate, whose check is clean.
#[test]
fn bytes_failing_their_own_check_are_never_shown() {
    let journey = run("failing_check");
    let refused = (journey.agent.iter())
        .map(crate::peer::Seen::last)
        .find(|said| said.contains("the check of these bytes fails"));
    let refused = refused.expect("the author read the check's refusal");
    assert!(refused.contains("temperature"), "{refused}");
    let shown = journey.step(0);
    assert_eq!(outcome_kind(shown), "proposal", "{shown:#}");
    let bytes = candidate(shown);
    assert!(bytes.contains(CLAUDE), "{bytes}");
    assert!(
        !bytes.contains(TEMPERATURE),
        "only the repaired bytes: {bytes}"
    );
    let preview = shown["outcome"]["text"].as_str().unwrap_or_default();
    assert!(!preview.contains("findings ✖"), "{preview}");
    assert!(work(shown)["saved"].is_null(), "{shown:#}");
    assert_eq!(
        journey.agent.len(),
        4,
        "the refusal sent the author to repair once"
    );
}

/// A turn whose proposal Nika's check refused, and that shows none, says so after the author's
/// words, whatever they claim: « C'est enregistré et lancé » reaches the person with the fact that
/// nothing was proposed, saved or run (live gauntlet wave 9 r07, 2026-10-20). A plain answer with
/// no proposal in its turn carries no such line.
#[test]
fn a_claim_after_a_refused_proposal_is_followed_by_what_the_turn_did() {
    let journey = run("claimed_after_refusal");
    let claimed = journey.step(0);
    assert_eq!(outcome_kind(claimed), "reply", "{claimed:#}");
    let text = claimed["outcome"]["text"].as_str().unwrap_or_default();
    assert!(
        text.starts_with(CLAIMED_RUN),
        "the author's words stay: {text}"
    );
    assert!(
        text.ends_with("Rien n'a été proposé, enregistré ni lancé pendant ce tour : la vérification de Nika a refusé le brouillon."),
        "the turn's fact follows them: {text}"
    );
    assert!(work(claimed)["saved"].is_null(), "{claimed:#}");
    let plain = journey.step(1);
    let words = plain["outcome"]["text"].as_str().unwrap_or_default();
    assert!(words.contains(PLAIN_ANSWER), "{plain:#}");
    assert!(
        !words.contains("Rien n'a été proposé"),
        "a turn that refused no proposal says no such fact: {words}"
    );
}
