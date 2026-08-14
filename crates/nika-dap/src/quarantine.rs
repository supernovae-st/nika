// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The failed run's debt (F-P14 · NEP-0017 · « obligation de fin — la
//! dette du run », sous le mot réservé `finally`) — palier v1: the
//! QUARANTINE. `TokioFs::write` is atomic (temp + rename), so a
//! "semi-written output" is a file FULLY written by a task that settled
//! Success in a run whose terminal verdict is FAILED — the run does
//! not stop on a task failure, later waves settle Cancelled/Upstream,
//! and the writer's bytes outlive the verdict. Those bytes are the
//! debt: left in place, the next run reads them as honest inputs.
//!
//! The effect: every output path a Success-settled `nika:write` /
//! `nika:edit` task returned MOVES under
//! `.nika/quarantine/<run-stamp>/` ([`crate::store::QUARANTINE_DIR`]
//! — beside the journals, never inside them). The MOVE is the
//! substance of the palier: the old path no longer exists, so a next
//! run reading it fails LOUD (`NIKA-BUILTIN-READ-001`) instead of
//! silently consuming a semi-written artifact — the law's negative
//! acceptance at v1 semantics. The attestation rides the receipt
//! surface: the fold this module returns lands in the terminal
//! `run_sealed` line's `covers` via the F-P2 teardown (which proves
//! THAT the end happened; F-P14 says WHAT the end must do).
//!
//! Two Success settle classes are NOT debt — only paths THIS run
//! actually wrote are:
//!
//! - **recovered** (`cause == Recovered`) — the task FAILED, its
//!   `on_error: recover:` chain settled it, and the record's `output`
//!   is the AUTHOR-SUPPLIED fallback value, not a path a verb wrote.
//!   Moving it would rename an arbitrary author-named file past the
//!   permits boundary (the write permits gated the failed verb's
//!   `path:`, never the recover arm's value).
//! - **cache-hit** (the id rides [`RunOutcome::cache_hits`]) — a
//!   resumed run's hit REHYDRATES a prior run's honest output; nothing
//!   was written here, so there is nothing to quarantine (the prior
//!   run's own teardown already judged that debt).
//!
//! Boundaries (the ledger text): the saga/compensation palier is
//! declared P2 — no `finally:` schema block in v1, no new `EventKind`,
//! no runtime change (the runtime stays fs-free by design; this is the
//! L4 teardown's own effect — descended from `nika-cli` to here at the
//! 15k wall, the trust-plane pattern: compute descends, render stays).
//! The true check-side cross-run finding (prior quarantine lists × the
//! next workflow's read paths) is the named v2 owe. A quarantine
//! failure is a STATED MISS (`{path, error, action: "left_in_place"}`)
//! — never silent, never fatal to the teardown: the seal must still
//! attest the end it can.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use nika_runtime::{RunOutcome, TaskStatus, TerminalCause};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};

/// The write-capable builtins whose settled outputs ARE the
/// semi-written debt (v1 scope) — the two that return the path they
/// wrote as their output value.
const WRITER_TOOLS: [&str; 2] = ["nika:write", "nika:edit"];

/// The quarantine effect for one settled run, folded for the seal's
/// `covers` — `None` unless the run FAILED (terminal verdict not ok ·
/// not paused; a killed run never reaches teardown) AND at least one
/// Success-settled writer/edit task left an output path. `journal` is
/// the run journal's path once the `TraceFileSink` opened one: its
/// file stem IS the run-stamp (the honest run identifier the first
/// event's `UUIDv7` minted — the same stamp the journal file carries),
/// tying the debt dir to the journal whose `run_sealed` line attests
/// it. A disabled/broken journal falls back to a fresh mint in the
/// journal's own naming shape — the debt exists either way; the move
/// never waits on the receipt surface.
#[must_use]
pub fn attend(
    wf: &RawWorkflow,
    outcome: &RunOutcome,
    journal: Option<&Path>,
) -> Option<serde_json::Value> {
    if outcome.ok || outcome.paused.is_some() {
        return None; // a clean run attests nothing (absent is honest)
    }
    let paths = written_paths(wf, outcome);
    if paths.is_empty() {
        return None; // no semi-written debt — nothing to attest
    }
    let dir = PathBuf::from(crate::store::QUARANTINE_DIR).join(run_stamp(journal));
    Some(quarantine(&paths, &dir))
}

/// Enumerate the run's semi-written outputs: `wf.tasks ∩
/// outcome.records` where the task invokes a WRITER tool and settled
/// Success. The settled record's `output` carries the actual path(s) —
/// a string for a single call, an array of per-element path strings
/// for a fan-out (`for_each` folds its elements into one array).
/// Dedupe first-wins (two tasks writing the same path move it once);
/// non-string outputs are skipped (v1 scope — the writers return
/// strings by contract, anything else is not a path we can name).
///
/// Two Success classes are excluded — only a path THIS run's verb
/// actually wrote is debt: a `Recovered` record's output is the
/// author's `recover:` fallback (the failed verb wrote nothing; moving
/// the author-named value would be an arbitrary file move past the
/// permits boundary), and a cache-hit record rehydrates a PRIOR run's
/// output (nothing written here — the prior teardown judged its debt).
fn written_paths(wf: &RawWorkflow, outcome: &RunOutcome) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut paths = Vec::new();
    let cache_hits: BTreeSet<&str> = outcome.cache_hits.iter().map(String::as_str).collect();
    for task in &wf.tasks {
        let RawAction::Invoke(invoke) = &task.value.action else {
            continue;
        };
        let RawInvokeTarget::Tool(tool) = &invoke.target else {
            continue; // a child-workflow call writes no path of its own
        };
        if !WRITER_TOOLS.contains(&tool.value.as_str()) {
            continue;
        }
        let id = task.value.id.value.as_str();
        let Some(record) = outcome.records.get(id) else {
            continue;
        };
        if record.status != TaskStatus::Success {
            continue; // cancelled/upstream/failed records carry Null (spec 04)
        }
        if record.cause == TerminalCause::Recovered {
            continue; // the recover arm's fallback — this run wrote no path
        }
        if cache_hits.contains(id) {
            continue; // a rehydrated prior output — not this run's debt
        }
        let candidates: &[serde_json::Value] = match &record.output {
            single @ serde_json::Value::String(_) => std::slice::from_ref(single),
            serde_json::Value::Array(elements) => elements,
            _ => &[],
        };
        for candidate in candidates {
            if let serde_json::Value::String(path) = candidate
                && seen.insert(path.clone())
            {
                paths.push(path.clone());
            }
        }
    }
    paths
}

/// The run-stamp: the journal's file stem when the journal opened
/// (`<compact-ts>-<short-id>` — collision-proofed at the journal's own
/// `create_new` open), else a fresh mint in the same shape.
fn run_stamp(journal: Option<&Path>) -> String {
    journal
        .and_then(Path::file_stem)
        .map_or_else(mint_stamp, |stem| stem.to_string_lossy().into_owned())
}

/// The fallback mint for a journal-less run (`--no-trace-file` · a
/// broken rider) — the journal's naming shape (`<ISO-compact>-<tail>`,
/// the tail being the random end of a fresh `UUIDv7`), so a quarantine
/// dir reads exactly like the stamp the journal would have given.
/// Wall-clock + entropy at the L4 boundary — the same seams the
/// journal's own seal line uses.
fn mint_stamp() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let iso = nika_types::timestamp::Timestamp::from_unix_ms(millis).to_string();
    let seconds = iso.split('.').next().unwrap_or(&iso);
    let simple = nika_types::id::EventId::generate()
        .uuid
        .as_simple()
        .to_string();
    let short = &simple[simple.len().saturating_sub(4)..];
    format!("{}Z-{short}", seconds.replace(':', "-"))
}

/// The moves + the fold. Every enumeration entry is STATED: a moved
/// file rides `{path, quarantined_to}`; a move that failed rides
/// `{path, error, action: "left_in_place"}` — the debt is named, the
/// teardown never aborts. The dir is created lazily by the first move
/// (a run whose moves all miss leaves no empty dir behind); the name
/// inside the dir is the source's file name, collision-suffixed
/// (`out.txt` → `out--2.txt`) when two written paths share one name.
fn quarantine(paths: &[String], dir: &Path) -> serde_json::Value {
    let mut outputs = Vec::new();
    let mut taken = BTreeSet::new();
    for path in paths {
        let source = Path::new(path);
        let Some(name) = source.file_name() else {
            outputs.push(serde_json::json!({
                "path": path,
                "error": "the path names no file",
                "action": "left_in_place",
            }));
            continue;
        };
        let target = dir.join(unique_name(&name.to_string_lossy(), &mut taken));
        match std::fs::create_dir_all(dir).and_then(|()| std::fs::rename(source, &target)) {
            Ok(()) => outputs.push(serde_json::json!({
                "path": path,
                "quarantined_to": target.to_string_lossy(),
            })),
            Err(e) => outputs.push(serde_json::json!({
                "path": path,
                "error": e.to_string(),
                "action": "left_in_place",
            })),
        }
    }
    serde_json::json!({
        "dir": dir.to_string_lossy(),
        "outputs": outputs,
    })
}

/// A collision-free name inside the quarantine dir: the first claim
/// keeps the bare file name; later claims of the same name take
/// `<stem>--<n>[.<ext>]` for the first free `n ≥ 2` (deterministic in
/// enumeration order — the fold replays exactly).
fn unique_name(name: &str, taken: &mut BTreeSet<String>) -> String {
    if taken.insert(name.to_owned()) {
        return name.to_owned();
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned());
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    let mut n = 2u64;
    loop {
        let candidate = match &ext {
            Some(ext) => format!("{stem}--{n}.{ext}"),
            None => format!("{stem}--{n}"),
        };
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::collections::BTreeMap;

    use nika_runtime::{TaskRecord, TerminalCause};

    use super::*;

    fn parsed(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn settled(status: TaskStatus, output: serde_json::Value) -> TaskRecord {
        let cause = match status {
            TaskStatus::Success => TerminalCause::Normal,
            TaskStatus::Failure => TerminalCause::VerbError,
            TaskStatus::Skipped => TerminalCause::Gate,
            TaskStatus::Cancelled => TerminalCause::Upstream,
        };
        let mut record = TaskRecord::unran(status, cause);
        record.output = output;
        record
    }

    /// The fixture: two single writers (`a` write · `e` edit), a fan-out
    /// writer (`f`), a non-writer invoke (`j`), an exec (`x`), and a
    /// writer that FAILED (`z`) — one of each kind the enumeration meets.
    const WF: &str = r#"
nika: quarantine-enum
permits:
  tools: ["nika:write", "nika:edit", "nika:read"]
  fs: { write: ["**"], read: ["**"] }
tasks:
  a:
    invoke: { tool: "nika:write", args: { path: "one.txt", content: "1" } }
  e:
    invoke: { tool: "nika:edit", args: { path: "two.txt" } }
  f:
    for_each: { items: ["u", "v"] }
    invoke: { tool: "nika:write", args: { path: "${{ item }}.txt", content: "x" } }
  j:
    invoke: { tool: "nika:read", args: { path: "one.txt" } }
  x:
    exec: { command: ["false"] }
  z:
    invoke: { tool: "nika:write", args: { path: "zed.txt", content: "z" } }
"#;

    fn outcome_with(records: &[(&str, TaskRecord)]) -> RunOutcome {
        RunOutcome::new(
            false,
            records
                .iter()
                .map(|(id, r)| ((*id).to_owned(), r.clone()))
                .collect::<BTreeMap<_, _>>(),
            BTreeMap::new(),
        )
    }

    /// The enumeration picks EXACTLY the success-settled writer/edit
    /// paths — string output AND the fan-out array — and skips the
    /// non-writer invoke, the exec task, and the failed writer.
    #[test]
    fn the_enumeration_picks_exactly_the_success_settled_writer_paths() {
        let wf = parsed(WF);
        let outcome = outcome_with(&[
            (
                "a",
                settled(TaskStatus::Success, serde_json::json!("one.txt")),
            ),
            (
                "e",
                settled(TaskStatus::Success, serde_json::json!("two.txt")),
            ),
            (
                "f",
                settled(
                    TaskStatus::Success,
                    serde_json::json!(["u.txt", "v.txt", "u.txt"]), // fan-out + an in-array dupe
                ),
            ),
            (
                "j",
                settled(TaskStatus::Success, serde_json::json!("one.txt")),
            ),
            ("x", settled(TaskStatus::Failure, serde_json::Value::Null)),
            ("z", settled(TaskStatus::Failure, serde_json::Value::Null)),
        ]);
        let paths = written_paths(&wf, &outcome);
        assert_eq!(
            paths,
            ["one.txt", "two.txt", "u.txt", "v.txt"],
            "workflow order · dedupe first-wins (the reader's output never \
             counts — `j` is no writer — and the failed `z` is out): {paths:?}"
        );
    }

    /// A writer whose record is missing (never scheduled) or settled
    /// Cancelled contributes NOTHING — only Success is debt.
    #[test]
    fn only_success_is_debt() {
        let wf = parsed(WF);
        let outcome = outcome_with(&[
            (
                "a",
                settled(TaskStatus::Success, serde_json::json!("one.txt")),
            ),
            ("e", settled(TaskStatus::Cancelled, serde_json::Value::Null)),
            // `f` · `j` · `x` · `z` never settled — no record at all.
        ]);
        assert_eq!(written_paths(&wf, &outcome), ["one.txt"]);
    }

    /// H7 — a RECOVERED writer is not debt: the task failed, its
    /// `on_error: recover:` arm settled it, and the record's `output` is
    /// the AUTHOR-SUPPLIED fallback value — a path no verb of this run
    /// wrote. Moving it would rename an arbitrary author-named file past
    /// the permits boundary, so the enumeration names NOTHING and the
    /// fold never happens.
    #[test]
    fn a_recovered_writers_output_is_not_debt() {
        let wf = parsed(WF);
        let mut recovered = TaskRecord::unran(TaskStatus::Success, TerminalCause::Recovered);
        recovered.output = serde_json::json!("author-named.txt");
        recovered.recovered_from = Some(nika_runtime::TaskErrorRecord {
            code: "NIKA-BUILTIN-WRITE-001".to_owned(),
            message: "the write failed".to_owned(),
            transient: false,
        });
        let outcome = outcome_with(&[("a", recovered)]);
        assert!(
            written_paths(&wf, &outcome).is_empty(),
            "the recover arm's value is no path this run wrote"
        );
        assert!(
            attend(&wf, &outcome, None).is_none(),
            "no debt, no quarantine fold"
        );
    }

    /// H8 — a CACHE-HIT writer is not debt: a resumed run's hit
    /// rehydrates a PRIOR run's honest output (status Success · cause
    /// Normal — the hit is visible only on `outcome.cache_hits`).
    /// Nothing was written here, so the enumeration names NOTHING.
    #[test]
    fn a_cache_hit_writers_output_is_not_debt() {
        let wf = parsed(WF);
        let mut outcome = outcome_with(&[(
            "a",
            settled(TaskStatus::Success, serde_json::json!("one.txt")),
        )]);
        outcome.cache_hits.push("a".to_owned());
        assert!(
            written_paths(&wf, &outcome).is_empty(),
            "the rehydrated output is the prior run's, not this run's debt"
        );
        assert!(
            attend(&wf, &outcome, None).is_none(),
            "no debt, no quarantine fold"
        );
    }

    /// The lane gate: a COMPLETED run attests nothing even when writers
    /// settled (absent is honest — the no-fake-zero posture); a failed
    /// run with no writer debt attests nothing either. (The paused arm
    /// of the same early-return rides `outcome.paused.is_some()` —
    /// `WorkflowPause` is non-exhaustive and unconstructible here; the
    /// clause is the same boolean.)
    #[test]
    fn attend_folds_only_for_a_failed_run_with_debt() {
        let wf = parsed(WF);
        let mut ok = outcome_with(&[(
            "a",
            settled(TaskStatus::Success, serde_json::json!("one.txt")),
        )]);
        ok.ok = true;
        assert!(attend(&wf, &ok, None).is_none(), "a clean run: no key");

        let empty = RunOutcome::new(false, BTreeMap::new(), BTreeMap::new());
        assert!(
            attend(&wf, &empty, None).is_none(),
            "a failed run with no debt: no key"
        );
    }

    /// The moves: files land under the stamp dir named by file name; a
    /// shared file name takes the deterministic `--<n>` suffix; the fold
    /// states every entry.
    #[test]
    fn the_moves_land_under_the_stamp_dir_with_collision_suffixes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("q");
        let first = tmp.path().join("out.txt");
        let second = tmp.path().join("nested").join("out.txt");
        std::fs::create_dir_all(second.parent().expect("parent")).expect("mkdir");
        std::fs::write(&first, "first").expect("write");
        std::fs::write(&second, "second").expect("write");

        let paths = vec![
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ];
        let fold = quarantine(&paths, &dir);

        assert!(
            !first.exists() && !second.exists(),
            "the old paths are gone"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("out.txt")).expect("moved"),
            "first"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("out--2.txt")).expect("suffixed"),
            "second"
        );
        assert_eq!(fold["outputs"][0]["path"], serde_json::json!(paths[0]));
        assert_eq!(
            fold["outputs"][0]["quarantined_to"],
            serde_json::json!(dir.join("out.txt").to_string_lossy())
        );
        assert_eq!(
            fold["outputs"][1]["quarantined_to"],
            serde_json::json!(dir.join("out--2.txt").to_string_lossy())
        );
        assert_eq!(
            fold["dir"],
            serde_json::json!(dir.to_string_lossy()),
            "the fold names its dir"
        );
    }

    /// The stated miss: a move that fails (the source is gone) rides as
    /// `{path, error, action}` — never silent, never a panic, and the
    /// sibling entries still move.
    #[test]
    fn a_failed_move_is_a_stated_miss_never_fatal() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("q");
        let real = tmp.path().join("real.txt");
        std::fs::write(&real, "here").expect("write");
        let ghost = tmp.path().join("ghost.txt");

        let paths = vec![
            ghost.to_string_lossy().into_owned(),
            real.to_string_lossy().into_owned(),
        ];
        let fold = quarantine(&paths, &dir);

        assert_eq!(
            fold["outputs"][0]["action"],
            serde_json::json!("left_in_place"),
            "the miss is stated: {fold}"
        );
        assert!(
            fold["outputs"][0]["error"]
                .as_str()
                .is_some_and(|e| !e.is_empty()),
            "the miss names WHY: {fold}"
        );
        assert!(
            fold["outputs"][0].get("quarantined_to").is_none(),
            "a miss never fakes a target: {fold}"
        );
        assert_eq!(
            fold["outputs"][1]["quarantined_to"],
            serde_json::json!(dir.join("real.txt").to_string_lossy()),
            "the sibling still moved"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("real.txt")).expect("moved"),
            "here"
        );
    }

    /// The stamp: the journal's file stem when one opened; the fallback
    /// mint keeps the journal's `<ISO-compact>-<tail>` shape.
    #[test]
    fn the_run_stamp_prefers_the_journal_stem() {
        let journal = Path::new(".nika/traces/2026-07-29T13-40-01Z-a3f2.ndjson");
        assert_eq!(
            run_stamp(Some(journal)),
            "2026-07-29T13-40-01Z-a3f2",
            "the debt dir ties to the journal that attests it"
        );
        let minted = mint_stamp();
        let (ts, short) = minted.rsplit_once('-').expect("the shape holds");
        assert_eq!(short.len(), 4, "the uuid tail: {minted}");
        assert!(short.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(
            ts.ends_with('Z') && !ts.contains(':'),
            "path-safe: {minted}"
        );
    }
}
