// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The clap surface of the `nika trace` verb tree — descended from the
//! bin's dispatcher 2026-07-21 (the 1500-line file cap: the bin
//! composes, the verbs own their arg types — the `verbs::key::KeyAction`
//! precedent).

use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Subcommand)]
pub enum TraceAction {
    /// Re-render a run live (replay = re-render, NEVER re-execute).
    Replay(TraceArgs),
    /// Export the evidence pack for one run (journal + manifest +
    /// receipt + VERIFY.md) — RAMS-15: one door on a run's dossier
    /// (read · export · prove), all under `trace`.
    Evidence {
        #[command(flatten)]
        args: crate::evidence::EvidenceArgs,
    },
    /// Read a run receipt — `explain` renders its readable projection
    /// (stable text · a READING, never a proof).
    Receipt {
        #[command(subcommand)]
        action: crate::receipt::ReceiptAction,
    },
    /// Print the final card only.
    Show(TraceArgs),
    /// List the workspace trace store (`.nika/traces/`): age · size ·
    /// workflow · terminal state (completed/failed/paused) · the
    /// resume-candidate marker (★ — the newest of each workflow, the
    /// trace retention never collects · ADR-100).
    Ls {},
    /// Remove traces from the store — one by name/path, `--older-than
    /// <dur>`, or `--all`. Removing a paused trace refuses without
    /// `--force` and names the unanswered prompt it would destroy
    /// (ADR-100).
    Rm {
        /// The trace to remove — a name from `trace ls` or a path.
        #[arg(required_unless_present_any = ["older_than", "all"],
              conflicts_with_all = ["older_than", "all"])]
        trace: Option<String>,
        /// Remove every trace older than this (`45s` · `30m` · `12h` · `7d`).
        #[arg(long, value_name = "DURATION", conflicts_with = "all")]
        older_than: Option<String>,
        /// Remove every trace in the store.
        #[arg(long)]
        all: bool,
        /// Remove even a paused trace (destroys its unanswered prompt).
        #[arg(long)]
        force: bool,
    },
    /// Browse per-task outputs: verb · duration · tokens · bounded
    /// preview (full value: `trace peek`).
    Outputs {
        /// Trace NDJSON path (default: the workspace's latest trace).
        trace: Option<PathBuf>,
    },
    /// Project the journal to OTLP/JSON lines — every `OTel` tool becomes
    /// a viewer (drag into Jaeger UI ≥1.60 · POST lines to any OTLP/HTTP
    /// endpoint). Local file, zero collector, zero vendor.
    Export {
        /// Trace NDJSON path (one `nika-event` Event per line).
        trace: PathBuf,
        /// Output path (default: `<trace>.otlp.jsonl` beside the journal).
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Include recorded task outputs as span attributes (payloads
        /// stay LOCAL either way — this only widens the exported file).
        #[arg(long)]
        include_content: bool,
    },
    /// Verify the journal's tamper-evidence chain (0.96+), then climb
    /// the proof ladder: SEALED (the `run_sealed` signature verifies
    /// against a custody key) · ANCHORED (the `<trace>.anchor.json`
    /// sidecar verifies fully offline) · REPLAYED (--replay compares
    /// a fresh run). The HIGHEST honestly-attained tier is reported.
    /// Three refusals name themselves rather than hide in a tier:
    /// TAMPERED · SEAL BURIED (lines chained AFTER the seal — appending
    /// needs only write access, so this is forgery, never a crash) ·
    /// ANCHOR FORGED (a sidecar that vouches for nothing) · and
    /// SEAL UNATTRIBUTABLE (the seal names a key you do not hold: the
    /// signature is NOT judged, which is a missing input and never
    /// evidence of forgery).
    /// Exit 0 the tier holds · 2 broken chain, forged seal or forged
    /// anchor · 3 unchained (pre-chain journal), a missing input, or a
    /// seal this host cannot attribute.
    Verify {
        /// Trace NDJSON path(s) — a shell glob (`.nika/traces/*.ndjson`)
        /// just works: each file verifies under its own header, the
        /// worst exit survives (default: the workspace's latest trace).
        traces: Vec<PathBuf>,
        /// A candidate run public key for the SEALED tier (default:
        /// ~/.nika/keys/run-signing.pub, then the retired.pub ledger).
        #[arg(long)]
        key: Option<PathBuf>,
        /// Require the anchor tier: a MISSING sidecar is exit 3 (a
        /// forged one is exit 2 either way).
        #[arg(long)]
        anchored: bool,
        /// Require the seal tier: an UNSEALED journal (no `run_sealed`
        /// line) is exit 3 (a forged seal is exit 2 either way).
        #[arg(long)]
        sealed: bool,
        /// The REPLAYED tier: a FRESH journal of the same workflow to
        /// compare against (verify never re-executes).
        #[arg(long)]
        replay: Option<PathBuf>,
    },
    /// Notarize the journal head OUTSIDE the journal (S3): submit the
    /// post-seal head — signed with the run key — to the public
    /// Sigstore Rekor v2 transparency log plus an RFC 3161 timestamp,
    /// writing a detached `<trace>.anchor.json` sidecar. An explicit
    /// NETWORK act: this verb IS the opt-in. Exit 0 anchored · 2 the
    /// journal refuses (broken/torn) · 3 no key/network.
    Anchor {
        /// Trace NDJSON path (default: the workspace's latest trace).
        trace: Option<PathBuf>,
        /// The Rekor v2 shard. A private rekor-tiles deployment works,
        /// but its checkpoint is not the pinned Sigstore key's — the
        /// ANCHORED verify tier stays out of reach there.
        #[arg(long, default_value_t = crate::anchor::DEFAULT_REKOR_URL.to_owned())]
        rekor_url: String,
        /// The RFC 3161 timestamp authority. The token verifies against
        /// the pinned Sigstore TSA leaf — mirrors of that TSA work,
        /// other authorities fail closed.
        #[arg(long, default_value_t = crate::anchor::DEFAULT_TSA_URL.to_owned())]
        tsa_url: String,
    },
    /// Is this run reproducible? Compare a recorded journal against a
    /// fresh one and classify every task: reproduced · nondeterministic
    /// (same def+inputs, different output) · authored · environment ·
    /// status-changed · unverifiable. Exit 0 reproduced · 2 diverged.
    Reproduce {
        /// The RECORDED journal (the reference frame).
        recorded: PathBuf,
        /// A FRESH journal of the same workflow (run it again first).
        fresh: PathBuf,
    },
    /// Read ONE task's full output + its identity (hashes · duration ·
    /// tokens). `--raw` prints the exact value only (pipe it to jq).
    Peek {
        /// Trace NDJSON path (one `nika-event` Event per line).
        trace: PathBuf,
        /// The task id whose output to read.
        task: String,
        /// Print the exact recorded value only (machine-friendly).
        #[arg(long)]
        raw: bool,
    },
    /// The session digest: waves · the wave holder others waited for ·
    /// the spend by verb · the wall. The four numbers `nika-tui-core`
    /// derives — every one of them gated by the predicate that says
    /// whether it may be claimed (a holder nobody waited for is not
    /// named; no holder is painted over a failure).
    ///
    /// Takes the workflow file for the same reason `flow` does: the
    /// journal records the workflow's ID and sha256, never its PATH, and
    /// waves live in the dependency graph the definition carries.
    Session {
        /// Trace NDJSON path (one `nika-event` Event per line).
        trace: PathBuf,
        /// The workflow file the run executed (`*.nika.yaml`).
        workflow: String,
    },
    /// The data waterfall: which output fed which task, with recorded
    /// sizes (plan bindings from the workflow file × sizes from the
    /// trace).
    Flow {
        /// Trace NDJSON path (default: the workspace's latest trace —
        /// `nika trace flow <workflow>` alone reads the last run).
        trace: Option<PathBuf>,
        /// The workflow file the run executed (`*.nika.yaml`) — the
        /// trace records values, the definition records the bindings.
        workflow: Option<String>,
    },
}

#[derive(Args)]
// Five independent CLI flags ARE five bools — the clap-surface idiom, not
// a state machine to encode.
#[allow(clippy::struct_excessive_bools)]
pub struct TraceArgs {
    /// Trace NDJSON path (one `nika-event` Event per line).
    pub trace: Option<PathBuf>,
    /// Render the built-in success storyboard.
    #[arg(long, conflicts_with = "trace")]
    pub demo: bool,
    /// Render the built-in failure storyboard.
    #[arg(long, conflicts_with_all = ["trace", "demo"])]
    pub demo_fail: bool,
    /// Replay time compression (6 = 6× faster than recorded).
    #[arg(long, default_value_t = 6.0)]
    pub speed: f64,
    /// Hide the per-task output summaries (`→ {…} · 312B`) on the
    /// rendered storyboard. Interactive TTY only — a piped `trace show`
    /// never carries them anyway.
    #[arg(long)]
    pub no_outputs: bool,
}
