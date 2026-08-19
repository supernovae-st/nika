// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika arm` clap surface — the arg types the bin's `Command`
//! enum carries (the `CheckArgs` precedent at the 1500-line cap: the
//! bin composes, the verb owns its arg types).
//!
//! `emit` · `write` · `out` · `mode` · `env_file` · `nika_bin` are the
//! W3 (OS-unit emission) surface, DECLARED now so the clap tree freezes
//! once: passing one in W2 refuses honestly rather than silently doing
//! nothing.

use std::path::PathBuf;

/// `nika arm` — bare (no subcommand, no flag) it READS the registry
/// and reports (the file proposes, the machine disposes). The
/// subcommands are the machine's edge: `fire` is the one firer (D2 —
/// the OS units and `serve` both end here), `disarm` teaches the N4
/// gesture.
#[derive(Debug, clap::Args)]
pub struct ArmArgs {
    /// The machine edge (`fire` · `disarm`) — absent: the read-only
    /// arming report.
    #[command(subcommand)]
    pub sub: Option<ArmSub>,
    /// Emit the OS unit that fires the beats (`launchd` · `systemd`)
    /// instead of reading the registry — the W3 wave.
    #[arg(long, value_enum, value_name = "OS")]
    pub emit: Option<EmitTarget>,
    /// With `--emit`: write the unit file instead of printing it.
    #[arg(long)]
    pub write: bool,
    /// With `--emit --write`: the directory the unit writes to.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// With `--emit`: the scope the unit installs at.
    #[arg(long, value_enum, value_name = "SCOPE")]
    pub mode: Option<EmitMode>,
    /// With `--emit`: the env file the unit loads (provider keys live
    /// there, never in the unit).
    #[arg(long, value_name = "FILE")]
    pub env_file: Option<PathBuf>,
    /// With `--emit`: the nika binary the unit invokes.
    #[arg(long, value_name = "BIN")]
    pub nika_bin: Option<PathBuf>,
}

/// The machine edge under `nika arm`.
#[derive(Debug, clap::Subcommand)]
pub enum ArmSub {
    /// Fire ONE beat now, if it is due — the one firer (D2): on-time
    /// window · miss policy · overlap lock · per-tick ceiling · the
    /// firing record. Prints exactly one stdout line, always (D8).
    Fire(FireArgs),
    /// Teach the disarm gesture (law N4 — removing the line does NOT
    /// disarm; `actif: false` + `raison:` + `jusqu_au:` does).
    Disarm {
        /// The beat label (the workflow file radical).
        label: String,
        /// Also tear the OS unit down — the W3 wave.
        #[arg(long)]
        write: bool,
    },
}

/// `nika arm fire <label>` — the firer's args.
#[derive(Debug, clap::Args)]
pub struct FireArgs {
    /// The beat label — the workflow file radical
    /// (`workflows/doctor.nika.yaml` → `doctor`; a radical collision in
    /// file order takes `-2`, `-3`).
    pub label: String,
    /// Inject the decision instant (RFC 3339) instead of reading the
    /// wall clock — the clock is the verb's edge (D5), so a replay and
    /// a test are deterministic.
    #[arg(long, hide = true, value_name = "RFC3339")]
    pub now: Option<String>,
}

/// The OS an emitted unit targets — the W3 surface, declared now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum EmitTarget {
    /// macOS launchd user agent (`~/Library/LaunchAgents/nika.arm.<radical>.plist`).
    Launchd,
    /// A systemd user timer + service pair.
    Systemd,
}

/// The scope an emitted unit installs at — the W3 surface, declared now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum EmitMode {
    /// The operator's own agent (the default posture).
    User,
    /// A system-wide daemon.
    System,
}
