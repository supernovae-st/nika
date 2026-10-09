// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resident CLI vocabulary, shared without a dependency on the server.

use std::path::PathBuf;

/// `nika serve`'s explicit native authoring seat (the `nika compile` flag words; only the
/// reasoning effort falls back to the environment). Absent `--authoring-model`, nothing is read
/// and nothing changes.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct NativeAuthoringArgs {
    /// Seat native authoring on POST /v1/compile generation 2 with this direct provider model
    /// (`provider/name`); each caller still opts in (`cognition: "explicitProvider"`). Its key
    /// and endpoint are read from the environment now, never per request. Requires `--bind`.
    #[arg(
        long = "authoring-model",
        value_name = "PROVIDER/NAME",
        requires = "bind"
    )]
    pub model: Option<String>,
    /// Output tokens per call (positive); absent, the route's own completion bound. A caller
    /// may only narrow it.
    #[arg(long = "authoring-max-tokens", value_name = "N", requires = "model")]
    pub max_tokens: Option<u32>,
    /// Seconds per model invocation (positive); absent, the route's own wait. Resends consume
    /// the request grant.
    #[arg(long = "authoring-timeout", value_name = "SECONDS", requires = "model")]
    pub timeout: Option<u64>,
    /// Seconds per round (positive): the work stops and the request answers 408. Absent, no
    /// round deadline.
    #[arg(
        long = "authoring-deadline",
        value_name = "SECONDS",
        requires = "model"
    )]
    pub deadline: Option<u64>,
    /// Desired repair rounds, within the operator's explicit request grant. Absent, no count:
    /// repairs continue while they make progress.
    #[arg(long = "authoring-repairs", value_name = "N", requires = "model")]
    pub repairs: Option<u32>,
    /// A Foundry knowledge release root, admitted and pinned at start against a trusted identity;
    /// a flag carries none, so the seat is refused until one is wired.
    #[arg(long = "knowledge", value_name = "DIR", requires = "model")]
    pub knowledge: Option<PathBuf>,
    /// A corpus whose examples the knowledge door never recalls.
    #[arg(
        long = "knowledge-exclude",
        value_name = "CORPUS",
        requires = "knowledge"
    )]
    pub knowledge_exclude: Option<String>,
    /// Turn the knowledge off on the seat's own layer: nothing is pinned (beside --knowledge,
    /// the shared parser refuses the seat).
    #[arg(long = "no-knowledge", requires = "model")]
    pub no_knowledge: bool,
    /// The reasoning effort every seat call asks (low · high · max), sent only where the route
    /// qualifies it; the one word `NIKA_AUTHORING_REASONING` names when the flag is absent.
    #[arg(long = "authoring-reasoning", value_name = "LEVEL", requires = "model")]
    pub reasoning: Option<String>,
    /// A decision model (`typesafe/jev-1.13.0` or `provider/name`, the `nika compile` words) that
    /// judges every candidate in place of the author, on its own client; its key is read now.
    #[arg(long = "decision-model", value_name = "MODEL", requires = "model")]
    pub decision_model: Option<String>,
}

const SHUTDOWN_HELP: &str = "Shutdown (persistent mode): Ctrl-C/SIGINT and SIGTERM stop HTTP admissions \
and new scheduling, then drain running AND queued jobs for up to 30 seconds \
with four workers. On grace expiry, running jobs become interrupted and \
jobs still queued remain queued. Restart with the same --state-root resumes queued \
jobs from their captured snapshots; interrupted jobs are not retried. SIGKILL \
skips cleanup: the next start interrupts ownerless running jobs and resumes \
queued jobs. A completed drain exits 0; grace expiry exits 1. The 30-second \
grace bounds execution draining, not filesystem cleanup or a stuck backend. \
Allow extra time before a supervisor forces SIGKILL.";

/// `nika serve` — the resident firer's args, plus the explicit HTTP pair.
#[allow(clippy::struct_excessive_bools)] // one bool per independent CLI switch
#[derive(Debug, Clone, Default, clap::Args)]
#[non_exhaustive]
#[command(after_long_help = SHUTDOWN_HELP)]
pub struct ServeArgs {
    /// Fire what is due once, then exit — the rehearsal.
    #[arg(long)]
    pub once: bool,
    /// Say what WOULD fire, run nothing.
    #[arg(long)]
    pub dry: bool,
    /// Inject the clock (RFC 3339 · D5) — the harness.
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub now: Option<String>,
    /// Stop the loop at this instant (RFC 3339) — the harness.
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub until: Option<String>,
    /// Bind an authenticated HTTP listener. Requires `--workflows` and `--token-file`.
    #[arg(long, value_name = "ADDR")]
    pub bind: Option<String>,
    /// The served registry: the listener lists, schedules and ADMITS BY NAME
    /// (`POST /v1/jobs {"workflow": "<name>"}`) only the `.nika` workflows
    /// under this directory, named from the project root. A remote world
    /// rides the snapshot `nika check <file> --json --sdk-snapshot` prints.
    /// Requires `--bind`.
    #[arg(long, value_name = "DIR")]
    pub workflows: Option<PathBuf>,
    /// Acknowledge a non-loopback `--bind`. Authentication is unchanged.
    /// TLS is a reverse proxy — this process does not terminate it.
    #[arg(long)]
    pub allow_remote: bool,
    /// Owner-only Bearer file (32–512 visible ASCII bytes, mode 0600). Never argv.
    /// Mint: umask 077 && openssl rand -hex 24 > .nika/serve.token && chmod 600 .nika/serve.token
    #[arg(long, value_name = "FILE")]
    pub token_file: Option<PathBuf>,
    /// Durable job-state root. Defaults to `<cwd>/.nika/serve`.
    #[arg(long, value_name = "DIR")]
    pub state_root: Option<PathBuf>,
    #[command(flatten)]
    pub authoring: NativeAuthoringArgs,
    /// Explicit model invocation and physical-request ceiling per authoring round; absent, no
    /// count bounds the round's requests (its repair preference and deadline still apply).
    #[arg(long, value_name = "N", requires = "model")]
    pub authoring_max_calls: Option<u32>,
    /// Seat the cost-review door (POST /v1 and /v2/cost-reviews · health `costReviewV1`/`V2`).
    #[arg(long, requires = "bind")]
    pub cost_review: bool,
    /// Per-run spend ceiling of manual jobs in USD (default 1); `none` disarms it explicitly.
    #[arg(long, value_name = "USD|none", requires = "bind")]
    pub run_cost_ceiling: Option<String>,
    /// Serve the project's native Session under /v1/sessions (health `sessionHost`): the same
    /// Session bare `nika` opens, whose runs are this resident's jobs. A Session writes the
    /// project's workflows only on a human's consent.
    #[arg(long, requires = "bind")]
    pub sessions: bool,
}
