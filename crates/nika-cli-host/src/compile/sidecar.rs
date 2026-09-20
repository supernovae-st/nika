// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The recorded plan beside the working directory: `.nika/compile/<intent sha256>.plan.json`.
//!
//! One authoring conversation is several `nika compile` invocations of the same intent
//! with more `--answer` each round. Without a record, every round reads (or samples) the
//! intent again, may settle on a different plan, and pays a provider call for nothing.
//! The record keeps the plan the first round produced; an answer round replays it through
//! `CompileRequest::with_plan` (zero provider calls, the same candidate). It carries the
//! plan, the engine that produced it and the intent hash it belongs to: never the
//! candidate, never a key. The directory ignores itself (the cache-directory convention),
//! so the request's verbatim excerpts cannot enter a commit by accident.

use nika_onboard::compile::{COMPILE_WIRE_VERSION, CompileOutcome, CompileRequest, Strategy};
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The directory, relative to the working directory the operator compiles from.
pub(super) const DIR: &str = ".nika/compile";

/// What this invocation did with the record; the human text names it, the machine
/// document only carries a failure (the core's `route` already says `replayed plan`).
pub(super) enum Note {
    /// A fresh compile produced a plan and this invocation recorded it here.
    Recorded(PathBuf),
    /// This invocation replayed the plan recorded here.
    Replayed(PathBuf),
    /// A plan was produced but could not be recorded; the compile itself stands.
    Failed { path: PathBuf, error: String },
}

pub(super) fn path(sha: &str) -> PathBuf {
    Path::new(DIR).join(format!("{sha}.plan.json"))
}

/// An answer round (one or more `--answer`, not `--fresh`) attaches the record for this
/// intent to the request; any other round leaves the request as it is.
pub(super) fn replay(
    sha: Option<&str>,
    args: &super::CompileArgs,
    request: CompileRequest,
) -> (CompileRequest, Option<Note>) {
    if let Some(sha) = sha
        && !args.answers.is_empty()
        && !args.fresh
        && let Some(plan) = load(sha)
    {
        return (request.with_plan(plan), Some(Note::Replayed(path(sha))));
    }
    (request, None)
}

/// After the compile: a replay keeps its note and rewrites nothing; a fresh compile that
/// settled a plan records it; anything else records nothing.
pub(super) fn keep(sha: Option<&str>, note: Option<Note>, out: &CompileOutcome) -> Option<Note> {
    if note.is_some() {
        return note;
    }
    let sha = sha.filter(|_| recordable(out))?;
    Some(match record(sha, out) {
        Ok(path) => Note::Recorded(path),
        Err(error) => Note::Failed {
            path: path(sha),
            error: error.to_string(),
        },
    })
}

/// The plan recorded for this intent by THIS engine, with its strategy word riding
/// inside, or nothing: an absent, unreadable, foreign-engine or foreign-intent record is
/// simply not replayed, and the fresh compile that follows rewrites it.
pub(super) fn load(sha: &str) -> Option<Value> {
    let text = std::fs::read_to_string(path(sha)).ok()?;
    let record: Value = serde_json::from_str(&text).ok()?;
    if record.get("compile_version") != Some(&json!(COMPILE_WIRE_VERSION))
        || record.get("engine").and_then(Value::as_str) != Some(env!("CARGO_PKG_VERSION"))
        || record.get("intent_sha256").and_then(Value::as_str) != Some(sha)
    {
        return None;
    }
    let mut plan = record.get("plan").filter(|plan| plan.is_object())?.clone();
    if plan.get("strategy").is_none()
        && let Some(strategy) = record.get("strategy").and_then(Value::as_str)
    {
        plan["strategy"] = json!(strategy);
    }
    Some(plan)
}

/// Only a settled general-path plan is worth replaying: skeletons and the support
/// grammar have none, and a plan the compiler refused to assemble must not be assembled
/// on the next round either.
pub(super) fn recordable(out: &CompileOutcome) -> bool {
    out.provenance.plan.is_some()
        && matches!(
            out.provenance.strategy,
            Some(Strategy::Hot | Strategy::Warm | Strategy::Cold)
        )
}

/// Record the outcome's plan for this intent, atomically, in a self-ignoring directory.
pub(super) fn record(sha: &str, out: &CompileOutcome) -> std::io::Result<PathBuf> {
    let dir = Path::new(DIR);
    std::fs::create_dir_all(dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n")?;
    }
    let record = json!({
        "compile_version": COMPILE_WIRE_VERSION,
        "engine": out.provenance.compiler_version,
        "intent_sha256": sha,
        "plan": out.provenance.plan,
        "strategy": out.provenance.strategy.map(Strategy::word),
        "created_at": jiff::Timestamp::now().to_string(),
    });
    let target = path(sha);
    let mut pending = tempfile::NamedTempFile::new_in(dir)?;
    pending.write_all(serde_json::to_string_pretty(&record)?.as_bytes())?;
    pending.write_all(b"\n")?;
    pending.as_file().sync_all()?;
    pending.persist(&target).map_err(|error| error.error)?;
    Ok(target)
}
