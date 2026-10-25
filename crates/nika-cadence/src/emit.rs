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
//!   (the firer's `--env-file` under launchd · `EnvironmentFile=` under
//!   systemd); the values never cross into the text.
//! - **D9 · the binary path is absolute** — judged at the L4 edge,
//!   carried here verbatim.
//! - **D10 · launchd fires in the MACHINE's zone** — a beat whose `TZ=`
//!   differs wakes at each machine hour one of its slots can fall on over
//!   the coming year (every pair of the two zones' offsets), with no day,
//!   month or weekday: the firer reads each wake against the cadence in its
//!   own zone, fires the due slot and leaves the others not due, unjournaled,
//!   so nothing fires at an hour nobody wrote. Without the edge's clock
//!   ([`EmitCtx::at`]) the beat refuses ([`EmitRefusal::TzMismatch`]);
//!   systemd carries the zone in `OnCalendar=`.
//! - **D6 · v0 refuses what it cannot keep** — [`crate::v0_unsupported`]
//!   (one policy set). Emit refuses BEFORE the unit lands; the firer
//!   refuses AT the tick via [`crate::tick_decision`].
//! - **A unit carries its project** — every unit name carries the
//!   project key ([`project_key`]), so two projects arming beats of the
//!   same name never share, overwrite or disarm one unit; the GENERATED
//!   header names the project file the unit belongs to
//!   ([`generated_source`]).
//!
//! Split (the ~700-line single-file bar): the private `launchd` and
//! `systemd` submodules own each target's text.

use std::path::PathBuf;

use crate::registry::{ArmRegistry, Beat, Cadence, Locus};

mod launchd;
mod systemd;

/// The mark every emitted unit carries on its first line — the disarm
/// gesture recognizes its own by it and NEVER touches a foreign unit.
pub const GENERATED_MARK: &str = "nika arm · GENERATED from";

/// How many characters of the project root's fingerprint a unit name
/// carries: 32 bits keep one person's projects apart, and the GENERATED
/// header names the project for the rare collision the L4 edge refuses.
pub const PROJECT_KEY_LEN: usize = 8;

/// The project key every unit name carries: the first
/// [`PROJECT_KEY_LEN`] characters of the project root's fingerprint (the
/// L4 edge computes it from the canonical root, the one a run trace and
/// a readiness receipt bind).
#[must_use]
pub fn project_key(root_fingerprint: &str) -> String {
    root_fingerprint.chars().take(PROJECT_KEY_LEN).collect()
}

/// A beat's unit label, the launchd `Label` and the systemd unit stem:
/// `nika.arm.<project key>.<beat label>`.
#[must_use]
pub fn beat_unit(project_key: &str, label: &str) -> String {
    format!("nika.arm.{project_key}.{label}")
}

/// The unit label an older engine gave a beat, without the project key
/// (`nika.arm.<beat label>`). It is never written again: the L4 edge
/// recognizes it to report it, and to disarm this project's own.
#[must_use]
pub fn legacy_beat_unit(label: &str) -> String {
    format!("nika.arm.{label}")
}

/// A project's `nika serve` unit label: `nika.serve.<project key>`.
#[must_use]
pub fn serve_unit(project_key: &str) -> String {
    format!("nika.serve.{project_key}")
}

/// The files a beat's unit label names on `target`: the plist, or the
/// systemd timer and its service.
#[must_use]
pub fn beat_unit_files(target: Target, unit: &str) -> Vec<String> {
    match target {
        Target::Launchd => vec![format!("{unit}.plist")],
        Target::SystemdUser => vec![format!("{unit}.timer"), format!("{unit}.service")],
    }
}

/// The project file a GENERATED unit names, read back from its header
/// exactly as the render wrote it. `None` for a unit without the mark.
#[must_use]
pub fn generated_source(body: &str) -> Option<&str> {
    let (_, after) = body.split_once(GENERATED_MARK)?;
    let line = after.lines().next()?.strip_prefix(' ')?;
    let (head, _) = line.split_once(" · do not hand-edit")?;
    let (source, _) = head.rsplit_once(" · ")?;
    Some(source)
}

/// Hex characters of a unit revision.
pub const UNIT_REV_LEN: usize = 12;

/// The revision of the schedule a beat's unit encodes: what decides when the
/// OS wakes it, the cadence as written and, on launchd, the machine zone it
/// was rendered in (a systemd calendar names its own zone). The unit hands it
/// to the firer (`arm fire <label> --unit-rev <rev>`), which refuses a wake
/// from a unit whose revision the registry no longer gives: a stale unit.
#[must_use]
pub fn unit_revision(expression: &str, target: Target, machine_tz: &str) -> String {
    use sha2::Digest as _;
    let zone = if target == Target::Launchd {
        machine_tz
    } else {
        ""
    };
    let preimage = format!("nika/unit-rev@1\n{}\n{expression}\n{zone}", target.word());
    let digest = format!("{:x}", sha2::Sha256::digest(preimage.as_bytes()));
    digest[..UNIT_REV_LEN].to_owned()
}

/// The schedule revision a GENERATED unit carries in its header (`beat
/// <label> rev <rev>`); `None` for a unit written before revisions.
#[must_use]
pub fn revision_in_unit(body: &str) -> Option<&str> {
    let (_, after) = body.split_once(GENERATED_MARK)?;
    let line = after.lines().next()?;
    let (head, _) = line.split_once(" · do not hand-edit")?;
    let (_, rev) = head.rsplit_once(" rev ")?;
    (rev.len() == UNIT_REV_LEN && rev.bytes().all(|b| b.is_ascii_hexdigit())).then_some(rev)
}

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
    /// The project key every unit name carries ([`project_key`]).
    pub project_key: String,
    /// The env file the unit loads — provider keys live there, never in
    /// the unit (D7).
    pub env_file: Option<PathBuf>,
    /// The directory receiving the units' `<label>.{out,err}` logs.
    pub log_dir: PathBuf,
    /// The machine's IANA zone (launchd fires in it — D10).
    pub machine_tz: String,
    /// The instant the edge rendered at: the offsets a cadence in another zone than the
    /// machine's takes are read over the year after it (D10). `None` refuses such a beat on
    /// launchd.
    pub rendered_at: Option<jiff::Timestamp>,
}

impl EmitCtx {
    /// The full fact set, in the struct's field order.
    #[must_use]
    pub fn new(
        nika_bin: PathBuf,
        project_root: PathBuf,
        project_file: PathBuf,
        project_key: String,
        env_file: Option<PathBuf>,
        log_dir: PathBuf,
        machine_tz: String,
    ) -> Self {
        Self {
            nika_bin,
            project_root,
            project_file,
            project_key,
            env_file,
            log_dir,
            machine_tz,
            rendered_at: None,
        }
    }

    /// The same facts, rendered at `now`: a launchd beat in another zone than the machine's then
    /// wakes at the machine hours its slots fall on over the year after `now` (D10).
    #[must_use]
    pub fn at(mut self, now: jiff::Timestamp) -> Self {
        self.rendered_at = Some(now);
        self
    }
}

/// One unit file: its NAME (the loader and the disarm gesture both key
/// on it) and its full text, header included.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Unit {
    /// The file name (`nika.arm.<key>.<radical>.plist` ·
    /// `nika.arm.<key>.<radical>.timer` + `.service` ·
    /// `nika.serve.<key>.*`).
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
    /// differs, rendered without the edge's clock or in a zone the
    /// embedded tzdb does not know, would fire at an hour nobody wrote.
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
    /// D6 — a policy v0 cannot keep; the refusal states the present limit
    /// and promises no release.
    #[error(
        "beat {beat} · {what} — {arrives} · v0 n'émet que ce qu'elle peut tenir (les défauts de la loi ⑥ demeurent)"
    )]
    UnsupportedInV0 {
        /// The beat's label (D4).
        beat: String,
        /// The policy, as written in the file (`chevauchement: remplacer`).
        what: String,
        /// The present limit, as [`crate::v0_unsupported`] states it (the
        /// field kept its historical name; it no longer names a release).
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
/// `workflows/doctor.nika` → `doctor` — a collision taking `-2`,
/// `-3`. The ONE identity both consumers read: the firer
/// (`nika arm fire <label>` delegates here) and the units
/// ([`beat_unit`]).
#[must_use]
pub fn labels(reg: &ArmRegistry) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for beat in reg.beats() {
        let radical = radical_of(&beat.workflow);
        let mut candidate = radical.clone();
        let mut n = 1u32;
        // Uniqueness is the law (`doctor-2.nika` arming next to
        // two `doctor` must never hand two beats one label).
        while out.contains(&candidate) {
            n += 1;
            candidate = format!("{radical}-{n}");
        }
        out.push(candidate);
    }
    out
}

/// The radical of a workflow path: the basename minus the `.nika`
/// suffix the grammar guarantees (`support.v2.nika` → `support.v2`).
fn radical_of(workflow: &str) -> String {
    let base = workflow.rsplit('/').next().unwrap_or(workflow);
    nika_source::program_stem(base).unwrap_or(base).to_owned()
}

/// Render the units for `target` + `mode`. Per-beat: one unit per LOCAL
/// beat. A suspended beat keeps its unit: the firer journals it
/// « inactive » until its `jusqu_au`, then wakes it, so nothing has to
/// re-arm it. A cloud beat emits nothing (the L4 verb says why). The first
/// refusal stops the render, in file order: a unit set that half-landed is
/// the failure this shape refuses.
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
        if beat.locus() != Locus::Local {
            continue;
        }
        units.extend(beat_units(beat, label, ctx, target)?);
    }
    Ok(units)
}

/// Render one beat's units (its label, as [`labels`] spells it): what
/// [`render`] writes for that beat alone, so a gesture on one beat is never
/// refused for another. A cloud or unknown beat has none.
///
/// # Errors
/// As [`render`], for this beat only.
pub fn render_beat(
    reg: &ArmRegistry,
    ctx: &EmitCtx,
    target: Target,
    label: &str,
) -> Result<Vec<Unit>, EmitRefusal> {
    let names = labels(reg);
    match reg
        .beats()
        .zip(names.iter())
        .find(|(_, name)| name.as_str() == label)
    {
        Some((beat, name)) if beat.locus() == Locus::Local => beat_units(beat, name, ctx, target),
        _ => Ok(Vec::new()),
    }
}

/// One local beat's units on `target`.
fn beat_units(
    beat: &Beat,
    label: &str,
    ctx: &EmitCtx,
    target: Target,
) -> Result<Vec<Unit>, EmitRefusal> {
    let cadence = match Cadence::parse(&beat.cadence) {
        Ok(cadence) => cadence,
        Err(e) => {
            return Err(EmitRefusal::EngineFault {
                beat: label.to_owned(),
                detail: format!("une cadence validée refuse de se relire — {e}"),
            });
        }
    };
    if let Some((what, arrives)) = crate::tick::v0_unsupported(beat) {
        return Err(EmitRefusal::UnsupportedInV0 {
            beat: label.to_owned(),
            what: what.to_owned(),
            arrives: arrives.to_owned(),
        });
    }
    // The anchored interval wakes weekly on its anchor's weekday and
    // time (no OS calendar says « every other week »): on an off week
    // the firer finds the on-week slot already claimed and skips.
    let (tz, spec) = match &cadence {
        Cadence::Cron { tz, spec } => (tz, *spec),
        Cadence::Every { tz, anchor, .. } => (tz, crate::every::weekly_wake(*anchor)),
        _ => {
            return Err(EmitRefusal::Webhook {
                beat: label.to_owned(),
            });
        }
    };
    let spec = &spec;
    let rev = unit_revision(&beat.cadence, target, &ctx.machine_tz);
    match target {
        Target::Launchd if tz == &ctx.machine_tz => {
            Ok(vec![launchd::per_beat(ctx, label, spec, &rev)?])
        }
        Target::Launchd => {
            let deltas = (ctx.rendered_at).and_then(|now| zone_deltas(tz, &ctx.machine_tz, now));
            let Some(deltas) = deltas else {
                return Err(EmitRefusal::TzMismatch {
                    beat: label.to_owned(),
                    cadence_tz: tz.clone(),
                    machine_tz: ctx.machine_tz.clone(),
                });
            };
            Ok(vec![launchd::per_beat_shifted(
                ctx, label, spec, &deltas, &rev,
            )?])
        }
        Target::SystemdUser => Ok(systemd::per_beat(ctx, label, beat, tz, spec, &rev).into()),
    }
}

/// How many days ahead of the render the two zones' offsets are read (D10).
const ZONE_HORIZON_DAYS: i64 = 400;

/// The minutes the machine's clock can run ahead of the cadence's (negative when behind), one per
/// pair of the offsets each zone takes over the [`ZONE_HORIZON_DAYS`] after `now`, sorted. `None`
/// when a zone is not in the embedded tzdb.
fn zone_deltas(cadence_tz: &str, machine_tz: &str, now: jiff::Timestamp) -> Option<Vec<i32>> {
    let offsets = |zone: &jiff::tz::TimeZone| -> std::collections::BTreeSet<i32> {
        (0..=ZONE_HORIZON_DAYS)
            .filter_map(|day| {
                now.checked_add(jiff::SignedDuration::from_hours(24 * day))
                    .ok()
            })
            .map(|at| zone.to_offset(at).seconds() / 60)
            .collect()
    };
    let cadence = offsets(&crate::next::bundled_tz(cadence_tz)?);
    let machine = offsets(&crate::next::bundled_tz(machine_tz)?);
    let deltas: std::collections::BTreeSet<i32> = (machine.iter())
        .flat_map(|ahead| cadence.iter().map(move |behind| ahead - behind))
        .collect();
    Some(deltas.into_iter().collect())
}

/// The env-file path a unit names, if it loads one (D7).
///
/// A launchd beat unit hands it to its firer, `--env-file` then the path in
/// `ProgramArguments` (NIK-198); an older engine wrapped the firer in
/// `set -a && . '<path>' && set +a && exec …`, or `. '<path>' && exec …`
/// before that, both read too (XML-escaped in the plist); systemd carries
/// `EnvironmentFile=`. The VALUES never appear — only the path. `None` is
/// the short argv (`nika arm fire <label>`), the form that silently strips
/// provider keys if it replaces a unit that loads them.
#[must_use]
pub fn env_file_named_in_unit(body: &str) -> Option<String> {
    if let Some(path) = launchd_flag_env(body) {
        return Some(path);
    }
    let text = xml_unescape(body);
    if let Some(path) = launchd_sourced_env(&text) {
        return Some(path);
    }
    systemd_sourced_env(&text)
}

/// `<string>--env-file</string>` then `<string><path></string>` — the
/// firer's own flag in a launchd unit's `ProgramArguments`.
fn launchd_flag_env(body: &str) -> Option<String> {
    let mut lines = body.lines().map(str::trim);
    lines.find(|line| *line == "<string>--env-file</string>")?;
    let path = lines
        .next()?
        .strip_prefix("<string>")?
        .strip_suffix("</string>")?;
    Some(xml_unescape(path)).filter(|path| !path.is_empty())
}

/// `. '<path>' && exec …` — the launchd wrap. Scans every `. ` so a
/// `WorkingDirectory` that happens to contain that pair cannot steal the
/// match: only a POSIX-quoted path followed by ` && exec ` counts.
fn launchd_sourced_env(text: &str) -> Option<String> {
    let mut search = text;
    while let Some(at) = search.find(". ") {
        let rest = search.get(at + 2..)?;
        if let Some((path, after)) = split_sh_quoted(rest)
            && (after.starts_with(" && exec ") || after.starts_with(" && set +a && exec "))
            && !path.is_empty()
        {
            return Some(path);
        }
        search = rest;
    }
    None
}

/// `EnvironmentFile=` on the service — systemd's native D7 form.
fn systemd_sourced_env(text: &str) -> Option<String> {
    for line in text.lines() {
        let Some(path) = line.strip_prefix("EnvironmentFile=") else {
            continue;
        };
        let path = path.trim().replace("%%", "%");
        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}

/// Inverse of POSIX single-quoting for a prefix: `'…'` with `'\''` inside.
fn split_sh_quoted(text: &str) -> Option<(String, &str)> {
    let mut rest = text.strip_prefix('\'')?;
    let mut out = String::new();
    loop {
        let i = rest.find('\'')?;
        out.push_str(&rest[..i]);
        rest = rest.get(i + 1..)?;
        if let Some(after) = rest.strip_prefix("\\''") {
            out.push('\'');
            rest = after;
            continue;
        }
        return Some((out, rest));
    }
}

/// Inverse of the five XML escapes (`&amp;` last — it introduces the rest).
fn xml_unescape(text: &str) -> String {
    text.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
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
