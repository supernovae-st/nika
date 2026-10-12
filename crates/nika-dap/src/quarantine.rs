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
//! The effect: a file a Success-settled `nika:write` / `nika:edit` task
//! CREATED in this run MOVES under `.nika/quarantine/<run-stamp>/`
//! ([`crate::store::QUARANTINE_DIR`] — beside the journals, never inside
//! them): the old path no longer exists, so a next run reading it fails
//! LOUD (`NIKA-BUILTIN-READ-001`) instead of silently consuming a
//! semi-written artifact. A file that EXISTED before the run is a
//! person's own and never moves: moving an edited `notes.md` broke every
//! later run that read or edited it. Its original bytes, copied aside
//! before the run first wrote it (the host's write-origins ledger), move
//! under `<run-stamp>/originals/` so it can be restored. A path whose
//! origin the ledger cannot name stays in place: moving nothing is the
//! data-safe default. The fold also lands as `<run-stamp>/manifest.json`,
//! which a resumed run reads to put the moved files back. The attestation
//! rides the receipt surface: the fold this module returns lands in the
//! terminal `run_sealed` line's `covers` via the F-P2 teardown (which
//! proves THAT the end happened; F-P14 says WHAT the end must do).
//!
//! Three Success settle classes are NOT debt — only paths THIS run
//! actually wrote, and not as the record of its failure, are:
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
//! - **failure path** — a writer admitted by an `after:` edge on
//!   `failure`, `terminal` or `unwind` writes BECAUSE something failed
//!   (the error report, the always-written log): its output is the point
//!   of that path, never the debt.
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

use nika_event::settlement::RunState;
use nika_runtime::{RunOutcome, TaskStatus, TerminalCause, WriteOrigin};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask, RawWorkflow};
use nika_schema::types::AfterPredicate;

/// The write-capable builtins whose settled outputs ARE the
/// semi-written debt (v1 scope) — the two that return the path they
/// wrote as their output value.
const WRITER_TOOLS: [&str; 2] = ["nika:write", "nika:edit"];

/// The quarantine effect for one settled run, folded for the seal's
/// `covers` — `None` unless the run settled FAILED (a cancelled run
/// keeps its confirmed effects for resume) AND at least one
/// Success-settled writer/edit task left an output path. `journal` is
/// the run journal's path once the `TraceFileSink` opened one: its
/// file stem IS the run-stamp (the honest run identifier the first
/// event's `UUIDv7` minted — the same stamp the journal file carries),
/// tying the debt dir to the journal whose `run_sealed` line attests
/// it. A disabled/broken journal falls back to a fresh mint in the
/// journal's own naming shape — the debt exists either way; the move
/// never waits on the receipt surface. `origin_of` answers what each
/// path's first write found there (the host's write-origins ledger);
/// only a [`WriteOrigin::Created`] path moves.
#[must_use]
pub fn attend(
    wf: &RawWorkflow,
    outcome: &RunOutcome,
    journal: Option<&Path>,
    origin_of: &dyn Fn(&str) -> Option<WriteOrigin>,
) -> Option<serde_json::Value> {
    attend_at(Path::new(""), wf, outcome, journal, origin_of)
}

/// [`attend`] for a run whose relative paths resolve at `root`: a served
/// run writes under its display root, not the process cwd. The fold and
/// the manifest name paths relative to `root`, as the run wrote them.
#[must_use]
pub fn attend_at(
    root: &Path,
    wf: &RawWorkflow,
    outcome: &RunOutcome,
    journal: Option<&Path>,
    origin_of: &dyn Fn(&str) -> Option<WriteOrigin>,
) -> Option<serde_json::Value> {
    // ADR-128: the typed settlement owns the terminal state. Operator
    // cancellation is not failure; moving its completed writes would leave
    // resume cache hits pointing at files the teardown itself removed.
    if outcome.settlement.state != RunState::Failed {
        return None;
    }
    let paths = written_paths(wf, outcome);
    if paths.is_empty() {
        return None; // no semi-written debt — nothing to attest
    }
    let dir = PathBuf::from(crate::store::QUARANTINE_DIR).join(run_stamp(journal));
    Some(quarantine(root, &paths, &dir, origin_of))
}

/// A writer admitted by an `after:` edge on `failure`, `terminal` or
/// `unwind` writes the record of what failed: its output is spared.
fn on_the_failure_path(task: &RawTask) -> bool {
    task.after.iter().any(|(_, predicate)| {
        matches!(
            predicate.value,
            AfterPredicate::Failure | AfterPredicate::Terminal | AfterPredicate::Unwind
        )
    })
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
/// A writer on the failure path is spared too ([`on_the_failure_path`]).
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
        if !WRITER_TOOLS.contains(&tool.value.as_str()) || on_the_failure_path(&task.value) {
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

/// The manifest a quarantine dir carries: the fold, read back by a
/// resumed run ([`restore`]). An unsigned run records its moves nowhere
/// else.
pub const MANIFEST: &str = "manifest.json";

/// The moves + the fold. Every enumeration entry is STATED. A file the
/// run created moves and rides `{path, quarantined_to}`; a move that
/// failed rides `{path, error, action: "left_in_place"}` — the debt is
/// named, the teardown never aborts. A file that existed stays and rides
/// `{path, action: "kept_in_place", original}` once its staged original
/// moved under `<dir>/originals/` (`original_error` when it could not be
/// kept); a path of unknown origin stays and rides
/// `{path, action: "kept_in_place", origin: "unknown"}`. The name inside a
/// dir is the source's file name, collision-suffixed (`out.txt` → `out--2.txt`)
/// when two paths share one name. The `kept` key and the manifest appear
/// only when they have something to say. Every path, the fold's and the
/// staged originals', is relative to `root`, where the effects land.
fn quarantine(
    root: &Path,
    paths: &[String],
    dir: &Path,
    origin_of: &dyn Fn(&str) -> Option<WriteOrigin>,
) -> serde_json::Value {
    let mut outputs = Vec::new();
    let mut kept = Vec::new();
    let mut taken = BTreeSet::new();
    let mut taken_originals = BTreeSet::new();
    let originals = dir.join("originals");
    for path in paths {
        match origin_of(path) {
            Some(WriteOrigin::Created) => outputs.push(move_aside(root, path, dir, &mut taken)),
            Some(WriteOrigin::Existed { original }) => {
                let place = (root, originals.as_path());
                kept.push(keep_original(place, path, original, &mut taken_originals));
            }
            _ => kept.push(serde_json::json!({
                "path": path,
                "action": "kept_in_place",
                "origin": "unknown",
            })),
        }
    }
    let mut fold = serde_json::json!({
        "dir": dir.to_string_lossy(),
        "outputs": outputs,
    });
    if let Some(map) = fold.as_object_mut()
        && !kept.is_empty()
    {
        map.insert("kept".to_owned(), serde_json::Value::Array(kept));
    }
    if root.join(dir).is_dir() {
        record_manifest(root, dir, &mut fold);
    }
    fold
}

/// Move one file the run created under `dir`; the fold states the move
/// or the miss.
fn move_aside(
    root: &Path,
    path: &str,
    dir: &Path,
    taken: &mut BTreeSet<String>,
) -> serde_json::Value {
    let Some(name) = Path::new(path).file_name() else {
        return serde_json::json!({
            "path": path,
            "error": "the path names no file",
            "action": "left_in_place",
        });
    };
    let target = dir.join(unique_name(&name.to_string_lossy(), taken));
    let moved = std::fs::create_dir_all(root.join(dir))
        .and_then(|()| std::fs::rename(root.join(path), root.join(&target)));
    match moved {
        Ok(()) => serde_json::json!({
            "path": path,
            "quarantined_to": target.to_string_lossy(),
        }),
        Err(e) => serde_json::json!({
            "path": path,
            "error": e.to_string(),
            "action": "left_in_place",
        }),
    }
}

/// A person's file stays where they keep it; its original, staged before
/// the run first wrote it, moves under `originals` (both relative to
/// `root`) so it can be restored.
fn keep_original(
    (root, originals): (&Path, &Path),
    path: &str,
    original: Result<PathBuf, String>,
    taken: &mut BTreeSet<String>,
) -> serde_json::Value {
    let copy = match original {
        Ok(copy) => copy,
        Err(reason) => {
            return serde_json::json!({
                "path": path,
                "action": "kept_in_place",
                "original_error": reason,
            });
        }
    };
    let name = Path::new(path)
        .file_name()
        .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
    let target = originals.join(unique_name(&name, taken));
    let moved = std::fs::create_dir_all(root.join(originals))
        .and_then(|()| std::fs::rename(root.join(&copy), root.join(&target)));
    match moved {
        Ok(()) => serde_json::json!({
            "path": path,
            "action": "kept_in_place",
            "original": target.to_string_lossy(),
        }),
        Err(e) => serde_json::json!({
            "path": path,
            "action": "kept_in_place",
            "original_error": e.to_string(),
        }),
    }
}

/// Write the fold as `<dir>/manifest.json` and name it in the fold (or
/// state why it could not be written).
fn record_manifest(root: &Path, dir: &Path, fold: &mut serde_json::Value) {
    let manifest = dir.join(MANIFEST);
    let written = serde_json::to_vec_pretty(fold)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(root.join(&manifest), bytes).map_err(|e| e.to_string()));
    if let Some(map) = fold.as_object_mut() {
        let (key, value) = match written {
            Ok(()) => ("manifest", serde_json::json!(manifest.to_string_lossy())),
            Err(e) => ("manifest_error", serde_json::json!(e)),
        };
        map.insert(key.to_owned(), value);
    }
}

/// Put back what a prior failed run moved aside, before its resumed run
/// starts: the resumed run's cache hits name those files as written, and
/// a file left in quarantine failed every task that read it. Each
/// `{path, quarantined_to}` of the prior run's manifest returns to `path`
/// when nothing stands there now and both ends stay in their places (the
/// moved file inside the quarantine dir, the path inside the project).
/// `None` when the prior run moved nothing. Paths resolve at the process
/// cwd, the project root the CLI runs in, as the moves did.
#[must_use]
pub fn restore(prior_journal: &Path) -> Option<serde_json::Value> {
    restore_at(Path::new(""), prior_journal)
}

/// [`restore`] for the project at `root`.
fn restore_at(root: &Path, prior_journal: &Path) -> Option<serde_json::Value> {
    let stamp_dir = Path::new(crate::store::QUARANTINE_DIR).join(run_stamp(Some(prior_journal)));
    let dir = root.join(&stamp_dir);
    let bytes = std::fs::read(dir.join(MANIFEST)).ok()?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let restored: Vec<serde_json::Value> = manifest
        .get("outputs")?
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let path = entry.get("path")?.as_str()?;
            let moved = entry.get("quarantined_to")?.as_str()?;
            put_back(root, path, Path::new(moved), &stamp_dir)
        })
        .collect();
    (!restored.is_empty()).then(|| {
        serde_json::json!({
            "dir": stamp_dir.to_string_lossy(),
            "restored": restored,
        })
    })
}

/// Return one moved file to its path, or state why it stays; `None` when
/// it was already put back (a second resume of the same trace). Both are
/// read relative to `root`: the moved file must sit inside the run's
/// quarantine dir, and the path must name a place inside the project.
fn put_back(root: &Path, path: &str, moved: &Path, stamp_dir: &Path) -> Option<serde_json::Value> {
    let plain = |p: &Path| {
        p.is_relative()
            && p.components().all(|c| {
                matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            })
    };
    let target = root.join(path);
    let contained = plain(moved) && moved.starts_with(stamp_dir) && plain(Path::new(path));
    if contained && !root.join(moved).exists() {
        return None;
    }
    let reason = if !contained {
        Some("outside the quarantine dir or the project".to_owned())
    } else if target.exists() || target.is_symlink() {
        Some("something stands at the path".to_owned())
    } else {
        let parent = target.parent().filter(|p| !p.as_os_str().is_empty());
        parent
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::rename(root.join(moved), &target))
            .err()
            .map(|e| e.to_string())
    };
    Some(match reason {
        None => serde_json::json!({ "path": path, "restored_from": moved.to_string_lossy() }),
        Some(reason) => serde_json::json!({
            "path": path,
            "action": "left_quarantined",
            "reason": reason,
        }),
    })
}

/// Remove a settled run's staged originals: a failed run's were moved
/// beside its quarantine, and any other run's writes were the intent.
/// Best effort — a staging dir that cannot be removed holds only copies.
pub fn discard_staging(staging: &Path) {
    let _ = std::fs::remove_dir_all(staging);
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

    use nika_event::settlement::{RunCause, RunSettlement};
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
            // the owner's enum is `#[non_exhaustive]` (ADR-130): a state this
            // fixture does not model settles as an upstream cancellation.
            TaskStatus::Cancelled | _ => TerminalCause::Upstream,
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

    /// No write-origins ledger: every origin is unknown.
    fn no_ledger(_: &str) -> Option<WriteOrigin> {
        None
    }

    /// A ledger that saw the run create every path.
    fn all_created() -> impl Fn(&str) -> Option<WriteOrigin> {
        |_: &str| Some(WriteOrigin::Created)
    }

    /// A writer admitted because something failed (or whatever happened)
    /// writes the record of it: an error report is the point of the
    /// failure path, never its debt.
    #[test]
    fn a_writer_on_the_failure_path_is_spared() {
        let wf = parsed(
            "nika: spared\npermits:\n  tools: [\"nika:write\"]\n  exec: [\"false\"]\n  \
             fs: { write: [\"**\"] }\ntasks:\n  prep:\n    invoke: { tool: \"nika:write\", \
             args: { path: \"prep.txt\", content: \"1\" } }\n  build:\n    exec: { command: \
             [\"false\"] }\n  report:\n    after: { build: failure }\n    invoke: { tool: \
             \"nika:write\", args: { path: \"error.txt\", content: \"x\" } }\n  log:\n    \
             after: { build: terminal }\n    invoke: { tool: \"nika:write\", args: { path: \
             \"log.txt\", content: \"x\" } }\n",
        );
        let outcome = outcome_with(&[
            (
                "prep",
                settled(TaskStatus::Success, serde_json::json!("prep.txt")),
            ),
            (
                "build",
                settled(TaskStatus::Failure, serde_json::Value::Null),
            ),
            (
                "report",
                settled(TaskStatus::Success, serde_json::json!("error.txt")),
            ),
            (
                "log",
                settled(TaskStatus::Success, serde_json::json!("log.txt")),
            ),
        ]);
        assert_eq!(written_paths(&wf, &outcome), ["prep.txt"]);
    }

    /// The person's own file stays where they keep it, edited; its
    /// original moves beside the quarantine; the file the run created
    /// moves; the manifest records the fold.
    #[test]
    fn an_existing_file_stays_and_its_original_is_kept_beside_the_quarantine() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("q");
        let notes = tmp.path().join("notes.md");
        std::fs::write(&notes, "edited by the run").expect("write");
        let staged = tmp.path().join("staging").join("1-notes.md");
        std::fs::create_dir_all(staged.parent().expect("parent")).expect("mkdir");
        std::fs::write(&staged, "my own notes").expect("stage");
        let report = tmp.path().join("report.md");
        std::fs::write(&report, "created by the run").expect("write");
        let notes_path = notes.to_string_lossy().into_owned();
        let report_path = report.to_string_lossy().into_owned();
        let origin_of = |path: &str| {
            if path == notes_path {
                Some(WriteOrigin::Existed {
                    original: Ok(staged.clone()),
                })
            } else {
                (path == report_path).then_some(WriteOrigin::Created)
            }
        };

        let fold = quarantine(
            Path::new(""),
            &[notes_path.clone(), report_path.clone()],
            &dir,
            &origin_of,
        );

        assert_eq!(
            std::fs::read_to_string(&notes).expect("in place"),
            "edited by the run"
        );
        assert!(!report.exists(), "the file the run created moved");
        assert_eq!(
            std::fs::read_to_string(dir.join("report.md")).expect("moved"),
            "created by the run"
        );
        let original = dir.join("originals").join("notes.md");
        assert_eq!(
            std::fs::read_to_string(&original).expect("original kept"),
            "my own notes"
        );
        assert_eq!(fold["kept"][0]["path"], serde_json::json!(notes_path));
        assert_eq!(
            fold["kept"][0]["original"],
            serde_json::json!(original.to_string_lossy())
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(MANIFEST)).expect("manifest"))
                .expect("json");
        assert_eq!(
            manifest["outputs"][0]["path"],
            serde_json::json!(report_path)
        );
        assert_eq!(
            fold["manifest"],
            serde_json::json!(dir.join(MANIFEST).to_string_lossy())
        );
    }

    /// Without a known origin nothing moves: the data-safe default.
    #[test]
    fn a_path_of_unknown_origin_stays_in_place() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("q");
        let file = tmp.path().join("unseen.txt");
        std::fs::write(&file, "x").expect("write");
        let path = file.to_string_lossy().into_owned();

        let fold = quarantine(Path::new(""), std::slice::from_ref(&path), &dir, &no_ledger);

        assert!(file.exists(), "nothing moves without a known origin");
        assert!(!dir.exists(), "no move, no dir, no manifest");
        assert_eq!(
            fold["kept"][0],
            serde_json::json!({ "path": path, "action": "kept_in_place", "origin": "unknown" })
        );
    }

    /// A served run writes under its display root, not the process cwd:
    /// the moves happen under that root, the fold and the manifest keep
    /// the run's relative paths, and a restore at the same root puts the
    /// file back.
    #[test]
    fn a_root_anchored_quarantine_round_trips_through_restore() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("out")).expect("mkdir");
        std::fs::write(root.join("out/report.md"), "half done").expect("write");
        let dir = Path::new(crate::store::QUARANTINE_DIR).join("2026-10-12T11-00-00Z-cd34");

        let fold = quarantine(root, &["out/report.md".to_owned()], &dir, &all_created());

        assert!(!root.join("out/report.md").exists(), "moved under the root");
        assert_eq!(
            fold["outputs"][0]["quarantined_to"],
            serde_json::json!(dir.join("report.md").to_string_lossy()),
            "the fold keeps the run's relative path"
        );
        assert!(
            root.join(&dir).join(MANIFEST).is_file(),
            "the manifest under the root"
        );

        let journal = Path::new(".nika/traces/2026-10-12T11-00-00Z-cd34.ndjson");
        let restored = restore_at(root, journal).expect("something to restore");
        assert_eq!(
            restored["restored"][0]["path"],
            serde_json::json!("out/report.md")
        );
        assert_eq!(
            std::fs::read_to_string(root.join("out/report.md")).expect("back"),
            "half done"
        );
    }

    /// A resumed run gets back what its failed prior run moved aside,
    /// never over something that stands at the path now, and never from
    /// or to a place outside the quarantine dir or the project.
    #[test]
    fn a_resumed_run_gets_its_moved_files_back() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let journal = Path::new(".nika/traces/2026-10-12T10-00-00Z-ab12.ndjson");
        let stamp = Path::new(crate::store::QUARANTINE_DIR).join("2026-10-12T10-00-00Z-ab12");
        std::fs::create_dir_all(root.join(&stamp)).expect("mkdir");
        std::fs::write(root.join(&stamp).join("report.md"), "created").expect("moved");
        std::fs::write(root.join(&stamp).join("taken.md"), "moved").expect("moved");
        std::fs::write(root.join(&stamp).join("escape.md"), "x").expect("moved");
        std::fs::write(root.join("taken.md"), "recreated since").expect("recreated");
        let entry = |path: &str, moved: &str| serde_json::json!({ "path": path, "quarantined_to": stamp.join(moved).to_string_lossy() });
        let manifest = serde_json::json!({ "outputs": [
            entry("out/report.md", "report.md"),
            entry("taken.md", "taken.md"),
            entry("../outside.md", "escape.md"),
            { "path": "hosts.md", "quarantined_to": "/etc/hosts" },
        ] });
        std::fs::write(
            root.join(&stamp).join(MANIFEST),
            serde_json::to_vec(&manifest).expect("json"),
        )
        .expect("manifest");

        let restored = restore_at(root, journal).expect("something to restore");

        assert_eq!(
            std::fs::read_to_string(root.join("out/report.md")).expect("back"),
            "created"
        );
        assert_eq!(
            restored["restored"][0]["path"],
            serde_json::json!("out/report.md")
        );
        assert_eq!(
            std::fs::read_to_string(root.join("taken.md")).expect("untouched"),
            "recreated since"
        );
        for refused in 1..=3 {
            assert_eq!(
                restored["restored"][refused]["action"],
                serde_json::json!("left_quarantined"),
                "{restored}"
            );
        }
        assert!(
            root.join(&stamp).join("escape.md").exists(),
            "never moved out"
        );
        let again = restore_at(root, journal).expect("the refusals still stand");
        assert!(
            again["restored"]
                .as_array()
                .is_some_and(|entries| entries.iter().all(|e| e["path"] != "out/report.md")),
            "a file already put back is not mentioned again: {again}"
        );
        assert!(
            restore_at(root, Path::new(".nika/traces/none.ndjson")).is_none(),
            "no manifest, nothing to restore"
        );
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
        recovered.recovered_from = Some(nika_runtime::TaskErrorRecord::new(
            "NIKA-BUILTIN-WRITE-001",
            "the write failed",
            false,
        ));
        let outcome = outcome_with(&[("a", recovered)]);
        assert!(
            written_paths(&wf, &outcome).is_empty(),
            "the recover arm's value is no path this run wrote"
        );
        assert!(
            attend(&wf, &outcome, None, &no_ledger).is_none(),
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
            attend(&wf, &outcome, None, &no_ledger).is_none(),
            "no debt, no quarantine fold"
        );
    }

    /// The lane gate: a COMPLETED run attests nothing even when writers
    /// settled (absent is honest — the no-fake-zero posture); a failed
    /// run with no writer debt attests nothing either.
    #[test]
    fn attend_folds_only_for_a_failed_run_with_debt() {
        let wf = parsed(WF);
        let mut ok = outcome_with(&[(
            "a",
            settled(TaskStatus::Success, serde_json::json!("one.txt")),
        )]);
        ok = ok.with_settlement(RunSettlement::new(RunState::Succeeded, RunCause::Normal));
        assert!(
            attend(&wf, &ok, None, &no_ledger).is_none(),
            "a clean run: no key"
        );

        let empty = RunOutcome::new(false, BTreeMap::new(), BTreeMap::new());
        assert!(
            attend(&wf, &empty, None, &no_ledger).is_none(),
            "a failed run with no debt: no key"
        );
    }

    /// Cancellation preserves resume checkpoints; actual task, budget and
    /// output-contract failures retain the quarantine obligation.
    #[test]
    fn attend_uses_the_typed_run_state() {
        let wf = parsed(WF);
        for (state, cause, quarantined) in [
            (RunState::Succeeded, RunCause::Normal, false),
            (RunState::Paused, RunCause::HumanGate, false),
            (RunState::Cancelled, RunCause::Operator, false),
            (RunState::Failed, RunCause::TaskFailed, true),
            (RunState::Failed, RunCause::Budget, true),
            (RunState::Failed, RunCause::OutputContract, true),
        ] {
            // A nameless path exercises the gate without moving any file or
            // creating a quarantine directory. The real CLI test owns files.
            let outcome =
                outcome_with(&[("a", settled(TaskStatus::Success, serde_json::json!("/")))])
                    .with_settlement(RunSettlement::new(state, cause));
            assert_eq!(
                attend(&wf, &outcome, None, &no_ledger).is_some(),
                quarantined,
                "quarantine eligibility for {state:?}/{cause:?}"
            );
        }
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
        let fold = quarantine(Path::new(""), &paths, &dir, &all_created());

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
        let fold = quarantine(Path::new(""), &paths, &dir, &all_created());

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
