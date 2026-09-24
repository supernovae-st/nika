// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI transport and explicit materialization for the stateless Compile core.
mod authoring;
pub mod config;
#[cfg(feature = "access-harness")]
mod harness_seat;
pub mod knowledge;
mod observe;
mod render;
mod sidecar;
pub mod typesafe;

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
    /// Timeout of each authoring call in seconds (the private plan's and every native call
    /// alike): 120 by default, 300 for a harness seat, at most 600; a call is never retried.
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
    /// A corpus whose examples the knowledge door never recalls (a benchmark's own), for the
    /// snapshot `--knowledge` or `NIKA_KNOWLEDGE` names; refused when neither names one.
    /// `NIKA_KNOWLEDGE_EXCLUDE` in the environment names one when the flag is absent.
    #[arg(long, requires = "authoring_model")]
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
    // The authoring configuration (the strategy · the knowledge source): the flags over the
    // environment, through the parser every door shares; read only when an authoring seat is
    // named — the deterministic door never reads it.
    let authoring_config = if cognition && args.authoring_model.is_some() {
        match config::resolve(
            &explicit_settings(args),
            &config::AuthoringSettings::from_env(),
        ) {
            Ok(config) => Some(config),
            Err(error) => {
                return render::failure(
                    "authoring_config",
                    &error.to_string(),
                    exit::ENV,
                    args.json,
                );
            }
        }
    } else {
        None
    };
    if cognition {
        request = observed_world(args, request);
    }
    // The knowledge door: the source the configuration names, its pack for the intent the
    // compiler reads composed here and stated to the seat beside the card; a source that
    // cannot be honored refuses, never a silent card alone.
    if let Some(config) = &authoring_config {
        request = match knowledge_door(config, args, request) {
            Ok(request) => request,
            Err(error) => {
                return render::failure("knowledge", &error.to_string(), exit::ENV, args.json);
            }
        };
    }
    // Free intents and revisions in words carry a record (a creation's plan, a revision's
    // native candidate); a skeleton, hello or a structured edit never does.
    let sha = (!named).then(|| match revise_intent(&request) {
        Some(intent) => intent_sha256(&intent),
        None => intent_sha256(&effective_intent(args, cognition)),
    });
    let (request, note) = sidecar::replay(sha.as_deref(), args, request);
    let result = if cognition {
        let strategy = authoring_config
            .as_ref()
            .map_or(config::DEFAULT_STRATEGY, |config| config.strategy);
        authoring::compile(&request, args, strategy)
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

/// The door's own explicit words: the strategy, the snapshot, the pack, and the excluded corpus —
/// the exclusion held whichever side names the snapshot (a held-out corpus named on the command
/// line guards the snapshot `NIKA_KNOWLEDGE` names too).
fn explicit_settings(args: &CompileArgs) -> config::AuthoringSettings {
    let mut settings = config::AuthoringSettings::none();
    if let Some(word) = &args.authoring_strategy {
        settings = settings.with_strategy(word.clone());
    }
    if let Some(dir) = &args.knowledge {
        settings = settings.with_knowledge(dir.clone(), None);
    }
    if let Some(file) = &args.knowledge_pack {
        settings = settings.with_knowledge_pack(file.clone());
    }
    if let Some(corpus) = &args.knowledge_exclude {
        settings = settings.with_knowledge_exclude(corpus.clone());
    }
    settings
}

/// The knowledge door: a pre-composed pack enters as composed (an empty one carries no
/// knowledge); a snapshot composes the pack for the intent the compiler reads (a revision's
/// request with its change, a clarification's replacement), every presented byte verified
/// against the snapshot's manifest. The provenance names the snapshot and the selection.
///
/// # Errors
/// A pack that is not a pack, a directory that is not a snapshot, a stale snapshot.
fn knowledge_door(
    config: &config::AuthoringConfig,
    args: &CompileArgs,
    request: CompileRequest,
) -> Result<CompileRequest, knowledge::KnowledgeError> {
    match &config.knowledge {
        None => Ok(request),
        Some(config::KnowledgeSource::Pack { file }) => {
            Ok(match knowledge::pack_from_file(file)? {
                Some(pack) => request.with_authoring_knowledge(pack),
                None => request,
            })
        }
        Some(config::KnowledgeSource::Snapshot {
            dir,
            exclude_corpus,
        }) => {
            let intent = revise_intent(&request).unwrap_or_else(|| effective_intent(args, true));
            if intent.trim().is_empty() {
                return Ok(request);
            }
            let pack = knowledge::Snapshot::open(dir)?.pack(&intent, exclude_corpus.as_deref())?;
            Ok(request.with_authoring_knowledge(pack))
        }
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use clap::Parser as _;

    #[derive(clap::Parser)]
    struct Door {
        #[command(flatten)]
        args: CompileArgs,
    }

    fn parse(argv: &[&str]) -> CompileArgs {
        Door::try_parse_from(std::iter::once("compile").chain(argv.iter().copied()))
            .expect("the door parses")
            .args
    }

    /// A held-out corpus named on the command line guards the snapshot the environment names:
    /// the door's own resolver carries it whatever side names the snapshot, and refuses it when
    /// no snapshot is named at all — never a silent, unguarded evaluation.
    #[test]
    fn an_explicit_exclusion_reaches_the_environments_snapshot_through_the_door() {
        let args = parse(&[
            "Read ./a.md and do something clever with it, then write ./b.md",
            "--authoring-model",
            "mock/echo",
            "--knowledge-exclude",
            "heldout",
        ]);
        assert_eq!(args.knowledge, None, "no snapshot flag");
        let env = config::AuthoringSettings::none().with_knowledge("/env/snapshot", None);
        let resolved = config::resolve(&explicit_settings(&args), &env).expect("resolves");
        assert_eq!(
            resolved.knowledge,
            Some(config::KnowledgeSource::Snapshot {
                dir: std::path::PathBuf::from("/env/snapshot"),
                exclude_corpus: Some("heldout".to_owned()),
            })
        );
        assert_eq!(resolved.strategy, config::DEFAULT_STRATEGY);
        // The flag's snapshot keeps it the same way.
        let args = parse(&[
            "x",
            "--authoring-model",
            "mock/echo",
            "--knowledge",
            "/flag/snapshot",
            "--knowledge-exclude",
            "heldout",
            "--authoring-strategy",
            "only",
        ]);
        let resolved = config::resolve(
            &explicit_settings(&args),
            &config::AuthoringSettings::none(),
        )
        .expect("resolves");
        assert_eq!(
            resolved.knowledge,
            Some(config::KnowledgeSource::Snapshot {
                dir: std::path::PathBuf::from("/flag/snapshot"),
                exclude_corpus: Some("heldout".to_owned()),
            })
        );
        assert_eq!(resolved.strategy, nika_onboard::compile::NativeMode::Only);
        // No snapshot anywhere: the exclusion is refused, not dropped.
        let args = parse(&[
            "x",
            "--authoring-model",
            "mock/echo",
            "--knowledge-exclude",
            "heldout",
        ]);
        assert_eq!(
            config::resolve(
                &explicit_settings(&args),
                &config::AuthoringSettings::none()
            ),
            Err(config::ConfigError::ExclusionWithoutSnapshot {
                corpus: "heldout".to_owned()
            })
        );
        // The exclusion still needs an authoring seat, as every knowledge flag does.
        assert!(Door::try_parse_from(["compile", "x", "--knowledge-exclude", "heldout"]).is_err());
    }
}
