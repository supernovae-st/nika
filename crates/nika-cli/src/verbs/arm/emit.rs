// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika arm --emit launchd|systemd` — « LE PONT », the bridge to the
//! OS (W3). The render is PURE (`nika_cadence::emit`); this verb owns
//! the I/O: the machine's zone (named, or a refusal — no guessing), the
//! binary's absolute path (D9), the print (the default) and the write
//! (`--write` — the local gesture). It NEVER spawns launchctl or
//! systemctl: the load commands are printed, the operator runs them.
//! Every unit name carries the project's key, so two projects arming a
//! beat of the same name never share, overwrite or disarm one unit.
//!

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_cadence::emit::{self, EmitCtx, Mode, Target, Unit};
use nika_cadence::registry::{ArmRegistry, Locus};

use super::VerbOutput;
use super::args::{ArmArgs, EmitMode, EmitTarget};
use nika_arm::readiness::ReceiptStatus;
use nika_arm::unit_io::{self, OrphanUnit, ProjectUnits, UnitOwner};

/// `arm --emit <OS>` — render the registry's units, print them (the
/// default) or write them (`--write`).
#[must_use]
pub fn run(args: &ArmArgs, emit_target: EmitTarget) -> VerbOutput {
    if matches!(args.mode, Some(EmitMode::System)) {
        return VerbOutput::file(
            "arm --emit --mode system · la portée root arrive avec serve (W5) — aujourd'hui: la session de l'opérateur (LaunchAgents · systemd --user)"
                .to_owned(),
        );
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (path, registry) = match super::load(&cwd) {
        Ok(loaded) => loaded,
        Err(out) => return out,
    };
    let machine_tz = match machine_zone() {
        Ok(zone) => zone,
        Err(out) => return out,
    };
    let nika_bin = match unit_io::binary_path(args.nika_bin.as_deref()).map_err(unit_error) {
        Ok(bin) => bin,
        Err(out) => return out,
    };
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    let (root, path, key) = match canonical_project(&path) {
        Ok(project) => project,
        Err(out) => return out,
    };
    let names = emit::labels(&registry);
    if let Some(out) = unknown_beat(args.beat.as_deref(), &names) {
        return out;
    }
    if let Some(out) = unready_refusal(&registry, &path, args.beat.as_deref()) {
        return out;
    }
    let project = ProjectUnits::new(key.clone(), path.clone());
    let (dest, env_file) =
        match placement(args, emit_target, &cwd, &root, &project).map_err(unit_error) {
            Ok(placed) => placed,
            Err(out) => return out,
        };
    let target = platform_target(emit_target);
    let ctx = EmitCtx::new(
        nika_bin,
        root.clone(),
        path,
        key,
        env_file,
        root.join(".nika/arm/logs"),
        machine_tz,
    )
    .at(jiff::Timestamp::now());
    // One beat's units alone, so a gesture on it is never refused for another (D1).
    let rendered = match args.beat.as_deref() {
        Some(label) => emit::render_beat(&registry, &ctx, target, label),
        None => emit::render(&registry, &ctx, target, Mode::PerBeat),
    };
    let units = match rendered {
        Ok(units) => units,
        Err(refusal) => return VerbOutput::file(format!("arm --emit · {refusal}")),
    };

    let mut skips = cloud_skips(&registry, &names);
    if let Some(label) = args.beat.as_deref() {
        skips.retain(|skip| skip.contains(&format!(" {label} ")));
    }
    // The beats the registry gives a unit now: every local beat, suspended
    // ones included. Any other unit under this project's key is an orphan.
    let armed: Vec<String> = (registry.beats().zip(names.iter()))
        .filter(|(beat, _)| beat.locus() == Locus::Local)
        .map(|(_, label)| label.clone())
        .collect();
    if let Some(file) = &ctx.env_file
        && (args.write || args.env_file.is_some())
        && let Err(out) = unit_io::persist_env_file(&root, file).map_err(unit_error)
    {
        return out;
    }
    if let Some(dir) = dest {
        let written = Written {
            units: &units,
            skips: &skips,
            labels: &names,
            armed: &armed,
            project: &project,
        };
        write_units(&dir, emit_target, &written, &ctx.log_dir)
    } else {
        // What `--write` would retire, read where it would write.
        let orphans = unit_io::destination(target, args.out.as_deref(), std::env::home_dir())
            .map(|dir| unit_io::orphan_units(&dir, &armed, target, &project))
            .unwrap_or_default();
        print_units(&units, &skips, emit_target, &orphans)
    }
}

/// `--beat` names a label `nika.yaml` does not hold: the refusal names the labels it does.
fn unknown_beat(beat: Option<&str>, names: &[String]) -> Option<VerbOutput> {
    let label = beat?;
    (!names.iter().any(|name| name == label)).then(|| {
        VerbOutput::file(format!(
            "arm --emit --beat {label} · aucun beat `{label}` dans nika.yaml · les labels: {}",
            names.join(", ")
        ))
    })
}

/// An UNREADY beat is never armed: its unit would fire a run that cannot
/// start (an unbound input, a missing workflow, an unpriced model). The
/// emission refuses whole, naming each such beat as `nika arm` does. A
/// suspended or cloud beat gets no unit and is not judged here.
fn unready_refusal(
    registry: &ArmRegistry,
    project_file: &Path,
    only: Option<&str>,
) -> Option<VerbOutput> {
    let now = jiff::Zoned::now();
    let labels = emit::labels(registry);
    let mut unready = 0usize;
    let mut lines = Vec::new();
    for (index, beat) in registry.beats().enumerate() {
        if !beat.is_active() || beat.locus() != Locus::Local {
            continue;
        }
        let label = labels.get(index).map_or("?", String::as_str);
        if only.is_some_and(|only| only != label) {
            continue;
        }
        if let Some(receipt) = super::readiness::receipt(registry, index, label, project_file, &now)
            && receipt.status() != ReceiptStatus::Ready
        {
            unready += 1;
            lines.extend(receipt.human_lines(0));
        }
    }
    if unready == 0 {
        return None;
    }
    let mut text = format!(
        "arm --emit · {} UNREADY — une unité n'arme jamais un beat qui ne peut pas tourner · remède: corrige-le (nika arm dit pourquoi), ou suspends-le dans nika.yaml (actif: false · raison: · jusqu_au:)",
        crate::text::count(unready, "beat")
    );
    for line in lines {
        let _ = write!(text, "\n{line}");
    }
    Some(VerbOutput::file(text))
}

/// Where `--write` lays the units (none in print mode), and the env file
/// they source.
fn placement(
    args: &ArmArgs,
    emit_target: EmitTarget,
    cwd: &Path,
    root: &Path,
    project: &ProjectUnits,
) -> Result<(Option<PathBuf>, Option<PathBuf>), unit_io::UnitIoError> {
    let dest = if args.write {
        Some(unit_io::destination(
            platform_target(emit_target),
            args.out.as_deref(),
            std::env::home_dir(),
        )?)
    } else {
        None
    };
    let env_file = unit_io::resolve_env_file(
        args.env_file.as_deref(),
        cwd,
        root,
        dest.as_deref(),
        project,
    )?;
    Ok((dest, env_file))
}

/// A cloud beat is skipped WITH its reason — the calendar stays the
/// operator's (« le cloud exécute, le calendrier demeure à toi »).
fn cloud_skips(registry: &ArmRegistry, names: &[String]) -> Vec<String> {
    registry
        .beats()
        .zip(names.iter())
        .filter(|(beat, _)| beat.is_active() && beat.locus() == Locus::Cloud)
        .map(|(_, label)| {
            format!("# sauté · {label} · où: cloud — le cloud exécute, le calendrier demeure au registre")
        })
        .collect()
}

/// The machine's IANA zone, launchd's firing zone (D10): a nameless zone
/// refuses, never a guess.
pub(super) fn machine_zone() -> Result<String, VerbOutput> {
    jiff::tz::TimeZone::system()
        .iana_name()
        .map(str::to_owned)
        .ok_or_else(|| {
            VerbOutput::env(
                "arm --emit · le fuseau de cette machine n'a pas de nom IANA — une unité y tirerait à une heure que personne n'a écrite · remède: nomme le fuseau de la machine (timedatectl · réglages)"
                    .to_owned(),
            )
        })
}

/// The canonical project: its root, its file, and the key every unit name
/// carries (the start of the root's fingerprint). Two spellings of one
/// project (a symlinked path) find the same units.
pub(super) fn canonical_project(file: &Path) -> Result<(PathBuf, PathBuf, String), VerbOutput> {
    let root = file.parent().unwrap_or_else(|| Path::new("."));
    let canonical = std::fs::canonicalize(root)
        .map_err(|e| VerbOutput::env(format!("arm · {}: {e}", root.display())))?;
    let fingerprint = nika_runtime::project_root_fingerprint(&canonical).ok_or_else(|| {
        VerbOutput::env(format!(
            "arm · {} ne se résout pas — aucune clé de projet pour nommer ses unités",
            canonical.display()
        ))
    })?;
    let file = file
        .file_name()
        .map_or_else(|| canonical.clone(), |name| canonical.join(name));
    Ok((canonical, file, emit::project_key(&fingerprint)))
}

/// What a `--write` lays down, and whose it is.
struct Written<'a> {
    units: &'a [Unit],
    skips: &'a [String],
    labels: &'a [String],
    /// The labels the registry gives a unit now.
    armed: &'a [String],
    project: &'a ProjectUnits,
}

/// Print mode — the default. Units to stdout, the load commands with
/// the `~` home (the operator's shell expands it; nothing was written,
/// so nothing is owed a real path yet).
fn print_units(
    units: &[Unit],
    skips: &[String],
    target: EmitTarget,
    orphans: &[OrphanUnit],
) -> VerbOutput {
    if units.is_empty() {
        let mut text =
            "aucune unité à émettre — tout beat est cloud (un beat suspendu garde la sienne)"
                .to_owned();
        for skip in skips {
            let _ = write!(text, "\n{skip}");
        }
        if let Err(error) = say_would_retire(&mut text, orphans, platform_target(target)) {
            return unit_error(error);
        }
        return VerbOutput::ok(text);
    }
    let mut out = String::new();
    for unit in units {
        let _ = writeln!(out, "# ── {}\n{}", unit.file_name, unit.body);
    }
    for skip in skips {
        let _ = writeln!(out, "{skip}");
    }
    let _ = writeln!(
        out,
        "\n{} · rien d'écrit — `--write` les pose",
        crate::text::count(units.len(), "unité")
    );
    if let Err(error) = say_would_retire(&mut out, orphans, platform_target(target)) {
        return unit_error(error);
    }
    let _ = writeln!(out, "charge:");
    let commands = match unit_io::load_commands(units, platform_target(target), None) {
        Ok(commands) => commands,
        Err(error) => return unit_error(error),
    };
    for command in commands {
        let _ = writeln!(out, "  {command}");
    }
    VerbOutput::ok(out)
}

/// Write mode — the local gesture. The units land where the OS reads
/// them (or `--out`), the log dir is created (launchd creates no
/// parent), and the load commands carry the real paths. An older
/// engine's unkeyed units are said: this project's with the commands that
/// retire them before the new ones load, any other left in place.
fn write_units(
    dir: &Path,
    target: EmitTarget,
    written: &Written<'_>,
    log_dir: &Path,
) -> VerbOutput {
    let units = written.units;
    if let Err(error) = unit_io::write_units(dir, log_dir, units, written.project) {
        return unit_error(error);
    }
    let mut out = String::new();
    for unit in units {
        let _ = writeln!(out, "écrit {}", dir.join(&unit.file_name).display());
    }
    for skip in written.skips {
        let _ = writeln!(out, "{skip}");
    }
    if let Err(error) = say_legacy(&mut out, dir, platform_target(target), written) {
        return unit_error(error);
    }
    if let Err(error) = retire_orphans(&mut out, dir, platform_target(target), written) {
        return unit_error(error);
    }
    let _ = writeln!(
        out,
        "\n{} · rien n'est chargé — la charge demeure ton geste:",
        crate::text::count(units.len(), "unité")
    );
    let commands = match unit_io::load_commands(units, platform_target(target), Some(dir)) {
        Ok(commands) => commands,
        Err(error) => return unit_error(error),
    };
    for command in commands {
        let _ = writeln!(out, "  {command}");
    }
    VerbOutput::ok(out)
}

/// The units an older engine wrote for this registry's beats under the
/// name without the project key.
fn say_legacy(
    out: &mut String,
    dir: &Path,
    target: Target,
    written: &Written<'_>,
) -> Result<(), unit_io::UnitIoError> {
    for legacy in unit_io::legacy_units(dir, written.labels, target, written.project) {
        if legacy.owner == UnitOwner::ThisProject {
            let _ = writeln!(
                out,
                "ancienne unité {} (sans la clé du projet) · retire-la avant de charger la nouvelle:",
                legacy.unit
            );
            for command in unit_io::retire_commands(&legacy, target)? {
                let _ = writeln!(out, "  {command}");
            }
        } else if let UnitOwner::Other(source) = &legacy.owner {
            let _ = writeln!(
                out,
                "ancienne unité {} · appartient au projet {} — jamais touchée (ce projet la retire en ré-émettant)",
                legacy.unit,
                source.display()
            );
        } else {
            let _ = writeln!(
                out,
                "ancienne unité {} · sans marque GENERATED — étrangère, jamais touchée",
                legacy.unit
            );
        }
    }
    Ok(())
}

/// Retire this project's units whose beat `nika.yaml` no longer arms: each
/// is said with the unload it needs, then its files are removed (`--write`
/// is the consent, as for writing; an emit never unloads, so the unload
/// commands, which work without the file, stay the operator's gesture). A
/// unit under this project's key whose header names another project, or
/// none, is said and never touched.
fn retire_orphans(
    out: &mut String,
    dir: &Path,
    target: Target,
    written: &Written<'_>,
) -> Result<(), unit_io::UnitIoError> {
    for orphan in unit_io::orphan_units(dir, written.armed, target, written.project) {
        if orphan.owner != UnitOwner::ThisProject {
            let _ = writeln!(
                out,
                "unité {} · sous la clé de ce projet mais sans sa marque GENERATED — jamais touchée",
                orphan.unit
            );
            continue;
        }
        let _ = writeln!(
            out,
            "retire {} · plus de beat `{}` armé dans nika.yaml — décharge-la:",
            orphan.unit, orphan.label
        );
        for command in unit_io::orphan_unload_commands(&orphan, target)? {
            let _ = writeln!(out, "  {command}");
        }
        match unit_io::remove_orphan(&orphan, written.project) {
            Ok(()) => {
                for path in &orphan.paths {
                    let _ = writeln!(out, "  supprimé {}", path.display());
                }
            }
            Err(error) => {
                let _ = writeln!(out, "  NON supprimée: {}", error.into_message());
            }
        }
    }
    Ok(())
}

/// What `--write` would retire, said in print mode: this project's units
/// whose beat is no longer armed, each with its files and its unload.
fn say_would_retire(
    out: &mut String,
    orphans: &[OrphanUnit],
    target: Target,
) -> Result<(), unit_io::UnitIoError> {
    for orphan in orphans {
        if orphan.owner != UnitOwner::ThisProject {
            continue;
        }
        let files: Vec<String> = (orphan.paths.iter())
            .map(|path| path.display().to_string())
            .collect();
        let _ = write!(
            out,
            "\n`--write` retirerait {} (plus de beat `{}` armé): supprime {} · décharge:",
            orphan.unit,
            orphan.label,
            files.join(" ")
        );
        for command in unit_io::orphan_unload_commands(orphan, target)? {
            let _ = write!(out, "\n  {command}");
        }
    }
    Ok(())
}

/// This project's installed beat units that wake a schedule `nika.yaml` no
/// longer gives (the cadence was edited, or a launchd unit's machine zone
/// changed), each with its file and the re-emit that fixes it; a unit
/// written before schedule revisions is said too. READ ONLY: both OS homes
/// are looked at on any host, as a checkout can hold either.
pub(super) fn stale_unit_lines(registry: &ArmRegistry, project_file: &Path) -> Vec<String> {
    let Some(home) = std::env::home_dir() else {
        return Vec::new();
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let file = if project_file.is_absolute() {
        project_file.to_path_buf()
    } else {
        cwd.join(project_file)
    };
    let Ok((_, _, key)) = canonical_project(&file) else {
        return Vec::new();
    };
    let zone = (jiff::tz::TimeZone::system().iana_name())
        .map(str::to_owned)
        .unwrap_or_default();
    let labels = emit::labels(registry);
    let mut lines = Vec::new();
    for (beat, label) in registry.beats().zip(labels.iter()) {
        if beat.locus() != Locus::Local {
            continue;
        }
        let unit = emit::beat_unit(&key, label);
        let installed = [
            (
                Target::Launchd,
                home.join("Library/LaunchAgents")
                    .join(format!("{unit}.plist")),
            ),
            (
                Target::SystemdUser,
                home.join(".config/systemd/user")
                    .join(format!("{unit}.timer")),
            ),
        ];
        for (target, path) in installed {
            let Ok(body) = std::fs::read_to_string(&path) else {
                continue;
            };
            let current = emit::unit_revision(&beat.cadence, target, &zone);
            match emit::revision_in_unit(&body) {
                Some(rev) if rev == current => {}
                Some(_) => lines.push(format!(
                    "  · {label} — {} réveille un horaire que nika.yaml ne donne plus ({}) · remède: `nika arm --emit {} --write`, puis recharge l'unité",
                    path.display(),
                    beat.cadence,
                    target.word()
                )),
                None => lines.push(format!(
                    "  · {label} — {} a été écrite avant les révisions d'horaire · ré-émets-la (`nika arm --emit {} --write`) pour que nika arm la compare à nika.yaml",
                    path.display(),
                    target.word()
                )),
            }
        }
    }
    lines
}

fn platform_target(target: EmitTarget) -> Target {
    match target {
        EmitTarget::Launchd => Target::Launchd,
        EmitTarget::Systemd => Target::SystemdUser,
    }
}

pub(super) fn unit_error(error: unit_io::UnitIoError) -> VerbOutput {
    let input = error.is_input();
    let message = error.into_message();
    if input {
        VerbOutput::file(message)
    } else {
        VerbOutput::env(message)
    }
}
