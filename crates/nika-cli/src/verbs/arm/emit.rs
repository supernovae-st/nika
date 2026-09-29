// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika arm --emit launchd|systemd` — « LE PONT », the bridge to the
//! OS (W3). The render is PURE (`nika_cadence::emit`); this verb owns
//! the I/O: the machine's zone (named, or a refusal — no guessing), the
//! binary's absolute path (D9), the print (the default) and the write
//! (`--write` — the local gesture). It NEVER spawns launchctl or
//! systemctl: the load commands are printed, the operator runs them.
//!

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_cadence::emit::{self, EmitCtx, Mode, Target, Unit};
use nika_cadence::registry::Locus;

use super::VerbOutput;
use super::args::{ArmArgs, EmitMode, EmitTarget};
use nika_arm::unit_io;

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
    let machine_tz = match jiff::tz::TimeZone::system().iana_name() {
        Some(name) => name.to_owned(),
        None => {
            return VerbOutput::env(
                "arm --emit · le fuseau de cette machine n'a pas de nom IANA — une unité y tirerait à une heure que personne n'a écrite · remède: nomme le fuseau de la machine (timedatectl · réglages)"
                    .to_owned(),
            );
        }
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
    let root = path.parent().map_or_else(|| cwd.clone(), Path::to_path_buf);
    let dest = if args.write {
        match unit_io::destination(
            platform_target(emit_target),
            args.out.as_deref(),
            std::env::home_dir(),
        )
        .map_err(unit_error)
        {
            Ok(dir) => Some(dir),
            Err(out) => return out,
        }
    } else {
        None
    };
    let env_file =
        match unit_io::resolve_env_file(args.env_file.as_deref(), &cwd, &root, dest.as_deref())
            .map_err(unit_error)
        {
            Ok(file) => file,
            Err(out) => return out,
        };
    let target = platform_target(emit_target);
    let ctx = EmitCtx::new(
        nika_bin,
        root.clone(),
        path,
        env_file,
        root.join(".nika/arm/logs"),
        machine_tz,
    );
    let units = match emit::render(&registry, &ctx, target, Mode::PerBeat) {
        Ok(units) => units,
        Err(refusal) => return VerbOutput::file(format!("arm --emit · {refusal}")),
    };

    // A cloud beat is skipped WITH its reason — the calendar stays the
    // operator's (« le cloud exécute, le calendrier demeure à toi »).
    let names = emit::labels(&registry);
    let skips: Vec<String> = registry
        .beats()
        .zip(names.iter())
        .filter(|(beat, _)| beat.is_active() && beat.locus() == Locus::Cloud)
        .map(|(_, label)| {
            format!("# sauté · {label} · où: cloud — le cloud exécute, le calendrier demeure au registre")
        })
        .collect();

    if let Some(file) = &ctx.env_file
        && (args.write || args.env_file.is_some())
        && let Err(out) = unit_io::persist_env_file(&root, file).map_err(unit_error)
    {
        return out;
    }
    match dest {
        Some(dir) => write_units(&dir, emit_target, &units, &skips, &ctx.log_dir),
        None => print_units(&units, &skips, emit_target),
    }
}

/// Print mode — the default. Units to stdout, the load commands with
/// the `~` home (the operator's shell expands it; nothing was written,
/// so nothing is owed a real path yet).
fn print_units(units: &[Unit], skips: &[String], target: EmitTarget) -> VerbOutput {
    if units.is_empty() {
        let mut text = "aucune unité à émettre — tout beat est suspendu ou cloud".to_owned();
        for skip in skips {
            let _ = write!(text, "\n{skip}");
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
/// parent), and the load commands carry the real paths.
fn write_units(
    dir: &Path,
    target: EmitTarget,
    units: &[Unit],
    skips: &[String],
    log_dir: &Path,
) -> VerbOutput {
    if let Err(error) = unit_io::write_units(dir, log_dir, units) {
        return unit_error(error);
    }
    let mut out = String::new();
    for unit in units {
        let _ = writeln!(out, "écrit {}", dir.join(&unit.file_name).display());
    }
    for skip in skips {
        let _ = writeln!(out, "{skip}");
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

/// `arm disarm <label>` — without `--write`, teach the N4 gesture (the
/// file-side suspension). With `--write`, remove the EMITTED unit only
/// (recognized by its GENERATED header — a foreign unit is NEVER
/// touched, and one foreign candidate refuses the WHOLE gesture), print
/// the bootout command, and journal the disarm in the beat's history.
/// « Absence never disarms — this does. »
#[must_use]
pub fn disarm(label: &str, write: bool) -> VerbOutput {
    if !write {
        return VerbOutput::ok(format!(
            "disarm `{label}` — law N4: removing the line does NOT disarm\n  \
             the gesture, in nika.yaml, on the beat's entry:\n  \
             · actif: false   — the declared intention\n  \
             · raison: \"…\"    — why it sleeps (a suspension is told)\n  \
             · jusqu_au: YYYY-MM-DD — when it wakes or is deleted"
        ));
    }
    let Some(home) = std::env::home_dir() else {
        return VerbOutput::env("arm disarm --write · HOME introuvable".to_owned());
    };
    // Both OS homes are scanned on ANY host — a checkout can hold units
    // written for another machine.
    let candidates = [
        home.join("Library/LaunchAgents")
            .join(format!("nika.arm.{label}.plist")),
        home.join(".config/systemd/user")
            .join(format!("nika.arm.{label}.timer")),
        home.join(".config/systemd/user")
            .join(format!("nika.arm.{label}.service")),
    ];
    // First pass, READ ONLY: every candidate that exists must carry the
    // GENERATED header before anything is removed.
    let mut present = Vec::new();
    for path in &candidates {
        match std::fs::read_to_string(path) {
            Ok(body) => {
                if !body.contains(emit::GENERATED_MARK) {
                    return VerbOutput::file(format!(
                        "arm disarm {label} · {} ne porte pas la marque GENERATED — une unité étrangère, JAMAIS touchée · retire-la à la main si elle est à toi",
                        path.display()
                    ));
                }
                present.push(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return VerbOutput::env(format!("arm disarm {label} · {}: {e}", path.display()));
            }
        }
    }
    if present.is_empty() {
        return VerbOutput::ok(format!(
            "arm disarm {label} · aucune unité émise — rien à démonter (le fichier demeure le geste: actif: false · raison: · jusqu_au:)"
        ));
    }
    let mut out = String::new();
    for path in &present {
        if let Err(e) = std::fs::remove_file(path) {
            return VerbOutput::env(format!("arm disarm {label} · {}: {e}", path.display()));
        }
        let _ = writeln!(out, "retiré {}", path.display());
    }
    let _ = writeln!(out, "décharge:");
    for command in bootout_commands(label, &present) {
        let _ = writeln!(out, "  {command}");
    }
    out.push_str(&journal_disarm(label));
    VerbOutput::ok(out)
}

/// The bootout commands — printed, never run. The `.service` rides its
/// timer's disable (no `[Install]` there).
fn bootout_commands(label: &str, removed: &[&PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for path in removed {
        let name = path.display().to_string();
        if std::path::Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("plist"))
        {
            out.push(format!("launchctl bootout gui/$UID {name}"));
        } else if std::path::Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("timer"))
        {
            out.push(format!(
                "systemctl --user disable --now nika.arm.{label}.timer"
            ));
        }
    }
    out
}

/// Journal the disarm in the beat's history (N4 made machine-real) —
/// when a project is here to hold the sidecar. The removal already
/// happened; a missing journal is SAID, never silent.
fn journal_disarm(label: &str) -> String {
    let Ok(cwd) = std::env::current_dir() else {
        return String::new();
    };
    let Ok(Some((path, _))) = nika_vocab::project::discover(&cwd) else {
        return "· historique non journalé — aucun projet ici (le sidecar vit à sa racine)\n"
            .to_owned();
    };
    let root = path.parent().map_or_else(|| cwd.clone(), Path::to_path_buf);
    let state = super::state::ArmState::at_project(&root);
    match state.record_disarm(
        label,
        jiff::Zoned::now().timestamp(),
        std::process::id(),
        "unité retirée",
    ) {
        Ok(_) => format!("· journalé: disarmed dans .nika/arm/{label}/history.ndjson\n"),
        Err(e) => format!("· historique NON journalé ({e}) — l'unité, elle, est retirée\n"),
    }
}

fn platform_target(target: EmitTarget) -> Target {
    match target {
        EmitTarget::Launchd => Target::Launchd,
        EmitTarget::Systemd => Target::SystemdUser,
    }
}

fn unit_error(error: unit_io::UnitIoError) -> VerbOutput {
    let input = error.is_input();
    let message = error.into_message();
    if input {
        VerbOutput::file(message)
    } else {
        VerbOutput::env(message)
    }
}
