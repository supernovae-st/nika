// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI transport and explicit materialization for the stateless Compile core.
mod authoring;
#[cfg(feature = "access-harness")]
mod harness_seat;
mod observe;
mod render;
mod sidecar;
mod typesafe;

use crate::output::{VerbOutput, exit};
use nika_onboard::compile::{CompileRequest, CompileStatus, compile, intent_sha256};
use std::io::Write as _;
use std::path::Path;

/// Explicit CLI inputs. No terminal conversation or ambient authoring policy.
#[derive(Debug, clap::Args)]
#[group(id = "compile_options", multiple = true)]
// Four independent CLI flags ARE four bools — the clap-surface idiom
// (same as RunArgs), not a state machine to encode.
#[allow(clippy::struct_excessive_bools)]
pub struct CompileArgs {
    /// Exact skeleton, hello, or bounded support intent; unknown work remains incomplete.
    pub intent: Option<String>,
    /// Write a Ready candidate here; omitted means preview only.
    #[arg(group = "destination")]
    pub dest: Option<String>,
    /// Explicit destination for edit mode (or create without a positional destination).
    #[arg(long, short = 'o', conflicts_with = "dest", group = "destination")]
    pub output: Option<String>,
    /// Explicit accepted source for a conservative edit.
    #[arg(long, requires = "change", conflicts_with = "intent")]
    pub base: Option<String>,
    /// Supported edit: `Set const.NAME to JSON_LITERAL`.
    #[arg(long, requires = "base")]
    pub change: Option<String>,
    /// Answer a stable question: `KEY=JSON_LITERAL` (repeatable).
    #[arg(long = "answer")]
    pub answers: Vec<String>,
    /// Explicitly permit one provider call to interpret free intent (wire generation 2).
    #[arg(long, conflicts_with_all = ["base", "list"])]
    pub authoring_model: Option<String>,
    /// Maximum authoring output tokens; requires explicit authoring model.
    #[arg(long, requires = "authoring_model")]
    pub authoring_max_tokens: Option<u32>,
    /// Authoring timeout in seconds, at most 120; no retries.
    #[arg(long, requires = "authoring_model")]
    pub authoring_timeout: Option<u64>,
    /// HOT admission contract: strict (default), legacy (pre-refactor, ablation) or off (never HOT for prose).
    #[arg(long, value_parser = ["strict", "legacy", "off"])]
    pub hot_policy: Option<String>,
    /// Independent COLD proposals to compare (1..=5); each is one call. Requires the authoring model.
    #[arg(long, requires = "authoring_model")]
    pub authoring_samples: Option<u32>,
    /// When the seat writes the candidate itself (a native `.nika` judged by the parser, the
    /// Check and the fidelity laws): `escalate` (default) after the private plan fails a human,
    /// `only` straight away, `off` never. Requires the authoring model.
    #[arg(long, requires = "authoring_model", value_parser = ["escalate", "only", "off"])]
    pub authoring_strategy: Option<String>,
    /// Repair rounds a native candidate may buy from the compiler's diagnostics (0..=5, default 3).
    #[arg(long, requires = "authoring_model")]
    pub authoring_repairs: Option<u32>,
    /// Explicitly seat one bounded-decision capability (`typesafe/jev-1.13.0` or `provider/name`) for finite ambiguities.
    #[arg(long, conflicts_with_all = ["base", "list"])]
    pub decision_model: Option<String>,
    /// Replace the explicitly named destination.
    #[arg(long, requires = "destination")]
    pub force: bool,
    /// Ignore the plan recorded for this intent (`.nika/compile/<sha256>.plan.json`) and read or
    /// sample it again. An answer round otherwise replays that plan: zero provider calls.
    #[arg(long, conflicts_with_all = ["base", "list"])]
    pub fresh: bool,
    /// Print the versioned structured result, including incomplete questions.
    #[arg(long)]
    pub json: bool,
    /// List exact skeleton names without authoring or writing.
    #[arg(long, conflicts_with_all = ["intent", "dest", "output", "base", "answers"])]
    pub list: bool,
}

/// Compile once; only a Ready result with an explicit destination writes files.
#[must_use]
pub fn run(args: &CompileArgs) -> VerbOutput {
    if args.list {
        return render::listing(args.json);
    }
    let dest = args.dest.as_ref().or(args.output.as_ref());
    if dest.is_some_and(|path| !nika_source::is_canonical_program_path(path)) {
        return render::failure(
            "destination_name",
            "destination must be a canonical *.nika program path",
            exit::FILE,
            args.json,
        );
    }
    let mut request = if let Some(base) = &args.base {
        let source = match std::fs::read_to_string(base) {
            Ok(source) => source,
            Err(error) => {
                return render::failure("read_base", &error.to_string(), exit::ENV, args.json);
            }
        };
        CompileRequest::edit(source, args.change.as_deref().unwrap_or(""))
    } else {
        let mut request = CompileRequest::create(args.intent.as_deref().unwrap_or(""));
        if let Some(dest) = dest {
            request = request.with_workflow_id(workflow_id(dest));
        }
        request
    };
    request = request.with_hot_policy(match args.hot_policy.as_deref() {
        Some("legacy") => nika_onboard::compile::HotPolicy::Legacy,
        Some("off") => nika_onboard::compile::HotPolicy::Off,
        _ => nika_onboard::compile::HotPolicy::Strict,
    });
    for answer in &args.answers {
        let Some((key, literal)) = answer.split_once('=') else {
            return render::failure(
                "invalid_answer",
                "--answer expects KEY=JSON_LITERAL",
                exit::FILE,
                args.json,
            );
        };
        request = request.answer(key, literal);
    }
    let named = matches!(
        args.intent.as_deref().map(str::trim),
        Some("hello" | "01-hello")
    ) || nika_pack::template_names()
        .iter()
        .any(|name| Some(name.as_str()) == args.intent.as_deref().map(str::trim));
    let cognition = (args.authoring_model.is_some() || args.decision_model.is_some())
        && args.base.is_none()
        && !named;
    // An authoring seat reads the shape of the files the request names (a header, a key set,
    // a categorical column's values — never a row), observed here under the working directory.
    if cognition
        && let Some(intent) = args.intent.as_deref()
        && let Ok(cwd) = std::env::current_dir()
        && let Some(world) = observe::world(&cwd, intent)
    {
        request = request.with_knowledge(world);
    }
    // Free intents only: a skeleton, hello or an edit never produces a plan to record.
    let sha =
        (args.base.is_none() && !named).then(|| intent_sha256(&effective_intent(args, cognition)));
    let (request, note) = sidecar::replay(sha.as_deref(), args, request);
    let result = if cognition {
        authoring::compile(&request, args)
    } else {
        compile(&request).map_err(|error| error.to_string())
    };
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            return render::failure("compile_error", &error, exit::ENV, args.json);
        }
    };
    let note = sidecar::keep(sha.as_deref(), note, &outcome);
    let mut written = None;
    if outcome.status == CompileStatus::Ready
        && let (Some(dest), Some(candidate)) = (dest, &outcome.candidate)
    {
        if let Err(error) = materialize(Path::new(dest), candidate, args.force) {
            return render::failure("destination", &error.to_string(), exit::ENV, args.json);
        }
        written = Some(dest.as_str());
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::DraftCreated,
            crate::metrics::Facts {
                draft: Some(crate::metrics::DraftSource::Compile),
                ..crate::metrics::Facts::none()
            },
        );
    }
    render::outcome(&outcome, written, note.as_ref(), args.json)
}

/// The intent the compiler will actually read, as the sha key must see it: the
/// `intent.clarification` answer replaces the intent, but only through the cognition
/// door, which is the only door that consumes that answer.
fn effective_intent(args: &CompileArgs, cognition: bool) -> String {
    let intent = args.intent.clone().unwrap_or_default();
    if !cognition {
        return intent;
    }
    args.answers
        .iter()
        .filter_map(|answer| answer.split_once('='))
        .filter(|(key, _)| *key == "intent.clarification")
        .filter_map(|(_, literal)| serde_json::from_str::<serde_json::Value>(literal).ok())
        .filter_map(|value| value.as_str().map(str::to_owned))
        .find(|text| !text.trim().is_empty())
        .unwrap_or(intent)
}

fn workflow_id(dest: &str) -> String {
    let name = Path::new(dest)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workflow");
    let stem = nika_source::program_stem(name).unwrap_or(name);
    let id: String = stem
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let id = id.trim_matches('-');
    if id.is_empty() {
        "workflow".to_owned()
    } else if id.starts_with(|c: char| c.is_ascii_digit()) {
        format!("workflow-{id}")
    } else {
        id.to_owned()
    }
}

fn materialize(dest: &Path, candidate: &str, force: bool) -> std::io::Result<()> {
    let conflict = || {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "destination exists; pass --force to overwrite",
        )
    };
    // Refuse ordinary conflicts before touching even the trace-protection file.
    if !force && dest.symlink_metadata().is_ok() {
        return Err(conflict());
    }
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // A destination in a directory that does not exist yet is an ordinary request
    // (`nika compile hello out/hello.nika`), not a raw `os error 2` on a temp path.
    std::fs::create_dir_all(parent)?;
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    pending.write_all(candidate.as_bytes())?;
    pending.as_file().sync_all()?;
    // A protection failure is reported BEFORE destination publication. An existing
    // destination remains byte-identical; the temporary candidate is dropped.
    nika_onboard::project_file::protect_traces(parent)?;
    let result = if force {
        pending.persist(dest)
    } else {
        pending.persist_noclobber(dest)
    };
    result.map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            conflict()
        } else {
            e.error
        }
    })?;
    Ok(())
}
