// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The source-anchored revision of a base no semantic record binds (slice F, bounded): one
//! written destination replaced, or one added beside an existing write, in place. The seat
//! answers typed links and additions only; the compiler owns every byte. The pure laws live in
//! `sketch::import` (the destination's parsed slots, each occurrence proven alone by parsing
//! again, the complete document proven once more, the copied write of an addition); this core
//! door reads the request, records the revision in the native record seam and replays it. When
//! the facts leave the destination, the new path or the copied write open, the revision asks ONE
//! bounded choice among the exact paths, never guesses; its answer round decides with zero calls.
//! The base's digest is the import's provenance, an answer round replays only on that very base
//! ([`rebound`]), and the next revision binds the revised bytes with the words they answer.

use nika_compile_fidelity::sketch::import::{
    self,
    record::{self, Step},
};
use serde_json::Value;

use super::{CompileOutcome, CompileRequest, DiagnosticKind, Strategy};
use crate::types::{EditChange, Input};

pub use import::Substituted;

fn parse(text: &str) -> Option<Value> {
    crate::edit::literal_projection(text)
}

fn projected(request: &CompileRequest) -> record::Request<'_> {
    let (source, change) = match &request.input {
        Input::Edit { source, change } => (
            Some(source.as_str()),
            match change {
                EditChange::Text(words) => Some(words.as_str()),
                _ => None,
            },
        ),
        Input::Create(_) => (None, None),
    };
    record::Request::new(
        source,
        change,
        request.original_intent.as_deref(),
        request.plan.as_ref(),
        &request.answers,
        super::intent_sha256(&crate::revise_intent(request).unwrap_or_default()),
    )
}

fn ledger(words: &str) -> Value {
    super::request_basis(words, &CompileRequest::create(words))["ledger"].clone()
}

/// `base` with the destination `path` replaced by `by` at its parsed slots, and only there
/// (`sketch::import::substitute` over the core's literal projection).
///
/// # Errors
/// Why the replacement is not proven on this base.
pub fn substitute(base: &str, path: &str, by: &str) -> Result<Substituted, String> {
    import::substitute(base, path, by, &parse)
}

/// Convert pure evidence to core questions or a checked native outcome; never grants consent.
fn settle(step: Step, request: &CompileRequest, out: &mut CompileOutcome) {
    match step {
        Step::Done(record) => {
            let applied = record::applied(&record);
            super::native_apply(&record, request, out);
            out.provenance.plan = Some(record);
            crate::finding(out, DiagnosticKind::Applied, "revision", applied);
        }
        Step::Ask(pending, questions) => {
            for question in questions {
                super::ask(&question, out);
            }
            out.provenance.plan = Some(pending);
        }
        _ => crate::finding(
            out,
            DiagnosticKind::Unknown,
            "revision",
            "The recorded revision is not a result this compiler applies.",
        ),
    }
}

/// Revise `request` (a change in words to a base no semantic record binds, or to the bytes a
/// source revision wrote) under the seat's typed `stated` links and additions: one destination
/// replaced or added in place, recorded in the native seam; a bounded choice when the facts leave
/// it open; or why not — named, with no candidate.
#[must_use]
pub fn source_revision(request: &CompileRequest, stated: &Value) -> CompileOutcome {
    let mut out = crate::initial();
    out.provenance.strategy = Some(Strategy::Native);
    match record::revise(&projected(request), stated, &ledger, &parse) {
        Ok(step) => settle(step, request, &mut out),
        Err(why) => crate::finding(
            &mut out,
            DiagnosticKind::Missed,
            "revision",
            format!(
                "The revision is not kept: {why}. The base is kept as it is; state the change again."
            ),
        ),
    }
    out
}

/// The answer round of a revision's bounded question: on the very base and request it asked,
/// the same typed links decide again with this round's answers, zero calls — the record then
/// replayed as any native revision ([`super::native_replay`]: held for its judge), or the question
/// still open.
pub(crate) fn answered(
    intent: &str,
    record: &Value,
    request: &CompileRequest,
    judgments: &[crate::ledger::Judgment],
    out: &mut CompileOutcome,
) {
    super::record_route(out, &["replayed revision question".to_owned()]);
    out.provenance.strategy = Some(Strategy::Native);
    let step = record::answered(
        &super::intent_sha256(intent),
        record,
        &projected(request),
        &ledger,
        &parse,
    );
    match step {
        Ok(Step::Done(done)) => super::native_replay(intent, &done, request, judgments, out),
        Ok(ask) => settle(ask, request, out),
        Err(why) => crate::finding(
            out,
            DiagnosticKind::Unknown,
            "recorded_plan",
            format!(
                "The recorded revision question does not bind this request: {why}. Revise the workflow again without it."
            ),
        ),
    }
}

/// Recheck the source revision's byte identity before the native replay grants any verdict.
///
/// # Errors
/// Why the record does not bind the current base.
pub(crate) fn rebound(record: &Value, request: &CompileRequest) -> Result<(), String> {
    record::rebound(record, projected(request).source, &parse)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Flow style, quotes, a comment naming the destination, a read of a similarly named path and
    /// data text naming the destination: only the write and its permit may change.
    const FLOW: &str = r#"nika: copy
# the result goes to a.txt
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["data/a.txt"]
    write: ["a.txt"]
tasks:
  read_source:
    invoke: { tool: "nika:read", args: { path: "data/a.txt" } }
  note:
    invoke: { tool: "nika:log", args: { message: "a.txt" } }
  write_dest:
    with: { content: "${{ tasks.read_source.output }}" }
    invoke: { tool: "nika:write", args: { path: "a.txt", content: "${{ with.content }}", overwrite: true } }
"#;

    #[test]
    fn only_the_destination_slots_change_and_every_other_byte_stays() {
        let out = substitute(FLOW, "a.txt", "b.txt").unwrap();
        assert_eq!(
            out.slots,
            ["/tasks/write_dest/invoke/args/path", "/permits/fs/write/0"]
        );
        let expected = FLOW
            .replace("write: [\"a.txt\"]", "write: [\"b.txt\"]")
            .replace("{ path: \"a.txt\", content", "{ path: \"b.txt\", content");
        assert_eq!(out.source, expected);
        assert!(out.source.contains("# the result goes to a.txt"));
        assert!(out.source.contains("message: \"a.txt\""));
        assert!(out.source.contains("\"data/a.txt\""));
    }

    #[test]
    fn a_block_source_through_a_constant_keeps_the_other_destination() {
        let block = "nika: totals\nconst:\n  confirmed_path: confirmed.csv\n  total_path: 'total.txt'\npermits:\n  tools:\n  - nika:write\n  fs:\n    write:\n    - confirmed.csv\n    - total.txt\ntasks:\n  write_confirmed:\n    invoke:\n      tool: nika:write\n      args:\n        path: ${{ const.confirmed_path }}\n        content: x\n  write_total:\n    invoke:\n      tool: nika:write\n      args:\n        path: ${{ const.total_path }}\n        content: y\n";
        let out = substitute(block, "confirmed.csv", "final.csv").unwrap();
        assert_eq!(out.slots, ["/const/confirmed_path", "/permits/fs/write/0"]);
        assert_eq!(out.source, block.replace("confirmed.csv", "final.csv"));
        assert!(out.source.contains("total_path: 'total.txt'"));
    }

    #[test]
    fn a_rooted_destination_keeps_its_dot_slash_form() {
        let base = "nika: c\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"./out/first.txt\"]\ntasks:\n  w:\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"./out/first.txt\"\n        content: \"x\"\n";
        let out = substitute(base, "./out/first.txt", "./out/second.txt").unwrap();
        assert_eq!(out.source, base.replace("first", "second"));
    }

    #[test]
    fn an_unprovable_or_colliding_replacement_is_refused() {
        assert!(
            substitute(FLOW, "data/a.txt", "x.txt")
                .unwrap_err()
                .contains("not a destination")
        );
        assert!(
            substitute(FLOW, "a.txt", "data/a.txt")
                .unwrap_err()
                .contains("already named")
        );
        assert!(substitute("not: [yaml", "a.txt", "b.txt").is_err());
    }

    /// A block-style base whose destination is a constant and whose permits are a block list:
    /// the added write is a copy of that task with a literal path, the permit appended as an item.
    #[test]
    fn an_added_destination_copies_the_write_and_appends_its_permit_in_place() {
        let block = "nika: totals\nconst:\n  confirmed_path: confirmed.csv\npermits:\n  tools:\n  - nika:write\n  fs:\n    write:\n    - confirmed.csv\ntasks:\n  write_confirmed:\n    invoke:\n      tool: nika:write\n      args:\n        path: ${{ const.confirmed_path }}\n        content: x\n# confirmed.csv is the business copy\n";
        let parse = |text: &str| crate::edit::literal_projection(text);
        let out = import::add_destination(block, "confirmed.csv", "final.csv", &parse).unwrap();
        let doc = parse(&out.source).unwrap();
        assert_eq!(
            doc["tasks"]["write_confirmed_2"]["invoke"]["args"]["path"],
            "final.csv"
        );
        assert_eq!(
            doc["tasks"]["write_confirmed"]["invoke"]["args"]["path"],
            "${{ const.confirmed_path }}"
        );
        assert_eq!(
            doc["permits"]["fs"]["write"],
            json!(["confirmed.csv", "final.csv"])
        );
        assert!(out.source.contains("# confirmed.csv is the business copy"));
        assert_eq!(
            out.slots,
            ["/tasks/write_confirmed_2", "/permits/fs/write/1"]
        );
    }

    /// A flow-style task map is no block the compiler copies in place: refused by name.
    #[test]
    fn a_flow_task_map_is_not_copied_in_place() {
        let flow = "nika: c\npermits: { tools: [\"nika:write\"], fs: { write: [\"a.txt\"] } }\ntasks: { w: { invoke: { tool: \"nika:write\", args: { path: \"a.txt\", content: \"x\" } } } }\n";
        let parse = |text: &str| crate::edit::literal_projection(text);
        let why = import::add_destination(flow, "a.txt", "b.txt", &parse).unwrap_err();
        assert!(why.contains("not a task block"), "{why}");
    }
}
