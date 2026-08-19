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
    let nika_bin = match nika_bin(args) {
        Ok(bin) => bin,
        Err(out) => return out,
    };
    let env_file = match env_file(args, &cwd) {
        Ok(file) => file,
        Err(out) => return out,
    };
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    let root = path.parent().map_or_else(|| cwd.clone(), Path::to_path_buf);
    let target = match emit_target {
        EmitTarget::Launchd => Target::Launchd,
        EmitTarget::Systemd => Target::SystemdUser,
    };
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

    if args.write {
        write_units(args, emit_target, &units, &skips, &ctx.log_dir)
    } else {
        print_units(&units, &skips, emit_target)
    }
}

/// The binary the units invoke (D9 — ABSOLUTE): `--nika-bin` when
/// given, else argv[0] when it is absolute (the brew LINK stays stable
/// across upgrades), else the resolved exe.
fn nika_bin(args: &ArmArgs) -> Result<PathBuf, VerbOutput> {
    match &args.nika_bin {
        Some(bin) if bin.is_absolute() => Ok(bin.clone()),
        Some(bin) => Err(VerbOutput::file(format!(
            "arm --emit --nika-bin {} · D9: le chemin du binaire est ABSOLU — l'unité survit au shell qui l'a posée",
            bin.display()
        ))),
        None => match std::env::args_os().next().map(PathBuf::from) {
            Some(argv0) if argv0.is_absolute() => Ok(argv0),
            _ => std::env::current_exe().map_err(|e| {
                VerbOutput::env(format!("arm --emit · impossible de nommer le binaire: {e}"))
            }),
        },
    }
}

/// The env file, made absolute and proven readable — D7: the provider
/// keys live THERE (the unit names the path, the values never cross),
/// so the file must exist before any unit does.
fn env_file(args: &ArmArgs, cwd: &Path) -> Result<Option<PathBuf>, VerbOutput> {
    let Some(file) = &args.env_file else {
        return Ok(None);
    };
    let file = if file.is_absolute() {
        file.clone()
    } else {
        cwd.join(file)
    };
    match std::fs::metadata(&file) {
        Ok(meta) if meta.is_file() => Ok(Some(file)),
        _ => Err(VerbOutput::env(format!(
            "arm --emit --env-file {} · illisible — les clés y vivent, il doit exister avant l'unité",
            file.display()
        ))),
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
    for command in load_commands(units, target, None) {
        let _ = writeln!(out, "  {command}");
    }
    VerbOutput::ok(out)
}

/// Write mode — the local gesture. The units land where the OS reads
/// them (or `--out`), the log dir is created (launchd creates no
/// parent), and the load commands carry the real paths.
fn write_units(
    args: &ArmArgs,
    target: EmitTarget,
    units: &[Unit],
    skips: &[String],
    log_dir: &Path,
) -> VerbOutput {
    let dir = match dest_dir(args, target) {
        Ok(dir) => dir,
        Err(out) => return out,
    };
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::create_dir_all(log_dir)) {
        return VerbOutput::env(format!("arm --emit --write · {}: {e}", dir.display()));
    }
    let mut out = String::new();
    for unit in units {
        let path = dir.join(&unit.file_name);
        if let Err(e) = std::fs::write(&path, &unit.body) {
            return VerbOutput::env(format!("arm --emit --write · {}: {e}", path.display()));
        }
        let _ = writeln!(out, "écrit {}", path.display());
    }
    for skip in skips {
        let _ = writeln!(out, "{skip}");
    }
    let _ = writeln!(
        out,
        "\n{} posée · rien n'est chargé — la charge demeure ton geste:",
        crate::text::count(units.len(), "unité")
    );
    for command in load_commands(units, target, Some(&dir)) {
        let _ = writeln!(out, "  {command}");
    }
    VerbOutput::ok(out)
}

/// Where the OS reads user units (per TARGET, on any host — writing a
/// systemd pair from a mac for a linux box is a real gesture).
fn dest_dir(args: &ArmArgs, target: EmitTarget) -> Result<PathBuf, VerbOutput> {
    if let Some(out) = &args.out {
        return Ok(out.clone());
    }
    let Some(home) = std::env::home_dir() else {
        return Err(VerbOutput::env(
            "arm --emit --write · HOME introuvable — où poser l'unité ?".to_owned(),
        ));
    };
    Ok(match target {
        EmitTarget::Launchd => home.join("Library/LaunchAgents"),
        EmitTarget::Systemd => home.join(".config/systemd/user"),
    })
}

/// The load commands — PRINTED, never run (the bridge's honesty: the
/// operator's hands stay on the OS). launchd bootstraps each plist;
/// systemd enables the timers (the services ride along — no
/// `[Install]` there), and `serve` its service.
fn load_commands(units: &[Unit], target: EmitTarget, dir: Option<&Path>) -> Vec<String> {
    let mut out = Vec::new();
    for unit in units {
        match target {
            EmitTarget::Launchd => {
                let path = dir.map_or_else(
                    || format!("~/Library/LaunchAgents/{}", unit.file_name),
                    |dir| dir.join(&unit.file_name).display().to_string(),
                );
                out.push(format!("launchctl bootstrap gui/$UID {path}"));
            }
            EmitTarget::Systemd => {
                let is_timer = std::path::Path::new(&unit.file_name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("timer"));
                let is_serve = unit.file_name == "nika.serve.service";
                if is_timer || is_serve {
                    out.push(format!("systemctl --user enable --now {}", unit.file_name));
                }
            }
        }
    }
    out
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
    let entry = super::state::HistoryEntry {
        slot: None,
        decided_at: jiff::Zoned::now().timestamp(),
        kind: super::state::FireKind::Disarmed,
        reason: Some("unité retirée".to_owned()),
        trace: None,
        exit: None,
        slots: None,
    };
    match state.record(label, &entry) {
        Ok(()) => format!("· journalé: disarmed dans .nika/arm/{label}/history.ndjson\n"),
        Err(e) => format!("· historique NON journalé ({e}) — l'unité, elle, est retirée\n"),
    }
}
