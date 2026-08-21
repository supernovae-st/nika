// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The OS-unit renderer (W3 · « LE PONT ») — pure, zero I/O: GIVEN a
//! validated registry and the machine facts the L4 edge collected
//! ([`EmitCtx`]), hand the unit FILES back as text. Writing them (and
//! reading the machine's zone) is the L4 verb's job — this layer never
//! touches a filesystem, and every path it interpolates is data.
//!
//! The laws the render keeps:
//!
//! - **D2 · one firer** — a per-beat unit calls `nika arm fire
//!   <label>`, NEVER `run` (a test pins it); `serve` (W5) gets its own
//!   unit by name, emitted today.
//! - **D7 · no secret in a unit** — an env file rides by PATH only
//!   (a `/bin/sh -c` wrap under launchd · `EnvironmentFile=` under
//!   systemd); the values never cross into the text.
//! - **D9 · the binary path is absolute** — judged at the L4 edge,
//!   carried here verbatim.
//! - **D10 · launchd fires in the MACHINE's zone** — a beat whose `TZ=`
//!   differs refuses ([`EmitRefusal::TzMismatch`]); systemd carries the
//!   zone in `OnCalendar=`, so the same beat renders there.
//! - **D6 · v0 refuses what it cannot keep** — the twin of
//!   `v0_refusal` in `verbs/arm/fire.rs` (nika-cli): one policy set,
//!   two edges. Emit refuses BEFORE the unit lands; fire refuses AT the
//!   tick. The two lists must never disagree.
//!
//! Split (the ~700-line single-file bar): the private `launchd` and
//! `systemd` submodules own each target's text.

use std::path::PathBuf;

use crate::registry::{AfterSkip, ArmRegistry, Beat, Cadence, Locus, MissPolicy, Overlap};

mod launchd;
mod systemd;

/// The mark every emitted unit carries on its first line — the disarm
/// gesture recognizes its own by it and NEVER touches a foreign unit.
pub const GENERATED_MARK: &str = "nika arm · GENERATED from";

/// The launchd interval budget: past it the plist is a load, not a
/// calendar (the cartesian product of the cadence's restricted fields,
/// one dict per tuple — 500 dicts is already an abuse the OS accepts).
pub const MAX_INTERVALS: usize = 500;

/// The OS an emitted unit targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Target {
    /// A macOS launchd user agent (`~/Library/LaunchAgents/`).
    Launchd,
    /// A systemd USER timer + service pair (`~/.config/systemd/user/`).
    SystemdUser,
}

impl Target {
    /// The word the `--emit` flag carries — the unit header cites it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Launchd => "launchd",
            Self::SystemdUser => "systemd",
        }
    }
}

/// What a render covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mode {
    /// One unit per active · local beat (the timers).
    PerBeat,
    /// ONE unit for `nika serve` (the daemon lands in W5 — the unit is
    /// emitted by name today).
    Serve,
}

/// The machine facts a render needs — collected by the L4 edge (the
/// zone read is the verb's; this layer never reads one).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct EmitCtx {
    /// The ABSOLUTE path of the nika binary the unit invokes (D9).
    pub nika_bin: PathBuf,
    /// The project root — the unit's working directory.
    pub project_root: PathBuf,
    /// The project file the units are generated from (the header cites
    /// it — a unit always names its source).
    pub project_file: PathBuf,
    /// The env file the unit loads — provider keys live there, never in
    /// the unit (D7).
    pub env_file: Option<PathBuf>,
    /// The directory receiving the units' `<label>.{out,err}` logs.
    pub log_dir: PathBuf,
    /// The machine's IANA zone (launchd fires in it — D10).
    pub machine_tz: String,
}

impl EmitCtx {
    /// The full fact set, in the struct's field order.
    #[must_use]
    pub fn new(
        nika_bin: PathBuf,
        project_root: PathBuf,
        project_file: PathBuf,
        env_file: Option<PathBuf>,
        log_dir: PathBuf,
        machine_tz: String,
    ) -> Self {
        Self {
            nika_bin,
            project_root,
            project_file,
            env_file,
            log_dir,
            machine_tz,
        }
    }
}

/// One unit file: its NAME (the loader and the disarm gesture both key
/// on it) and its full text, header included.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Unit {
    /// The file name (`nika.arm.<radical>.plist` ·
    /// `nika.arm.<radical>.timer` + `.service` · `nika.serve.*`).
    pub file_name: String,
    /// The unit's full text.
    pub body: String,
}

/// A render refusal — named, and teaching its fix (the voice of the
/// grammar's [`CadenceError`](crate::CadenceError), on the emit plane).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EmitRefusal {
    /// D10 — launchd fires in the machine's zone; a beat whose `TZ=`
    /// differs would fire at an hour nobody wrote.
    #[error(
        "beat {beat} · fuseau {cadence_tz} ≠ machine {machine_tz} — launchd tire dans le fuseau de la machine · remède: TZ={machine_tz} dans la cadence, ou --emit systemd (le fuseau y voyage dans OnCalendar=)"
    )]
    TzMismatch {
        /// The beat's label (D4).
        beat: String,
        /// The zone the cadence declares.
        cadence_tz: String,
        /// The zone the machine runs in.
        machine_tz: String,
    },
    /// D6 — a policy v0 cannot keep; the refusal names the version it
    /// arrives with.
    #[error(
        "beat {beat} · {what} — arrive avec {arrives} · v0 n'émet que ce qu'elle peut tenir (les défauts de la loi ⑥ demeurent)"
    )]
    UnsupportedInV0 {
        /// The beat's label (D4).
        beat: String,
        /// The policy, as written in the file (`chevauchement: remplacer`).
        what: String,
        /// The version the support arrives with.
        arrives: String,
    },
    /// The cartesian product outgrew [`MAX_INTERVALS`] — a plist of n
    /// dicts is a load, not a calendar.
    #[error(
        "beat {beat} · {n} créneaux demandés — au-delà de 500 dicts launchd n'est plus un calendrier · remède: resserre la cadence (ou --emit systemd, qui écrit les ensembles en ligne)"
    )]
    TooManyIntervals {
        /// The beat's label (D4).
        beat: String,
        /// How many dicts the product would write.
        n: usize,
    },
    /// `on-webhook` carries no calendar — no timer unit can fire it.
    #[error(
        "beat {beat} · on-webhook n'a pas de calendrier — le beat tire à l'événement, jamais à l'horloge · remède: serve écoute l'événement — aucune unité ne le peut"
    )]
    Webhook {
        /// The beat's label (D4).
        beat: String,
    },
    /// A cadence that passed `validate` refuses to re-parse — an ENGINE
    /// fault, said as such (the `fire.rs` precedent), never a guess.
    #[error("engine fault · beat {beat} · {detail} — à reporter avec le fichier")]
    EngineFault {
        /// The beat's label (D4).
        beat: String,
        /// The parser's own words.
        detail: String,
    },
}

/// The beat labels, in file order (D4): the workflow file's radical —
/// `workflows/doctor.nika.yaml` → `doctor` — a collision taking `-2`,
/// `-3`. The ONE identity both consumers read: the firer (`nika arm
/// fire <label>` delegates here) and the units (`nika.arm.<label>`).
#[must_use]
pub fn labels(reg: &ArmRegistry) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for beat in reg.beats() {
        let radical = radical_of(&beat.workflow);
        let mut candidate = radical.clone();
        let mut n = 1u32;
        // Uniqueness is the law (`doctor-2.nika.yaml` arming next to
        // two `doctor` must never hand two beats one label).
        while out.contains(&candidate) {
            n += 1;
            candidate = format!("{radical}-{n}");
        }
        out.push(candidate);
    }
    out
}

/// The radical of a workflow path: the basename minus the `.nika.yaml`
/// the grammar guarantees (a bare `*.yaml` loses its last extension).
fn radical_of(workflow: &str) -> String {
    let base = workflow.rsplit('/').next().unwrap_or(workflow);
    if let Some(radical) = base.strip_suffix(".nika.yaml") {
        return radical.to_owned();
    }
    base.rsplit_once('.')
        .map_or_else(|| base.to_owned(), |(stem, _)| stem.to_owned())
}

/// Render the units for `target` + `mode`. Per-beat: one unit per
/// ACTIVE · LOCAL beat (a suspended or cloud beat emits nothing — the
/// L4 verb says why). The first refusal stops the render, in file
/// order: a unit set that half-landed is the failure this shape
/// refuses.
///
/// The registry must have passed [`validate`](crate::validate) first
/// (the door): a cadence that still refuses to parse is an engine
/// fault, returned as such, never skipped in silence.
///
/// # Errors
/// An [`EmitRefusal`] — the D10 zone mismatch · the D6 v0 policy set ·
/// the interval budget · a webhook beat · an engine fault.
pub fn render(
    reg: &ArmRegistry,
    ctx: &EmitCtx,
    target: Target,
    mode: Mode,
) -> Result<Vec<Unit>, EmitRefusal> {
    if let Mode::Serve = mode {
        return Ok(vec![match target {
            Target::Launchd => launchd::serve(ctx),
            Target::SystemdUser => systemd::serve(ctx),
        }]);
    }
    let names = labels(reg);
    let mut units = Vec::new();
    for (beat, label) in reg.beats().zip(names.iter()) {
        if !beat.is_active() || beat.locus() != Locus::Local {
            continue;
        }
        let cadence = match Cadence::parse(&beat.cadence) {
            Ok(cadence) => cadence,
            Err(e) => {
                return Err(EmitRefusal::EngineFault {
                    beat: label.clone(),
                    detail: format!("une cadence validée refuse de se relire — {e}"),
                });
            }
        };
        if let Some((what, arrives)) = v0_unsupported(beat) {
            return Err(EmitRefusal::UnsupportedInV0 {
                beat: label.clone(),
                what: what.to_owned(),
                arrives: arrives.to_owned(),
            });
        }
        let Cadence::Cron { tz, spec } = &cadence else {
            return Err(EmitRefusal::Webhook {
                beat: label.clone(),
            });
        };
        match target {
            Target::Launchd => {
                if tz != &ctx.machine_tz {
                    return Err(EmitRefusal::TzMismatch {
                        beat: label.clone(),
                        cadence_tz: tz.clone(),
                        machine_tz: ctx.machine_tz.clone(),
                    });
                }
                units.push(launchd::per_beat(ctx, label, spec)?);
            }
            Target::SystemdUser => units.extend(systemd::per_beat(ctx, label, beat, tz, spec)),
        }
    }
    Ok(units)
}

/// The D6 set — v0 refuses what it cannot keep, naming the version the
/// support arrives with. The TWIN of `v0_refusal` in
/// `verbs/arm/fire.rs` (nika-cli): the two must never disagree.
fn v0_unsupported(beat: &Beat) -> Option<(&'static str, &'static str)> {
    const SERVE: &str = "serve v0.2";
    if beat.chevauchement == Some(Overlap::Remplacer) {
        return Some(("chevauchement: remplacer", SERVE));
    }
    if beat.apres_saut == Some(AfterSkip::ACompletion) {
        return Some(("après_saut: à-complétion", SERVE));
    }
    if beat.manque == Some(MissPolicy::Rattraper) {
        return Some(("manqué: rattraper", SERVE));
    }
    if beat.decalage.is_some() {
        return Some(("décalage:", SERVE));
    }
    None
}

/// The GENERATED header — one sentence behind two comment markers (XML
/// for the plist, `#` for systemd). The exact flag spelling stays OUT:
/// `--` is illegal inside an XML comment (the OS-lint gate proves it),
/// so the header names the emit FAMILY and the verb prints the exact
/// command at print time.
pub(crate) fn header(ctx: &EmitCtx, what: &str, target: Target) -> String {
    format!(
        "{GENERATED_MARK} {} · {what} · do not hand-edit · regenerated by nika arm emit {}",
        ctx.project_file.display(),
        target.word()
    )
}

/// POSIX single-quoting (`'` → `'\''`) — the launchd env wrap's only
/// escape hatch.
pub(crate) fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The five XML escapes (`&` first — it introduces the rest) — every
/// value the plist carries crosses it.
pub(crate) fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
