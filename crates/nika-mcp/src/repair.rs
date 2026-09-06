// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika_check(fix: true)` — the repair door of the oracle, IN MEMORY.
//!
//! Three spec codes (the NIKA-VAR-021 family · NIKA-PARSE-024) prescribe
//! `nika check --fix` as the canonical repair, and a no-shell agent wired
//! to this server had no such door: every repair landed by hand (#1270).
//! This module walks the SAME ladder the CLI verb walks
//! ([`nika_cli_host::fix_ladder`] — the typed-rename splices · the
//! dead-form migrations · the W2 hoist) over the source the caller handed
//! in, and hands the repaired text BACK. Nothing here touches a file: the
//! tool takes a workflow's source, never a path, and the same tools are
//! served over the HTTP transport — a path argument would turn an
//! authenticated remote client into a file-writer. The caller writes the
//! text back and checks again; the server stays read-only by construction.
//!
//! Every round is a TRANSACTION (the CLI's contract, replicated): it
//! starts from a savepoint, applies its repairs in memory, and is
//! committed only if the result still parses as YAML. A round that breaks
//! the document is rolled back — repairs and stop notes included — and
//! reported as a typed refusal; the returned text is committed text only,
//! never text `check` could not read.

use nika_cli_host::fix_ladder::{
    MAX_ROUNDS, Refusal, Repair, StopNotes, apply_dead_form_arm, apply_prepass,
    collect_typed_renames, judge_round, splice, try_w2_hoist,
};
use serde_json::{Value, json};

/// The in-payload wire marker of the fix answer (additive from here).
const FIX_VERSION: u8 = 1;

/// One round's working state — the text and the bookkeeping. The same
/// shape is the SAVEPOINT a round is rolled back to.
#[derive(Clone)]
pub(crate) struct Round {
    /// The workflow text as this round left it.
    pub(crate) source: String,
    /// Every repair claimed so far (applied or skipped-retryable).
    pub(crate) repairs: Vec<Repair>,
    /// The equivalence-or-stop notes (W2 · D1), verbatim.
    pub(crate) stop_notes: StopNotes,
}

/// The repaired text and its ledger — what the tool answers from.
pub(crate) struct Outcome {
    /// The committed state after the last round.
    pub(crate) round: Round,
    /// The rounds the loop refused to commit.
    pub(crate) refusals: Vec<Refusal>,
}

impl Outcome {
    /// How many repairs landed in committed rounds.
    fn applied(&self) -> usize {
        self.round.repairs.iter().filter(|r| r.applied).count()
    }
}

/// Walk the repair ladder over `original` until a round applies nothing
/// (capped by [`MAX_ROUNDS`]). Pure: the text in, the text out.
pub(crate) fn repair(original: &str) -> Outcome {
    let mut round = Round {
        source: original.to_owned(),
        repairs: Vec::new(),
        stop_notes: StopNotes(Vec::new()),
    };
    let mut refusals = Vec::new();
    // ADR-124 · one ladder, two doors: the CLI's prepass (a bare `exec:`
    // scalar · a `needs:` list) runs here too, so « the same ladder as
    // `nika check --fix` » is true on this door.
    apply_prepass(&mut round.source, &mut round.repairs, &mut round.stop_notes);
    for _ in 0..MAX_ROUNDS {
        let savepoint = round.clone();
        let progressed = one_round(&mut round);
        // COMMIT or ROLL BACK. Whatever this round announced, if it turned
        // a document that parsed into one that does not, it was not a
        // repair — the savepoint wins and the round is reported refused.
        if rollback_if_broken(savepoint, &mut round, &mut refusals) || !progressed {
            break;
        }
    }
    Outcome { round, refusals }
}

/// One round of the ladder. `true` when something applied (the loop
/// re-parses — convergence IS the proof), `false` when this loop can
/// repair nothing more: a parse-fatal it cannot splice, a STOP, or a
/// clean round.
fn one_round(round: &mut Round) -> bool {
    match nika_schema::parse(
        &round.source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) {
        Err(e) => {
            // The ONE typed rename door (`rename_repair`): a near-miss
            // key splices; a teaching-only refusal offers no rename and
            // is never scraped (the sentence-as-key corruption).
            if let Some((old, new)) = e.rename_repair() {
                splice(&mut round.source, &old, &new, "field", &mut round.repairs)
            } else {
                apply_dead_form_arm(
                    &e,
                    &mut round.source,
                    &mut round.repairs,
                    &mut round.stop_notes,
                ) == Some(true)
            }
        }
        Ok(wf) => {
            let report = nika_check::check(&wf);
            match try_w2_hoist(
                &report,
                &mut round.source,
                &mut round.repairs,
                &mut round.stop_notes,
            ) {
                Some(applied) => applied,
                None => collect_typed_renames(&report).into_iter().fold(
                    false,
                    |applied, (old, new, kind)| {
                        splice(&mut round.source, &old, &new, kind, &mut round.repairs) || applied
                    },
                ),
            }
        }
    }
}

/// The transaction's judge: when the round's text no longer parses as
/// YAML although the savepoint's did, restore the savepoint (text ·
/// repair rows · stop notes), record the typed refusal — naming every
/// row the round had claimed as applied — and answer `true` (the loop
/// stops: nothing this round did survives). Otherwise the round is
/// committed and the answer is `false`.
pub(crate) fn rollback_if_broken(
    savepoint: Round,
    round: &mut Round,
    refusals: &mut Vec<Refusal>,
) -> bool {
    let attempted: Vec<String> = round.repairs[savepoint.repairs.len()..]
        .iter()
        .filter(|r| r.applied)
        .map(|r| format!("{} `{}` → `{}`", r.kind, r.old, r.new))
        .collect();
    let Some(refusal) = judge_round(&savepoint.source, &round.source, attempted) else {
        return false;
    };
    *round = savepoint;
    refusals.push(refusal);
    true
}

/// `nika_check(fix: true)` — repair in memory, then the PLAIN audit of
/// the repaired text (`--fix` is check plus a pen, never a different
/// audit). `Ok` when the re-audit is green, `Err` (→ `isError: true`)
/// while findings remain — the same exit-code mirror the plain tool
/// keeps — and the SAME JSON rides both, so the caller writes `workflow`
/// back either way and repairs the rest from `verdict`.
pub(crate) fn check_fix(original: &str, native_strict: bool) -> Result<String, String> {
    let outcome = repair(original);
    let (clean, verdict) = match crate::tools::audit(&outcome.round.source, native_strict) {
        Ok(text) => (true, text),
        Err(text) => (false, text),
    };
    let applied = outcome.applied();
    let changed = outcome.round.source != original;
    let mut next_actions: Vec<String> = Vec::new();
    if changed {
        next_actions.push(
            "write the `workflow` text back to the file verbatim, then nika_check again".to_owned(),
        );
    }
    if !clean {
        next_actions.push(
            "repair the findings in `verdict` by hand — nothing left is machine-applicable"
                .to_owned(),
        );
    }
    let payload = json!({
        "fix_version": FIX_VERSION,
        "applied": applied,
        "changed": changed,
        "clean": clean,
        "repairs": outcome.round.repairs.iter().map(|r| json!({
            "kind": r.kind, "old": r.old, "new": r.new, "applied": r.applied,
        })).collect::<Vec<Value>>(),
        "stops": outcome.round.stop_notes.0,
        "refused": outcome.refusals.iter().map(|r| json!({
            "attempted": r.attempted, "reason": r.reason,
        })).collect::<Vec<Value>>(),
        "workflow": outcome.round.source,
        "verdict": verdict,
        "next_actions": next_actions,
    });
    let detail = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("fix answer serialization failed: {e}"))?;
    if clean {
        Ok(format!(
            "✔ fixed — {applied} repair(s) applied in memory and the re-audit is green; \
             write `workflow` back verbatim:\n{detail}"
        ))
    } else {
        Err(format!(
            "✖ fixed {applied} repair(s) in memory, findings remain — write `workflow` \
             back if `changed`, then repair the rest from `verdict`:\n{detail}"
        ))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::tools::execute;

    fn fixed(workflow: &str) -> Result<Value, Value> {
        let payload = |text: String| -> Value {
            let start = text.find('{').expect("the fix answer is a JSON object");
            serde_json::from_str(&text[start..]).expect("valid fix JSON")
        };
        execute("nika_check", &json!({ "workflow": workflow, "fix": true }))
            .map(payload)
            .map_err(payload)
    }

    /// The loop closes: a finding → the repair applied → the re-audit
    /// green → the returned source is what a plain `nika_check` accepts.
    /// The battery's own author-error classes, stacked: a parse-fatal
    /// field typo (`promt`) · a tool typo (`nika:raed`) · an arg typo
    /// (`inpit`) — healed across rounds, in memory.
    #[test]
    fn fix_heals_field_tool_and_arg_typos_and_the_reaudit_is_green() {
        let broken = "nika: w\nmodel: mock/echo\npermits: { tools: [\"nika:read\", \"nika:jq\"], fs: { read: [\"./x\"] } }\ntasks:\n  think:\n    infer: { promt: \"hi\", max_tokens: 10 }\n  read_it:\n    invoke: { tool: \"nika:raed\", args: { path: \"./x\" } }\n  shape:\n    invoke: { tool: \"nika:jq\", args: { expression: \".\", inpit: 1 } }\n";
        assert!(
            execute("nika_check", &json!({ "workflow": broken })).is_err(),
            "the fixture is dirty before the fix"
        );
        let out = fixed(broken).expect("the re-audit is green");
        assert_eq!(out["fix_version"], 1);
        assert_eq!(out["applied"], 3, "{out:#}");
        assert_eq!(out["changed"], true);
        assert_eq!(out["clean"], true);
        let healed = out["workflow"]
            .as_str()
            .expect("the repaired source rides back");
        assert!(healed.contains("prompt:"), "field healed: {healed}");
        assert!(healed.contains("nika:read"), "tool healed: {healed}");
        assert!(healed.contains("input: 1"), "arg healed: {healed}");
        assert!(!healed.contains("promt") && !healed.contains("raed") && !healed.contains("inpit"));
        let rows = out["repairs"].as_array().expect("repair rows");
        let row = |kind: &str, old: &str, new: &str| {
            rows.iter().any(|r| {
                r["kind"] == kind && r["old"] == old && r["new"] == new && r["applied"] == true
            })
        };
        assert!(row("field", "promt", "prompt"), "{out:#}");
        assert!(row("tool", "nika:raed", "nika:read"), "{out:#}");
        assert!(row("arg", "inpit", "input"), "{out:#}");
        // The verdict is the plain audit of the repaired text — and that
        // text, checked again WITHOUT fix, is green: the loop is closed.
        assert!(
            out["verdict"]
                .as_str()
                .expect("verdict text")
                .contains("clean"),
            "{out:#}"
        );
        execute("nika_check", &json!({ "workflow": healed }))
            .expect("the returned source passes the plain audit");
    }

    /// A structural finding (a missing required arg) has no rename: the
    /// source rides back byte-identical, nothing claims to be applied, and
    /// the answer stays red — a fix that cannot fix says so.
    #[test]
    fn fix_without_applicable_repairs_returns_the_source_unchanged_and_red() {
        let body = "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:hash\" }\n";
        let out = fixed(body).expect_err("the structural finding still reds");
        assert_eq!(out["applied"], 0, "{out:#}");
        assert_eq!(out["changed"], false, "{out:#}");
        assert_eq!(out["clean"], false, "{out:#}");
        assert_eq!(
            out["workflow"], body,
            "no rewrite without an applied repair"
        );
        assert!(
            out["verdict"].as_str().expect("verdict").contains("NIKA-"),
            "the remaining findings ride the verdict: {out:#}"
        );
    }

    /// The 2026-08-18 corruption class, both shapes: a dead envelope key
    /// with NO mechanical repair (`policy:`) and a de-commented modeline
    /// refuse with PROSE — the prose is never spliced in as a key.
    #[test]
    fn fix_never_splices_teaching_prose_into_the_document() {
        for body in [
            "nika: hello\npolicy: {}\ntasks:\n  say:\n    exec:\n      command: [echo, hi]\n",
            "nika: hello\nyaml-language-server: $schema=x\ntasks:\n  say:\n    exec:\n      command: [echo, hi]\n",
        ] {
            let out = fixed(body).expect_err("the refusal still reds");
            assert_eq!(out["workflow"], body, "prose is never spliced · {out:#}");
            assert_eq!(out["applied"], 0, "{out:#}");
            assert_eq!(out["changed"], false, "{out:#}");
        }
    }

    /// The two-site convergence case: `buidl` occurs twice (bare as an
    /// `after:` target · inside the qualified `tasks.buidl` ref), so the
    /// bare rename is ambiguous in round 1 — the qualified one is unique,
    /// applies, and leaves the bare token standing alone for round 2.
    #[test]
    fn fix_converges_across_rounds_on_the_two_site_case() {
        let body = "nika: w\npermits: { tools: [\"nika:log\"] }\ninputs: { topic: { type: string, required: true } }\ntasks:\n  build:\n    invoke: { tool: \"nika:log\", args: { message: \"building ${{ inputs.topik }}\" } }\n  ship:\n    after:\n      buidl: success\n    invoke: { tool: \"nika:log\", args: { message: \"shipping\" } }\noutputs:\n  made: ${{ tasks.buidl.output }}\n";
        let out = fixed(body).expect("clean after convergence");
        let healed = out["workflow"].as_str().expect("source");
        assert!(healed.contains("build: success"), "{healed}");
        assert!(healed.contains("tasks.build.output"), "{healed}");
        assert!(healed.contains("inputs.topic"), "{healed}");
        assert!(!healed.contains("buidl") && !healed.contains("topik"));
        assert!(
            out["repairs"]
                .as_array()
                .expect("rows")
                .iter()
                .all(|r| r["applied"] == true),
            "no residual skip rows: {out:#}"
        );
    }

    /// The transaction contract at the seam every round passes through:
    /// a round claims two applied repairs but its text no longer parses
    /// as YAML — the savepoint's text, rows and notes come back, the
    /// refusal names both claimed rows, and the loop is told to stop.
    #[test]
    fn a_round_that_breaks_the_document_is_rolled_back_and_refused() {
        let good = "nika: w\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n".to_owned();
        let mut round = Round {
            source: "nika: w\nthe fields here: a · b:\n  t: [unclosed\n".to_owned(),
            repairs: vec![
                Repair::applied("earlier", "kept", "field"),
                Repair::applied("workflow", "the fields here: a · b", "field"),
                Repair::applied("x", "y", "arg"),
            ],
            stop_notes: StopNotes(vec![
                "an earlier note".to_owned(),
                "a note from the bad round".to_owned(),
            ]),
        };
        let savepoint = Round {
            source: good.clone(),
            repairs: vec![Repair::applied("earlier", "kept", "field")],
            stop_notes: StopNotes(vec!["an earlier note".to_owned()]),
        };
        let mut refusals = Vec::new();
        assert!(rollback_if_broken(savepoint, &mut round, &mut refusals));
        assert_eq!(round.source, good, "the savepoint's text is back");
        assert_eq!(round.repairs.len(), 1, "the round's rows are gone");
        assert_eq!(round.stop_notes.0, vec!["an earlier note".to_owned()]);
        assert_eq!(refusals.len(), 1);
        assert_eq!(
            refusals[0].attempted,
            vec![
                "field `workflow` → `the fields here: a · b`".to_owned(),
                "arg `x` → `y`".to_owned()
            ]
        );
        // The commit half: a round whose text still parses is kept whole.
        let savepoint = Round {
            source: good.clone(),
            repairs: Vec::new(),
            stop_notes: StopNotes(Vec::new()),
        };
        let mut round = Round {
            source: good.clone(),
            repairs: vec![Repair::applied("a", "b", "arg")],
            stop_notes: StopNotes(Vec::new()),
        };
        assert!(!rollback_if_broken(savepoint, &mut round, &mut refusals));
        assert_eq!(round.repairs.len(), 1, "committed whole");
        assert_eq!(refusals.len(), 1, "no new refusal");
    }

    /// `fix: false` (or absent) IS the plain audit — one answer, one voice.
    /// ADR-124 · one ladder, two doors: the oracle's `fix: true` runs the
    /// CLI's prepass — a bare `exec:` scalar (the foreign form the parser
    /// refuses) becomes the argv mapping and the re-audit is green.
    #[test]
    fn fix_runs_the_prepass_the_cli_runs() {
        let broken = "nika: w\npermits: { exec: [\"echo\"] }\ntasks:\n  t:\n    exec: echo hi\n";
        let payload = |text: String| -> Value {
            let start = text.find('{').expect("the fix answer is a JSON object");
            serde_json::from_str(&text[start..]).expect("valid fix JSON")
        };
        let out = execute(
            "nika_check",
            &json!({ "workflow": broken, "fix": true, "native_strict": false }),
        )
        .map(payload)
        .map_err(payload)
        .expect("the re-audit is green");
        assert_eq!(out["clean"], true, "{out:#}");
        let healed = out["workflow"].as_str().expect("the repaired source");
        assert!(healed.contains("command: [\"echo\", \"hi\"]"), "{healed}");
        let rows = out["repairs"].as_array().expect("repair rows");
        assert!(
            rows.iter()
                .any(|r| r["kind"] == "bare-exec" && r["applied"] == true),
            "{out:#}"
        );
    }

    #[test]
    fn fix_false_is_the_plain_audit() {
        let wf = "nika: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let plain = execute("nika_check", &json!({ "workflow": wf }));
        let off = execute("nika_check", &json!({ "workflow": wf, "fix": false }));
        assert_eq!(plain, off);
        assert!(!plain.expect("green").contains("fix_version"));
    }

    /// The repair door is declared where it is honoured — and OFF by
    /// default: a plain audit never rewrites anything, even in memory.
    #[test]
    fn the_fix_flag_is_declared_on_nika_check_and_off_by_default() {
        let listed = crate::tools::catalog();
        let check_tool = listed
            .as_array()
            .expect("a tool array")
            .iter()
            .find(|t| t["name"] == "nika_check")
            .expect("nika_check is served");
        let fix = &check_tool["inputSchema"]["properties"]["fix"];
        assert_eq!(fix["type"], json!("boolean"), "{check_tool:#}");
        assert_eq!(fix["default"], json!(false), "{check_tool:#}");
        assert!(
            fix["description"]
                .as_str()
                .expect("described")
                .contains("write"),
            "the description says the caller writes the text back: {fix:#}"
        );
    }
}
