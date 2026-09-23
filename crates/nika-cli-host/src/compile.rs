// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI transport and explicit materialization for the stateless Compile core.
mod authoring;
#[cfg(feature = "access-harness")]
mod harness_seat;
mod knowledge;
mod observe;
mod render;
mod sidecar;
mod typesafe;

use crate::output::{VerbOutput, exit};
use nika_onboard::compile::{CompileRequest, CompileStatus, compile, intent_sha256, revise_intent};
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
    #[arg(long, requires = "change")]
    pub base: Option<String>,
    /// Supported edit: `Set const.NAME to JSON_LITERAL`.
    #[arg(long, requires = "base")]
    pub change: Option<String>,
    /// Answer a stable question: `KEY=JSON_LITERAL` (repeatable).
    #[arg(long = "answer")]
    pub answers: Vec<String>,
    /// Explicitly permit one provider call to interpret free intent (wire generation 2).
    #[arg(long, conflicts_with = "list")]
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
    /// `only` straight away, `sketch` (structure first: the seat sketches tasks, edges and gates,
    /// then fills typed holes, the compiler emits the file and derives the permits), `off`
    /// never. Requires the authoring model.
    #[arg(long, requires = "authoring_model", value_parser = ["escalate", "only", "sketch", "off"])]
    pub authoring_strategy: Option<String>,
    /// Repair rounds a native candidate may buy from the compiler's diagnostics (0..=5, default 3).
    #[arg(long, requires = "authoring_model")]
    pub authoring_repairs: Option<u32>,
    /// A knowledge snapshot directory (manifest.json · one JSONL per kind · relations.jsonl): the seat
    /// reads the pack composed for this intent beside the card; the provenance names the snapshot.
    /// `NIKA_KNOWLEDGE` in the environment names one when the flag is absent.
    #[arg(long, requires = "authoring_model")]
    pub knowledge: Option<std::path::PathBuf>,
    /// A corpus whose examples the knowledge door never recalls (a benchmark's own);
    /// `NIKA_KNOWLEDGE_EXCLUDE` in the environment names one when the flag is absent.
    #[arg(long, requires = "knowledge")]
    pub knowledge_exclude: Option<String>,
    /// A pack another builder composed for THIS intent (JSON: `identity` · `selection` ·
    /// `references: [{kind, id, text}]` · `repairs: {code: [strategy]}`): it enters the door as
    /// composed, identity and selection recorded verbatim, and wins over `--knowledge`.
    /// `NIKA_KNOWLEDGE_PACK` in the environment names one when the flag is absent.
    #[arg(long, requires = "authoring_model", conflicts_with = "knowledge")]
    pub knowledge_pack: Option<std::path::PathBuf>,
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
    let mut request = match build_request(args, dest) {
        Ok(request) => request,
        Err(failure) => return failure,
    };
    let named = matches!(
        args.intent.as_deref().map(str::trim),
        Some("hello" | "01-hello")
    ) || nika_pack::template_names()
        .iter()
        .any(|name| Some(name.as_str()) == args.intent.as_deref().map(str::trim));
    let cognition = (args.authoring_model.is_some() || args.decision_model.is_some()) && !named;
    if cognition {
        request = observed_world(args, request);
    }
    // The knowledge door: the snapshot the flag or the environment names, its pack for this
    // intent composed here and stated to the seat beside the card.
    if cognition && args.authoring_model.is_some() {
        request = knowledge_door(args, request);
    }
    // Free intents and revisions in words carry a record (a creation's plan, a revision's
    // native candidate); a skeleton, hello or a structured edit never does.
    let sha = (!named).then(|| match revise_intent(&request) {
        Some(intent) => intent_sha256(&intent),
        None => intent_sha256(&effective_intent(args, cognition)),
    });
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

/// The request the arguments state: an edit of the base (with the original intent beside it
/// when stated) or a creation (named after its destination), the HOT policy, the answers.
fn build_request(args: &CompileArgs, dest: Option<&String>) -> Result<CompileRequest, VerbOutput> {
    let mut request = if let Some(base) = &args.base {
        let source = match std::fs::read_to_string(base) {
            Ok(source) => source,
            Err(error) => {
                return Err(render::failure(
                    "read_base",
                    &error.to_string(),
                    exit::ENV,
                    args.json,
                ));
            }
        };
        let request = CompileRequest::edit(source, args.change.as_deref().unwrap_or(""));
        match args
            .intent
            .as_deref()
            .map(str::trim)
            .filter(|i| !i.is_empty())
        {
            // The intent beside a base is the request the base answered: the seat revises
            // against the whole meaning, never against the change alone.
            Some(original) => request.with_original_intent(original),
            None => request,
        }
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
            return Err(render::failure(
                "invalid_answer",
                "--answer expects KEY=JSON_LITERAL",
                exit::FILE,
                args.json,
            ));
        };
        request = request.answer(key, literal);
    }
    Ok(request)
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

/// An authoring seat reads the shape of the files the request names (a header, a key set, a
/// categorical column's values — never a row), observed under the working directory.
fn observed_world(args: &CompileArgs, request: CompileRequest) -> CompileRequest {
    match (args.intent.as_deref(), std::env::current_dir()) {
        (Some(intent), Ok(cwd)) => match observe::world(&cwd, intent) {
            Some(world) => request.with_knowledge(world),
            None => request,
        },
        _ => request,
    }
}

/// The knowledge door: a pre-composed pack the flag or `NIKA_KNOWLEDGE_PACK` names wins; else
/// the snapshot the flag or `NIKA_KNOWLEDGE` names (a directory, not a
/// secret) and the corpus the flag or `NIKA_KNOWLEDGE_EXCLUDE` names; the pack composed for the
/// intent rides the request, the provenance names the snapshot and the selection.
#[allow(clippy::disallowed_methods)] // a snapshot directory and a corpus name, NON-secret
fn knowledge_door(args: &CompileArgs, request: CompileRequest) -> CompileRequest {
    let pack = args
        .knowledge_pack
        .clone()
        .or_else(|| std::env::var_os("NIKA_KNOWLEDGE_PACK").map(std::path::PathBuf::from));
    if let Some(path) = pack.as_deref() {
        return match knowledge::pack_from_file(path) {
            Some(pack) => request.with_authoring_knowledge(pack),
            None => request,
        };
    }
    let dir = args
        .knowledge
        .clone()
        .or_else(|| std::env::var_os("NIKA_KNOWLEDGE").map(std::path::PathBuf::from));
    let exclude = args
        .knowledge_exclude
        .clone()
        .or_else(|| std::env::var("NIKA_KNOWLEDGE_EXCLUDE").ok());
    let (Some(intent), Some(dir)) = (args.intent.as_deref(), dir.as_deref()) else {
        return request;
    };
    match knowledge::Snapshot::open(dir) {
        Some(snapshot) => {
            request.with_authoring_knowledge(snapshot.pack(intent, exclude.as_deref()))
        }
        None => request,
    }
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
