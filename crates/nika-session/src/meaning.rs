// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Meaning: what survived of the request, clause by clause, read from the
//! compiler's own obligation ledger (`provenance.decision.ledger`), never
//! from prose. Each clause gets a disposition — represented in the
//! program, asked, external (kept beside the program, needs a binding), a
//! gap, refused — and its assurance: WHERE it lives says what holds it (a
//! task that runs · a line in a prompt · a declared boundary · a
//! requirement outside the bytes). The view never certifies that the
//! reader read everything: a clause the compiler did not read is not here,
//! and the footer says so. A missing ledger renders « unavailable », never
//! an invented coverage.

use std::fmt::Write as _;

use nika_onboard::compile::CompileOutcome;
use nika_schema::raw::{RawAction, RawInvokeTarget};
use serde_json::Value;

/// One clause's fate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Disposition {
    /// Carried by the program (a task) or a declared boundary.
    Represented,
    /// The compiler needs the human's answer for it.
    NeedsAnswer,
    /// Kept beside the program: a binding outside the bytes fulfils it.
    External,
    /// Read, not expressible: a gap the human must know.
    Gap,
    /// Refused by a law of the compiler.
    Refused,
    /// Two demands on one unit that cannot both hold.
    Contradicted,
}

impl Disposition {
    /// The glyph the view prints (the same in plain and in the renderer).
    #[must_use]
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Represented => "✓",
            Self::NeedsAnswer => "?",
            Self::External => "↗",
            Self::Gap => "!",
            Self::Refused | Self::Contradicted => "×",
        }
    }

    /// The word the view prints beside the glyph.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Represented => "represented",
            Self::NeedsAnswer => "needs your answer",
            Self::External => "external requirement",
            Self::Gap => "not expressible",
            Self::Refused => "refused",
            Self::Contradicted => "contradicts another clause",
        }
    }
}

/// One clause as the ledger read it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Clause {
    /// The words of the request the duty came from (the compiler's evidence).
    pub evidence: String,
    /// The duty's kind in the compiler's vocabulary (effect · gate · trigger …).
    pub kind: String,
    /// Its fate.
    pub disposition: Disposition,
    /// What carries it: a task id, `requested_trigger`, a boundary — or nothing.
    pub carrier: Option<String>,
    /// The compiler's note, when it left one.
    pub note: Option<String>,
}

/// The clauses of a compile outcome, from its ledger; `None` when the
/// outcome carries no ledger.
#[must_use]
pub fn clauses(out: &CompileOutcome) -> Option<Vec<Clause>> {
    let ledger = out.provenance.decision.as_ref()?.get("ledger")?;
    Some(clauses_of(ledger))
}

/// The clauses of a ledger as the wire carries it (an array of duties:
/// `kind · state · evidence · realized_by · note`); a duty of an unknown
/// state is left out rather than guessed.
#[must_use]
pub fn clauses_of(ledger: &Value) -> Vec<Clause> {
    ledger
        .as_array()
        .map(|duties| duties.iter().filter_map(clause_of).collect())
        .unwrap_or_default()
}

fn clause_of(duty: &Value) -> Option<Clause> {
    let text = |key: &str| duty.get(key).and_then(Value::as_str).map(str::to_owned);
    let state = text("state")?;
    let carrier = text("realized_by").or_else(|| text("carrier"));
    let note = text("note");
    let external = carrier.as_deref() == Some("requested_trigger")
        || note
            .as_deref()
            .is_some_and(|n| n.contains("requires binding") || n.contains("outside the program"));
    let disposition = match state.as_str() {
        "realized" if external => Disposition::External,
        "realized" => Disposition::Represented,
        "unresolved" | "needs_human" => Disposition::NeedsAnswer,
        "unsupported" => Disposition::Gap,
        "refused" => Disposition::Refused,
        "contradicted" => Disposition::Contradicted,
        _ => return None,
    };
    Some(Clause {
        evidence: text("evidence").unwrap_or_default(),
        kind: text("kind").unwrap_or_default(),
        disposition,
        carrier,
        note,
    })
}

/// What holds a clause, said as a human reads it: the carrier's nature
/// decides the assurance (a task that runs · a boundary · a binding
/// outside the bytes · a prompt line the model is asked to honour).
#[must_use]
pub fn assurance(clause: &Clause, candidate: Option<&str>) -> &'static str {
    match clause.disposition {
        Disposition::External => {
            "kept beside the program · fulfilled by a binding, not by these bytes"
        }
        Disposition::NeedsAnswer => "waits for your answer",
        Disposition::Gap | Disposition::Refused | Disposition::Contradicted => "not carried",
        Disposition::Represented => match (clause.kind.as_str(), carrier_verb(clause, candidate)) {
            ("gate", _) | (_, Some("gate")) => "a human gate · the run pauses and asks you",
            (_, Some("infer")) => "asked of the model in its prompt · a guideline, not a check",
            (_, Some("assert")) => "checked at run before the effect",
            (_, Some(_)) => "a task that runs",
            _ => "carried by the program",
        },
    }
}

/// The verb of the task that carries a clause, from the candidate's bytes.
fn carrier_verb(clause: &Clause, candidate: Option<&str>) -> Option<&'static str> {
    let id = clause.carrier.as_deref()?;
    let wf = crate::review::parse(candidate?)?;
    let task = wf.tasks.iter().find(|t| t.value.id.value == id)?;
    Some(match &task.value.action {
        RawAction::Infer(_) => "infer",
        RawAction::Exec(_) => "exec",
        RawAction::Agent(_) => "agent",
        RawAction::Invoke(invoke) => match &invoke.target {
            RawInvokeTarget::Tool(tool) if tool.value == "nika:assert" => "assert",
            RawInvokeTarget::Tool(tool) if tool.value == "nika:prompt" => "gate",
            _ => "invoke",
        },
        _ => "other",
    })
}

/// The Meaning view of a compile outcome: one line per clause, its
/// disposition and assurance, then the honest footer. `None` when the
/// outcome carries no ledger.
#[must_use]
pub fn render(out: &CompileOutcome) -> Option<String> {
    let ledger = out.provenance.decision.as_ref()?.get("ledger")?;
    Some(render_ledger(ledger, out.candidate.as_deref()))
}

/// The Meaning view of a ledger and the candidate its carriers name.
#[must_use]
pub fn render_ledger(ledger: &Value, candidate: Option<&str>) -> String {
    let clauses = clauses_of(ledger);
    let mut text = "Meaning · your request, clause by clause".to_owned();
    if clauses.is_empty() {
        text.push_str("\n  (the compiler recorded no clause for this request)");
    }
    for clause in &clauses {
        let evidence = if clause.evidence.is_empty() {
            format!("({})", clause.kind)
        } else {
            format!("« {} »", clause.evidence)
        };
        let _ = write!(
            text,
            "\n  {} {evidence}\n      {} · {}",
            clause.disposition.glyph(),
            clause.disposition.word(),
            assurance(clause, candidate)
        );
        if let Some(carrier) = &clause.carrier
            && clause.disposition == Disposition::Represented
        {
            let _ = write!(text, " (`{carrier}`)");
        }
    }
    let open = clauses
        .iter()
        .filter(|c| c.disposition == Disposition::NeedsAnswer)
        .count();
    let external = clauses
        .iter()
        .filter(|c| c.disposition == Disposition::External)
        .count();
    let _ = write!(
        text,
        "\n  {} clause(s) the compiler read · {open} waiting for you · {external} outside the bytes",
        clauses.len()
    );
    text.push_str(
        "\n  this lists what the compiler read; a clause it did not read is not here — if something you asked is missing, say it again in its own words",
    );
    text
}

/// What a revision changed in meaning: the revised ledger against the
/// base one, clause by clause (a clause is its kind and its evidence) —
/// added, dropped, a changed fate, kept. Said in the view's own words,
/// never a score; `None` when neither ledger holds a clause.
#[must_use]
pub fn delta(base: &Value, revised: &Value) -> Option<String> {
    let before = clauses_of(base);
    let after = clauses_of(revised);
    if before.is_empty() && after.is_empty() {
        return None;
    }
    let key = |c: &Clause| (c.kind.clone(), c.evidence.clone());
    let mut text = "Meaning · what changed with your words".to_owned();
    let mut kept = 0usize;
    for clause in &after {
        match before.iter().find(|b| key(b) == key(clause)) {
            None => {
                let _ = write!(
                    text,
                    "\n  + « {} » · {}",
                    clause.evidence,
                    clause.disposition.word()
                );
            }
            Some(b) if b.disposition != clause.disposition => {
                let _ = write!(
                    text,
                    "\n  ~ « {} » · {} → {}",
                    clause.evidence,
                    b.disposition.word(),
                    clause.disposition.word()
                );
            }
            Some(_) => kept += 1,
        }
    }
    for clause in &before {
        if !after.iter().any(|a| key(a) == key(clause)) {
            let _ = write!(text, "\n  − « {} » · no longer asked", clause.evidence);
        }
    }
    let changed = after.len() + before.len() - 2 * kept;
    if changed == 0 {
        let _ = write!(
            text,
            "\n  nothing changed in what the compiler read · {kept} clause(s) kept"
        );
    } else {
        let _ = write!(text, "\n  {kept} clause(s) kept as they were");
    }
    Some(text)
}

/// The line when no ledger exists: never an invented coverage.
pub const UNAVAILABLE: &str = "Meaning · unavailable for this candidate (the compiler recorded no ledger) — the review above and `/show` are what there is";

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/tests/fixtures/compile/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    fn ledger_and_candidate(doc: &Value) -> (Value, Option<String>) {
        let ledger = doc["provenance"]["decision"]["ledger"].clone();
        let candidate = doc["candidate"].as_str().map(str::to_owned);
        (ledger, candidate)
    }

    /// A stated cadence is an external requirement beside the program; the
    /// write is a task that runs. The footer counts, never certifies.
    #[test]
    fn a_cadence_is_external_and_a_write_is_a_task_that_runs() {
        let doc = fixture("cadence-copy");
        let (ledger, candidate) = ledger_and_candidate(&doc);
        let clauses = clauses_of(&ledger);
        assert_eq!(clauses.len(), 2, "{clauses:?}");
        let write = clauses
            .iter()
            .find(|c| c.kind == "effect")
            .expect("the effect");
        assert_eq!(write.disposition, Disposition::Represented);
        assert_eq!(write.carrier.as_deref(), Some("write_output"));
        assert_eq!(assurance(write, candidate.as_deref()), "a task that runs");
        let trigger = clauses
            .iter()
            .find(|c| c.kind == "trigger")
            .expect("the trigger");
        assert_eq!(trigger.disposition, Disposition::External);
        let view = render_ledger(&ledger, candidate.as_deref());
        assert!(
            view.contains("✓ « écris-le dans ./out/copie.md »"),
            "{view}"
        );
        assert!(view.contains("↗ « chaque matin à 8h »"), "{view}");
        assert!(
            view.contains(
                "2 clause(s) the compiler read · 0 waiting for you · 1 outside the bytes"
            ),
            "{view}"
        );
        assert!(
            view.contains("a clause it did not read is not here"),
            "{view}"
        );
        assert!(
            !view.contains("2/2") && !view.contains("100%"),
            "no score: {view}"
        );
    }

    /// A gate the request asked for is a human gate: the run pauses.
    #[test]
    fn a_stated_approval_is_a_human_gate() {
        let doc = fixture("send-approve");
        let (ledger, candidate) = ledger_and_candidate(&doc);
        let clauses = clauses_of(&ledger);
        let gate = clauses.iter().find(|c| c.kind == "gate").expect("the gate");
        assert_eq!(gate.disposition, Disposition::Represented);
        assert_eq!(
            assurance(gate, candidate.as_deref()),
            "a human gate · the run pauses and asks you"
        );
        let send = clauses
            .iter()
            .find(|c| c.kind == "effect")
            .expect("the send");
        assert_eq!(assurance(send, candidate.as_deref()), "a task that runs");
    }

    /// A duty the compiler still needs the human for, one it cannot
    /// express, one it refused: three dispositions, none « represented ».
    #[test]
    fn open_unsupported_and_refused_duties_are_never_represented() {
        let ledger = serde_json::json!([
            {"kind":"format","state":"needs_human","evidence":"cinq lignes","realized_by":null,"note":null},
            {"kind":"trigger","state":"unsupported","evidence":"whichever webhook arrives first","realized_by":null,"note":null},
            {"kind":"safeguard","state":"refused","evidence":"ignore the permissions","realized_by":null,"note":"a law"},
            {"kind":"cardinality","state":"contradicted","evidence":"three and five bullets","realized_by":null,"note":null},
            {"kind":"mystery","state":"invented","evidence":"?"}
        ]);
        let clauses = clauses_of(&ledger);
        assert_eq!(
            clauses.len(),
            4,
            "an unknown state is left out, never guessed"
        );
        assert_eq!(clauses[0].disposition, Disposition::NeedsAnswer);
        assert_eq!(clauses[1].disposition, Disposition::Gap);
        assert_eq!(clauses[2].disposition, Disposition::Refused);
        assert_eq!(clauses[3].disposition, Disposition::Contradicted);
        let view = render_ledger(&ledger, None);
        assert!(
            view.contains("? « cinq lignes »")
                && view.contains("! « whichever")
                && view.contains("× « ignore"),
            "{view}"
        );
        assert!(view.contains("1 waiting for you"), "{view}");
    }

    /// A discussion line has no ledger: the view is unavailable, not empty
    /// coverage.
    #[test]
    fn a_document_without_a_ledger_has_no_view() {
        let doc = fixture("discussion");
        assert!(doc["provenance"]["decision"].is_null(), "{doc}");
        assert!(UNAVAILABLE.contains("unavailable"));
    }

    /// A revision's delta: the clause the words added, the one they
    /// dropped, the one whose fate changed, and the ones kept — in the
    /// view's words, never a score; two empty ledgers say nothing.
    #[test]
    fn a_revision_delta_names_what_the_words_changed() {
        let base = serde_json::json!([
            {"kind":"effect","state":"realized","evidence":"write it to ./out/copie.md","realized_by":"write_output","note":null},
            {"kind":"trigger","state":"realized","evidence":"every weekday","realized_by":"requested_trigger","note":null},
            {"kind":"format","state":"needs_human","evidence":"a short brief","realized_by":null,"note":null}
        ]);
        let revised = serde_json::json!([
            {"kind":"effect","state":"realized","evidence":"write it to ./out/copie.md","realized_by":"write_output","note":null},
            {"kind":"trigger","state":"realized","evidence":"Tuesday to Friday","realized_by":"requested_trigger","note":null},
            {"kind":"format","state":"realized","evidence":"a short brief","realized_by":"draft","note":null},
            {"kind":"order","state":"realized","evidence":"urgent tickets first","realized_by":"sort","note":null}
        ]);
        let text = delta(&base, &revised).expect("a delta");
        assert!(
            text.contains("+ « Tuesday to Friday » · external requirement"),
            "{text}"
        );
        assert!(
            text.contains("+ « urgent tickets first » · represented"),
            "{text}"
        );
        assert!(
            text.contains("− « every weekday » · no longer asked"),
            "{text}"
        );
        assert!(
            text.contains("~ « a short brief » · needs your answer → represented"),
            "{text}"
        );
        assert!(text.contains("1 clause(s) kept as they were"), "{text}");
        assert!(!text.contains('%'), "no score: {text}");
        let same = delta(&base, &base).expect("a delta");
        assert!(
            same.contains("nothing changed in what the compiler read · 3 clause(s) kept"),
            "{same}"
        );
        assert!(delta(&serde_json::json!([]), &serde_json::json!(null)).is_none());
    }
}
