// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A human approval the person states is theirs whether they type it or pick it: an offer they
//! picked themselves whose label asks a person to approve before the write is that approval, read
//! as their typed words are (Laws 3 and 21). Live gauntlet, 2026-10-15: a job digest that read the
//! person's profile, fetched job boards and wrote a file was refused as an INVENTED GATE after each
//! of five questions where the person picked the gate, and after they typed it too, while the Rule
//! of Two required that very gate. A label a choice left to Nika took, or one that denies the
//! approval, states none.

use std::collections::BTreeMap;

use nika_compile_fidelity::fidelity::{Diagnostic, laws_answered};
use nika_compile_reader::lexicon;
use serde_json::{Value, json};

/// The request, as the person wrote it: no approval in it.
const REQUEST: &str = "Chaque matin, recupere les offres de https://remotive.com/api/remote-jobs, \
                       garde celles qui matchent mon profil.md, et ecris jobs-<date>.md dans ce \
                       dossier.";
/// The approval question the author asked and its two offers.
const KEY: &str = "approval_before_write";
const ASKED: &str =
    "Dois-je faire valider par un humain chaque run avant d'écrire jobs-<date>.md ?";
const YES: &str = "Oui, je valide chaque run (oui/non) avant l'écriture de jobs-<date>.md";
const NO: &str = "Non : pas de validation humaine";

/// A candidate reading the profile, fetching the board and writing the digest, the write gated on
/// a `nika:prompt` when `gated`.
fn candidate(gated: bool) -> Value {
    let mut tasks = json!({
        "profile": {"invoke": {"tool": "nika:read", "args": {"path": "./profil.md"}}},
        "jobs": {"invoke": {"tool": "nika:fetch",
            "args": {"url": "https://remotive.com/api/remote-jobs", "method": "GET"}}},
        "write_jobs": {
            "with": {"jobs": "${{ tasks.jobs.output }}", "profile": "${{ tasks.profile.output }}"},
            "invoke": {"tool": "nika:write",
                "args": {"path": "./jobs-${{ with.day }}.md", "content": "${{ with.jobs }}"}}},
    });
    if gated {
        // No `default:`: the prompt blocks for the human, the only gate Law 3 affirms.
        tasks["review"] = json!({"invoke": {"tool": "nika:prompt",
            "args": {"message": "Écrire jobs-<date>.md ?"}}});
        tasks["write_jobs"]["with"]["approved"] = json!("${{ tasks.review.output }}");
        tasks["write_jobs"]["when"] = json!("${{ with.approved == true }}");
    }
    json!({"nika": "jobs", "tasks": tasks, "permits": {"fs": {"read": ["./profil.md"],
        "write": ["./jobs-*.md"]}, "net": {"http": ["remotive.com"]},
        "tools": ["nika:read", "nika:fetch", "nika:write", "nika:prompt"]}})
}

/// The Session's description of the person's answer `u2` to the approval question: the offer
/// labelled `label`, taken as `kind` (`offered`: they picked it; `delegated`: they left the choice
/// to Nika).
fn answer(label: &str, kind: &str) -> String {
    let provenance = json!({"kind": kind, "message": "u2", "question": KEY, "option": "picked"});
    json!({
        "bound_values": [],
        "bindings": [],
        "choice_context": {"question_key": KEY, "question": ASKED, "human_citation": "u2",
            "selected_options": [{"key": "picked", "label": label, "values": [],
                "provenance": provenance}],
            "human_reply": "picked"},
    })
    .to_string()
}

/// The approval findings the laws state for `intent` over `doc` with `answers`: each message's
/// first words, up to its colon.
fn gate_findings(intent: &str, doc: &Value, answers: &[String]) -> Vec<String> {
    let answers: BTreeMap<String, String> = (answers.iter())
        .map(|raw| (KEY.to_owned(), raw.clone()))
        .collect();
    let plan = lexicon::read(intent).plan;
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_answered(intent, &plan, doc, (&answers, &[]), None, &mut out);
    (out.iter())
        .filter(|d| d.kind == "gate")
        .filter_map(|d| d.message.split(':').next().map(str::to_owned))
        .collect()
}

/// Unanswered, the gate is invented. The person's own pick of the approval makes it theirs: the
/// gated candidate stands, and the ungated one now owes the approval.
#[test]
fn a_picked_approval_is_the_persons_gate() {
    let (gated, ungated) = (candidate(true), candidate(false));
    assert_eq!(gate_findings(REQUEST, &gated, &[]), ["INVENTED GATE"]);
    assert_eq!(gate_findings(REQUEST, &ungated, &[]), [""; 0]);
    let picked = [answer(YES, "offered")];
    assert_eq!(gate_findings(REQUEST, &gated, &picked), [""; 0]);
    assert_eq!(
        gate_findings(REQUEST, &ungated, &picked),
        ["MISSING APPROVAL"]
    );
}

/// A label a choice left to Nika took, or one that denies the approval, states no approval: the
/// gate stays invented.
#[test]
fn a_delegated_or_denying_label_states_no_approval() {
    let gated = candidate(true);
    for picked in [answer(YES, "delegated"), answer(NO, "offered")] {
        assert_eq!(
            gate_findings(REQUEST, &gated, std::slice::from_ref(&picked)),
            ["INVENTED GATE"],
            "{picked}"
        );
    }
}

/// The same approval typed by the person, in the words the gauntlet's founder used, is read as
/// the gate it asks for.
#[test]
fn a_typed_approval_before_the_write_is_the_persons_gate() {
    let gated = candidate(true);
    let typed =
        format!("{REQUEST}\noui, je veux une validation humaine avant d'ecrire jobs-<date>.md");
    assert_eq!(gate_findings(&typed, &gated, &[]), [""; 0]);
}

/// The Session's description of a free reply `reply` to an approval question `asked`, no option
/// offered or picked: what the conversation keeps of an answer that bound nothing.
fn replied(asked: &str, reply: &str) -> String {
    json!({"choice_context": {"question_key": KEY, "question": asked, "human_citation": "u2",
        "selected_options": [], "human_reply": reply}})
    .to_string()
}

/// A free yes to a question that asked for the person's approval states that approval, as a
/// picked offer does: live gauntlet q02, the same consent was asked six times, each « Oui »
/// leaving the gate invented. A no, a qualified no or a question that asks no approval states
/// none.
#[test]
fn a_free_yes_to_an_approval_question_is_the_persons_gate() {
    let gated = candidate(true);
    for (asked, reply) in [
        (ASKED, "Oui"),
        (ASKED, "oui stp"),
        (
            "Veux-tu valider chaque run avant que j'écrive jobs-<date>.md ?",
            "ok",
        ),
        (
            "Souhaites-tu valider avant l'écriture de jobs-<date>.md ?",
            "d’accord",
        ),
        // The approval named, its effect left to the workflow.
        ("Veux-tu valider chaque run avant qu'il parte ?", "Oui"),
    ] {
        assert_eq!(
            gate_findings(REQUEST, &gated, &[replied(asked, reply)]),
            [""; 0],
            "{asked} · {reply}"
        );
    }
    for (asked, reply) in [
        (ASKED, "Non"),
        (ASKED, "oui, sans validation"),
        ("Quel dossier pour jobs-<date>.md ?", "oui"),
    ] {
        assert_eq!(
            gate_findings(REQUEST, &gated, &[replied(asked, reply)]),
            ["INVENTED GATE"],
            "{asked} · {reply}"
        );
    }
}
