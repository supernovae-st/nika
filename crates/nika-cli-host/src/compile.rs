// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI transport and explicit materialization for the stateless Compile core.
mod authoring;
pub use authoring::decision_seat_note;
mod capture;
pub use capture::{
    Capture, CaptureContext, CaptureFlags, CaptureListener, CapturePolicy, CaptureReport,
    CaptureState, CaptureStatus, TextAdmission,
};
mod authority;
pub use authority::{
    authoring_backend, authoring_host, authoring_http, authoring_http_with_deadline,
    redact_authoring_error,
};
pub mod config;
pub mod knowledge;
pub mod observe;
mod render;
mod sidecar;
pub use crate::serve_args::NativeAuthoringArgs;

pub mod typesafe;

use crate::output::{VerbOutput, exit};
use nika_onboard::compile::{
    CompileOutcome, CompileRequest, CompileStatus, compile, intent_sha256, revise_intent,
};
use std::io::Write as _;
use std::path::Path;

/// Explicit CLI inputs. No terminal conversation or ambient authoring policy.
#[derive(Debug, clap::Args)]
#[group(id = "compile_options", multiple = true)]
// A reasoning effort needs a seat to ask it: the authoring model, the decision model or both.
#[command(group(
    clap::ArgGroup::new("seat").args(["authoring_model", "decision_model"]).multiple(true)
))]
// Independent CLI flags ARE bools — the clap-surface idiom
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
    /// Seat one authoring model to interpret free intent (wire generation 2). There is no
    /// request count by default; `--authoring-max-calls` sets one explicitly. An ACP harness
    /// counts invocations, with its own requests unknown.
    #[arg(long, conflicts_with = "list")]
    pub authoring_model: Option<String>,
    /// Maximum output tokens per completion; defaults to the selected route's technical capacity.
    /// Requires an explicit authoring model.
    #[arg(long, requires = "authoring_model")]
    pub authoring_max_tokens: Option<u32>,
    /// Timeout of each authoring call in seconds (the private plan's and every native call
    /// alike): the route default, else any positive value. This is a transport deadline, not a
    /// limit on the whole creation. The transport never retries a request on its own.
    #[arg(long, requires = "authoring_model")]
    pub authoring_timeout: Option<u64>,
    /// HOT admission contract: strict (default), legacy (pre-refactor, ablation) or off (never HOT for prose).
    #[arg(long, value_parser = ["strict", "legacy", "off"])]
    pub hot_policy: Option<String>,
    /// Independent COLD proposals to compare (1..=5); each is one call, within
    /// `--authoring-max-calls`. Requires the authoring model.
    #[arg(long, requires = "authoring_model", value_parser = clap::value_parser!(u32).range(1..=5))]
    pub authoring_samples: Option<u32>,
    /// When the seat writes the candidate itself (a native `.nika` judged by the parser, the
    /// Check and the fidelity laws): `escalate` (default) after the private plan fails a human,
    /// `only` straight away, `sketch` (structure first: the seat sketches tasks, edges and gates,
    /// then fills typed holes, the compiler emits the file and derives the permits), `off`
    /// never. Requires the authoring model.
    #[arg(long, requires = "authoring_model", value_parser = ["escalate", "only", "sketch", "off"])]
    pub authoring_strategy: Option<String>,
    /// Optional repair-round limit for a native candidate (0 disables repairs). Absent, there
    /// is no repair count; creation ends on a result, failure, no progress or cancellation.
    #[arg(long, requires = "authoring_model")]
    pub authoring_repairs: Option<u32>,
    /// The reasoning effort every authoring and decision call asks (low · high · max), sent only
    /// where the route qualifies it; `NIKA_AUTHORING_REASONING` names one when the flag is absent.
    #[arg(long, requires = "seat")]
    pub authoring_reasoning: Option<String>,
    /// A Foundry knowledge release root, admitted by the strict door against a trusted identity
    /// or refused whole; a flag carries no identity, so it is refused until one is wired.
    /// `NIKA_KNOWLEDGE` in the environment names one when the flag is absent (`off`: none).
    #[arg(long, requires = "authoring_model")]
    pub knowledge: Option<std::path::PathBuf>,
    /// A corpus whose examples the knowledge door never recalls (a benchmark's own), for the
    /// release `--knowledge` or `NIKA_KNOWLEDGE` names, else the embedded one; refused where no
    /// release is read. `NIKA_KNOWLEDGE_EXCLUDE` in the environment names one without the flag.
    #[arg(long, requires = "authoring_model")]
    pub knowledge_exclude: Option<String>,
    /// A pack another builder composed for one intent: refused, since the knowledge door enters
    /// only an admitted release (`--knowledge`), never a pack bound to none.
    /// `NIKA_KNOWLEDGE_PACK` in the environment names one when the flag is absent.
    #[arg(long, requires = "authoring_model", conflicts_with = "knowledge")]
    pub knowledge_pack: Option<std::path::PathBuf>,
    /// Turn the knowledge off for this compile whatever the environment names (the door's own
    /// words win); refused beside `--knowledge` or `--knowledge-pack`.
    #[arg(long, requires = "authoring_model")]
    pub no_knowledge: bool,
    /// Explicitly seat one bounded-decision capability (`typesafe/jev-1.13.0` or `provider/name`) for finite ambiguities.
    /// Its requests ride its own client, outside `--authoring-max-calls`: a `typesafe/<jev>` question is sent once
    /// (no protocol retry); a `provider/name` seat keeps its client's protocol retries.
    #[arg(long, conflicts_with = "list")]
    pub decision_model: Option<String>,
    /// Replace the explicitly named destination.
    #[arg(long, requires = "destination")]
    pub force: bool,
    /// Ignore the plan recorded for this intent (`.nika/compile/<sha256>.plan.json`) and read or
    /// sample it again. An answer round otherwise replays that plan: no authoring call (a judge
    /// the round asks makes its own calls). The verdicts kept beside it
    /// (`<sha256>.declined.json`) still apply: a judge is never asked again on bytes it rejected.
    #[arg(long, conflicts_with_all = ["base", "list"])]
    pub fresh: bool,
    /// Print the versioned structured result, including incomplete questions.
    #[arg(long)]
    pub json: bool,
    /// List exact skeleton names without authoring or writing.
    #[arg(long, conflicts_with_all = ["intent", "dest", "output", "base", "answers"])]
    pub list: bool,
}

/// `--observe-only`, beside the compile flags (their literal is unchanged).
#[derive(Clone, Copy, Debug, Default, clap::Args)]
#[non_exhaustive]
pub struct ObserveFlags {
    /// Print only what this host observes of the files the intent states (what a seat reads,
    /// never a row) and their text for a remote trial, as one JSON document; nothing else runs.
    #[arg(
        long,
        requires = "intent",
        conflicts_with_all = ["dest", "output", "base", "answers", "authoring_model", "decision_model", "list", "fresh", "force"]
    )]
    pub observe_only: bool,
}

/// The authoring authority a CLI compile runs under: how many requests the authoring seat may
/// be sent. Absent, requests are observed without a count limit.
#[derive(Clone, Debug, Default, clap::Args)]
#[non_exhaustive]
pub struct AuthoringAuthority {
    /// Optional limit on authoring requests this compile may send: the plan and its evidence repair,
    /// the native candidate and its repairs alike, counted where they leave (an ACP harness: its
    /// invocations). A request past it is refused before any byte leaves. Requires the
    /// authoring model. Absent, no request count is imposed.
    #[arg(long, requires = "authoring_model", value_parser = clap::value_parser!(u32).range(1..))]
    pub authoring_max_calls: Option<u32>,
}

impl AuthoringAuthority {
    /// The default authority: no request count.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            authoring_max_calls: None,
        }
    }

    /// At most `calls` authoring requests, kept as given: zero is refused before any request,
    /// never raised to one.
    #[must_use]
    pub fn with_max_calls(mut self, calls: u32) -> Self {
        self.authoring_max_calls = Some(calls);
        self
    }
}

/// The compile arm: the stable compile flags and, beside them, the authoring authority
/// (`--authoring-max-calls`) that bounds how many requests the authoring seat is sent.
#[derive(Debug, clap::Args)]
#[non_exhaustive]
pub struct CompileCommand {
    #[command(flatten)]
    pub args: CompileArgs,
    #[command(flatten)]
    pub authority: AuthoringAuthority,
    #[command(flatten)]
    pub capture: CaptureFlags,
    #[command(flatten)]
    pub observe: ObserveFlags,
    /// The observed room a seated free-intent compile tries each final candidate in (the host
    /// names it, as the Session does): a failed or missing trial is never READY. Not a flag.
    #[arg(skip)]
    pub trials: Option<nika_onboard::compile::room::ObservedRoom>,
}

impl CompileCommand {
    /// The compile flags beside the authority they run under.
    #[must_use]
    pub const fn new(args: CompileArgs, authority: AuthoringAuthority) -> Self {
        Self {
            args,
            authority,
            capture: CaptureFlags::new(),
            observe: ObserveFlags {
                observe_only: false,
            },
            trials: None,
        }
    }

    /// Compile once as the command line states it.
    #[must_use]
    pub fn run(&self) -> VerbOutput {
        if self.observe.observe_only {
            let intent = self.args.intent.as_deref().unwrap_or("");
            return VerbOutput::ok(observation_document(Path::new("."), intent).to_string());
        }
        let output = run_with_capture(
            &self.args,
            &self.authority,
            &self.capture,
            self.trials.as_ref(),
        );
        if self.capture.enabled {
            let line = format!("{}\n", self.capture.status().summary());
            let _ = std::io::Write::write_all(&mut std::io::stderr().lock(), line.as_bytes());
        }
        output
    }
}

/// Compile once without an implicit request count; only a Ready result with
/// an explicit destination writes files.
#[must_use]
pub fn run(args: &CompileArgs) -> VerbOutput {
    run_with(args, &AuthoringAuthority::default())
}

/// Compile once under an explicit authoring authority; only a Ready result with an explicit
/// destination writes files.
#[must_use]
pub fn run_with(args: &CompileArgs, authority: &AuthoringAuthority) -> VerbOutput {
    run_with_capture(args, authority, &CaptureFlags::new(), None)
}

fn run_with_capture(
    args: &CompileArgs,
    authority: &AuthoringAuthority,
    capture: &CaptureFlags,
    trials: Option<&nika_onboard::compile::room::ObservedRoom>,
) -> VerbOutput {
    capture.begin();
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
    let Setup {
        config: authoring_config,
        authority: resolved,
    } = match authoring_setup(args, authority, cognition) {
        Ok(setup) => setup,
        Err(failure) => return failure,
    };
    // Every free-intent door grounds the keys a rule reads in what the host observes:
    // the deterministic door too, never only a seat; a named skeleton or template reads no file.
    if !named && let Ok(root) = std::env::current_dir() {
        request = observed_world(&root, &effective_intent(args, cognition), request);
    }
    // The knowledge door: the source the configuration names, its pack for the intent the
    // compiler reads composed here and stated to the seat beside the card; a source that
    // cannot be honored refuses, never a silent card alone.
    let mut lent = None;
    if let Some(config) = &authoring_config {
        (request, lent) = match config.with_knowledge_lent(request, &effective_intent(args, true)) {
            Ok(composed) => composed,
            Err(error) => {
                return render::failure("knowledge", &error.to_string(), exit::ENV, args.json);
            }
        };
    }
    // The catalogue of this request: the release the pack came from, its holdout kept out.
    let catalogue =
        (lent.as_ref()).map(|(snapshot, exclude)| snapshot.catalogue(exclude.as_deref()));
    let catalog = (catalogue.as_ref())
        .map(|catalogue| catalogue as &dyn nika_onboard::knowledge::ComponentCatalog);
    // Free intents and revisions in words carry a record (a creation's plan, a revision's
    // native candidate); a skeleton, hello or a structured edit never does.
    let sha = (!named).then(|| match revise_intent(&request) {
        Some(intent) => intent_sha256(&intent),
        None => intent_sha256(&effective_intent(args, cognition)),
    });
    let (request, note) = sidecar::replay(sha.as_deref(), args, request);
    // Every compile of the intent carries the verdicts that rejected its bytes in earlier rounds.
    let request = sidecar::carry(sha.as_deref(), request);
    let host = trials.map(|room| room as &dyn nika_onboard::compile::rehearse::Rehearse);
    let result = match (&resolved, &authoring_config) {
        (Some(resolved), Some(config)) => {
            authoring::compile(&request, args, (config, resolved), capture, (host, catalog))
        }
        _ => compile(&request).map_err(|error| error.to_string()),
    };
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            return render::failure("compile_error", &error, exit::ENV, args.json);
        }
    };
    let note = sidecar::keep(sha.as_deref(), note, &outcome);
    let declined = sidecar::decline(sha.as_deref(), &outcome);
    let written = match write_ready(dest.map(String::as_str), &outcome, args) {
        Ok(written) => written,
        Err(failure) => return failure,
    };
    // A named destination this compile did not write: what is there remains; only its
    // presence is read, never through a link, never its bytes.
    let existing = dest
        .filter(|path| written.is_none() && Path::new(path.as_str()).symlink_metadata().is_ok())
        .map(String::as_str);
    let notes = render::Notes {
        plan: note.as_ref(),
        declined: declined.as_ref(),
    };
    render::outcome(&outcome, written, existing, notes, args.json)
}

/// Only a Ready candidate with a named destination is written there, and counted as a draft;
/// any other outcome writes nothing.
fn write_ready<'a>(
    dest: Option<&'a str>,
    outcome: &CompileOutcome,
    args: &CompileArgs,
) -> Result<Option<&'a str>, VerbOutput> {
    let (Some(dest), Some(candidate)) = (dest, &outcome.candidate) else {
        return Ok(None);
    };
    if outcome.status != CompileStatus::Ready {
        return Ok(None);
    }
    if let Err(error) = materialize(Path::new(dest), candidate, args.force) {
        let message = error.to_string();
        return Err(render::failure(
            "destination",
            &message,
            exit::ENV,
            args.json,
        ));
    }
    crate::metrics::record_if_enabled(
        crate::metrics::EventKind::DraftCreated,
        crate::metrics::Facts {
            draft: Some(crate::metrics::DraftSource::Compile),
            ..crate::metrics::Facts::none()
        },
    );
    Ok(Some(dest))
}

/// What a seated compile reads before any file is observed or any request sent: the authoring
/// configuration (the strategy · the knowledge source · the reasoning effort: the flags over the
/// environment, through the parser every door shares; a decision seat alone reads the effort
/// only, the deterministic door nothing), then the authority under that strategy. A typed
/// multiplicity the authority cannot honor is refused here, with zero calls.
fn authoring_setup(
    args: &CompileArgs,
    authority: &AuthoringAuthority,
    cognition: bool,
) -> Result<Setup, VerbOutput> {
    if !cognition {
        return Ok(Setup {
            config: None,
            authority: None,
        });
    }
    let (explicit, env) = (
        explicit_settings(args),
        config::AuthoringSettings::from_env(),
    );
    let (explicit, env) = match args.authoring_model {
        Some(_) => (explicit, env),
        None => (explicit.reasoning_only(), env.reasoning_only()),
    };
    let resolved = config::resolve(&explicit, &env).map_err(|error| {
        render::failure("authoring_config", &error.to_string(), exit::ENV, args.json)
    })?;
    let authority = authority::resolve(args, authority.authoring_max_calls, resolved.strategy)
        .map_err(|refusal| {
            render::failure("authoring_authority", &refusal, exit::FILE, args.json)
        })?;
    Ok(Setup {
        config: Some(resolved),
        authority: Some(authority),
    })
}

/// What a seated compile resolved before any request, each absent without a seat.
struct Setup {
    config: Option<config::AuthoringConfig>,
    authority: Option<nika_onboard::compile::authority::Authority>,
}

/// The request the arguments state: an edit of the base (with the original intent beside it
/// when stated) or a creation (named after its destination), the HOT policy, the answers, and
/// the money the operator states in its words, which this door meters for no seat.
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
    Ok(request.with_stated_money())
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

/// The shape of the files the effective request names (a header, a key set, a categorical
/// column's short repeated values — never a row), observed under the project root `root` only:
/// a seat authors against it, and the grounding law grounds a rule's keys in it on every door.
/// This is the host observation; a library or Serve caller supplies its own world as knowledge.
fn observed_world(root: &Path, intent: &str, request: CompileRequest) -> CompileRequest {
    if intent.trim().is_empty() {
        return request;
    }
    match observe::world(root, intent) {
        Some(world) => request.with_knowledge(world),
        None => request,
    }
}

/// The `--observe-only` document of what `intent` states under `root`.
fn observation_document(root: &Path, intent: &str) -> serde_json::Value {
    let world = (!intent.trim().is_empty())
        .then(|| observe::world(root, intent))
        .flatten();
    nika_onboard::remote_door::document(root, intent, world.as_ref())
}

/// The door's own explicit words: the strategy, the snapshot, the pack, the excluded corpus —
/// the exclusion held whichever side names the snapshot (a held-out corpus named on the command
/// line guards the snapshot `NIKA_KNOWLEDGE` names too) — and the knowledge turned off.
fn explicit_settings(args: &CompileArgs) -> config::AuthoringSettings {
    config::AuthoringSettings::from_flags(
        args.authoring_strategy.as_deref(),
        args.knowledge.as_deref(),
        args.knowledge_pack.as_deref(),
        args.knowledge_exclude.as_deref(),
        args.authoring_reasoning.as_deref(),
        args.no_knowledge,
    )
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

    /// A revision may seat the decision intelligence the caller chose, as the HTTP door does: the
    /// base and the decision model parse together; listing still stands apart.
    #[test]
    fn a_revision_takes_an_explicit_decision_model() {
        let args = parse(&[
            "--base",
            "stock.nika",
            "--change",
            "Keep three days of history instead of two.",
            "--authoring-model",
            "deepseek/deepseek-v4-pro",
            "--decision-model",
            "typesafe/jev-1.13.0",
        ]);
        assert_eq!(args.base.as_deref(), Some("stock.nika"));
        assert_eq!(args.decision_model.as_deref(), Some("typesafe/jev-1.13.0"));
        assert!(
            Door::try_parse_from([
                "compile",
                "--list",
                "--decision-model",
                "typesafe/jev-1.13.0"
            ])
            .is_err()
        );
    }

    /// A held-out corpus named on the command line guards the snapshot the environment names:
    /// the door's own resolver carries it whatever side names the snapshot, guards this build's
    /// embedded release when none is named, and refuses it where no release is read — never a
    /// silent, unguarded evaluation.
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
                identity: None,
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
                identity: None,
            })
        );
        assert_eq!(resolved.strategy, nika_onboard::compile::NativeMode::Only);
        // No release named: the exclusion guards this build's embedded release, never dropped.
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
            )
            .map(|resolved| resolved.knowledge),
            Ok(Some(config::KnowledgeSource::Embedded {
                exclude_corpus: Some("heldout".to_owned()),
            }))
        );
        // With the knowledge off no release is read: the exclusion is refused, not dropped.
        let args = parse(&[
            "x",
            "--authoring-model",
            "mock/echo",
            "--knowledge-exclude",
            "heldout",
            "--no-knowledge",
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

    /// `--no-knowledge` is the door's own off: it wins over the release the environment names,
    /// is refused beside a source of its own, and needs an authoring seat like every knowledge
    /// flag. The same shared resolver decides it, never the clap surface alone.
    #[test]
    fn no_knowledge_turns_the_knowledge_off_over_the_environment() {
        let env = config::AuthoringSettings::none().with_knowledge("/env/release", None);
        let args = parse(&["x", "--authoring-model", "mock/echo", "--no-knowledge"]);
        let resolved = config::resolve(&explicit_settings(&args), &env).expect("resolves");
        assert_eq!(
            resolved.choice,
            config::KnowledgeChoice::Disabled {
                by: config::KnowledgeLayer::Explicit
            }
        );
        assert_eq!(resolved.knowledge, None);
        let both = parse(&[
            "x",
            "--authoring-model",
            "mock/echo",
            "--no-knowledge",
            "--knowledge",
            "/flag/release",
        ]);
        assert_eq!(
            config::resolve(
                &explicit_settings(&both),
                &config::AuthoringSettings::none()
            ),
            Err(config::ConfigError::ContradictoryKnowledge {
                layer: config::KnowledgeLayer::Explicit
            })
        );
        assert!(Door::try_parse_from(["compile", "x", "--no-knowledge"]).is_err());
    }

    /// Every free-intent door carries the host observation of the files its request states:
    /// the bounded, project-confined observer's keys and short repeated values only. An
    /// absent file and a link out of the project are named states, never keys, and a unique
    /// secret-like value never leaves its file.
    #[test]
    fn a_free_intent_carries_the_observation_of_what_it_states() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rows = r#"[{"id":"a","status":"open","token":"sk-live-51H8aZ3xQvYb0987654321abcdefghij"},
            {"id":"b","status":"open","token":"sk-live-51H8aZ3xQvYb0987654321zyxwvutsrq"}]"#;
        std::fs::write(root.join("tickets.json"), rows).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.json"), r#"[{"hidden":"v"}]"#).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret.json"), root.join("linked.json"))
            .unwrap();
        let intent = "Read ./tickets.json, ./linked.json and ./missing.json, keep only the rows whose status is open and write them to ./open.json";
        let request = observed_world(root, intent, CompileRequest::create(intent));
        let world = request.knowledge.expect("the stated files are observed");
        let text = world.to_string();
        let row = |path: &str| {
            let rows = world["observed"].as_array().unwrap();
            rows.iter().find(|r| r["path"] == path).cloned()
        };
        let tickets = row("./tickets.json").unwrap();
        assert_eq!(tickets["state"], "observed", "{text}");
        assert_eq!(
            tickets["columns"],
            serde_json::json!(["id", "status", "token"])
        );
        assert!(tickets["peek_sha256"].is_string(), "{text}");
        assert!(
            !text.contains("sk-live"),
            "no unique value leaves its file: {text}"
        );
        assert_eq!(row("./missing.json").unwrap()["state"], "absent", "{text}");
        #[cfg(unix)]
        {
            assert_eq!(row("./linked.json").unwrap()["state"], "outside_project");
            assert!(
                !text.contains("hidden"),
                "nothing is read through the link: {text}"
            );
        }
        // A request that states nothing is observed as nothing.
        let silent = observed_world(root, "  ", CompileRequest::create("  "));
        assert!(silent.knowledge.is_none());
    }

    #[derive(clap::Parser)]
    struct Verb {
        #[command(flatten)]
        command: CompileCommand,
    }

    /// `--observe-only` prints the observation the compile door would read, and the remote door
    /// admits exactly that document: it states nothing beyond the observer's own facts.
    #[test]
    fn observe_only_prints_the_observation_a_remote_door_admits() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("calendar.json"),
            r#"{"owner":"desk@x.org","appointments":[{"id":"a","status":"booked"}]}"#,
        )
        .unwrap();
        let intent = "Read ./calendar.json and save ./out/reminders.json";
        let document = observation_document(dir.path(), intent);
        assert_eq!(document["observation_version"], 1);
        assert_eq!(document["intent_sha256"], intent_sha256(intent));
        let world = &document["observed_world"];
        assert_eq!(
            world["observed"][0]["columns"],
            serde_json::json!(["appointments", "owner"])
        );
        assert!(!world.to_string().contains("desk@x.org"), "{world}");
        let observed = (world.to_string(), document["trial_inputs"].to_string());
        let admitted =
            nika_onboard::compile::remote::Observed::admit(intent, &observed.0, Some(&observed.1));
        assert_eq!(admitted.map(|o| o.trial.is_some()), Ok(true));
        assert!(observation_document(dir.path(), " ")["observed_world"].is_null());
        let verb = Verb::try_parse_from(["compile", intent, "--observe-only"]).expect("parses");
        assert!(verb.command.observe.observe_only);
        assert!(Verb::try_parse_from(["compile", "--observe-only"]).is_err());
        for seat in ["--authoring-model", "--decision-model"] {
            let argv = ["compile", intent, "--observe-only", seat, "m/x"];
            assert!(Verb::try_parse_from(argv).is_err(), "{seat}");
        }
        let argv = ["compile", intent, "out.nika", "--observe-only"];
        assert!(Verb::try_parse_from(argv).is_err());
    }
}
