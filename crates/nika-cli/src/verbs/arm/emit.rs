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

/// Project-arm sidecar remembering `--env-file` so a flag-less re-emit
/// keeps the D7 wrap (a short argv over a `. env && exec` unit strips
/// every provider key).
const ENV_FILE_SIDECAR: &str = ".nika/arm/env-file";

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
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    let root = path.parent().map_or_else(|| cwd.clone(), Path::to_path_buf);
    let dest = if args.write {
        match dest_dir(args, emit_target) {
            Ok(dir) => Some(dir),
            Err(out) => return out,
        }
    } else {
        None
    };
    let env_file = match resolve_env_file(args, &cwd, &root, dest.as_deref()) {
        Ok(file) => file,
        Err(out) => return out,
    };
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

    if let Some(file) = &ctx.env_file
        && (args.write || args.env_file.is_some())
        && let Err(out) = persist_env_file(&root, file)
    {
        return out;
    }
    match dest {
        Some(dir) => write_units(&dir, emit_target, &units, &skips, &ctx.log_dir),
        None => print_units(&units, &skips, emit_target),
    }
}

/// The binary the units invoke (D9 — ABSOLUTE): `--nika-bin` when
/// given, else `argv[0]` when it is absolute (the brew LINK stays stable
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

/// Flag, then the project-arm sidecar, then an existing unit's wrap.
/// A named path that cannot be read refuses — never a weaker unit.
fn resolve_env_file(
    args: &ArmArgs,
    cwd: &Path,
    root: &Path,
    dest: Option<&Path>,
) -> Result<Option<PathBuf>, VerbOutput> {
    if let Some(file) = &args.env_file {
        let file = absolute(file, cwd);
        return match std::fs::metadata(&file) {
            Ok(meta) if meta.is_file() => Ok(Some(file)),
            _ => Err(VerbOutput::env(format!(
                "arm --emit --env-file {} · illisible — les clés y vivent, il doit exister avant l'unité",
                file.display()
            ))),
        };
    }
    if let Some(file) = persisted_env_file(root)? {
        return Ok(Some(file));
    }
    match dest {
        Some(dir) => env_file_from_existing_units(dir),
        None => Ok(None),
    }
}

fn absolute(file: &Path, cwd: &Path) -> PathBuf {
    if file.is_absolute() {
        file.to_path_buf()
    } else {
        cwd.join(file)
    }
}

fn prove_readable(file: PathBuf) -> Result<PathBuf, VerbOutput> {
    match std::fs::metadata(&file) {
        Ok(meta) if meta.is_file() => Ok(file),
        _ => Err(weaker_refusal(Some(&file))),
    }
}

fn persisted_env_file(root: &Path) -> Result<Option<PathBuf>, VerbOutput> {
    let sidecar = root.join(ENV_FILE_SIDECAR);
    let text = match std::fs::read_to_string(&sidecar) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(VerbOutput::env(format!(
                "arm --emit · {}: {e}",
                sidecar.display()
            )));
        }
    };
    let line = text.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let file = PathBuf::from(line);
    let file = if file.is_absolute() {
        file
    } else {
        root.join(file)
    };
    prove_readable(file).map(Some)
}

fn persist_env_file(root: &Path, file: &Path) -> Result<(), VerbOutput> {
    let sidecar = root.join(ENV_FILE_SIDECAR);
    let parent = root.join(".nika/arm");
    if let Err(e) = std::fs::create_dir_all(&parent) {
        return Err(VerbOutput::env(format!(
            "arm --emit · {}: {e}",
            parent.display()
        )));
    }
    std::fs::write(&sidecar, format!("{}\n", file.display()))
        .map_err(|e| VerbOutput::env(format!("arm --emit · {}: {e}", sidecar.display())))
}

/// Read a dest that already carries the wrap — recovering the path is
/// what makes `--write` over field units non-destructive. The wrap is
/// the env-exec pattern itself (GENERATED header optional — stripping
/// the comment must not reopen a weaker overwrite). Two named paths
/// refuse rather than pick. A named path that is gone refuses rather
/// than emit the short argv over the wrap.
fn env_file_from_existing_units(dir: &Path) -> Result<Option<PathBuf>, VerbOutput> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(VerbOutput::env(format!(
                "arm --emit · {}: {e}",
                dir.display()
            )));
        }
    };
    let mut named: Option<PathBuf> = None;
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(e) => {
                return Err(VerbOutput::env(format!(
                    "arm --emit · {}: {e}",
                    dir.display()
                )));
            }
        };
        let Some(body) = unit_text(&path) else {
            continue;
        };
        match emit::env_file_named_in_unit(&body) {
            Some(raw) => {
                let candidate = PathBuf::from(raw);
                match &named {
                    None => named = Some(candidate),
                    Some(existing) if existing == &candidate => {}
                    Some(existing) => {
                        return Err(VerbOutput::file(format!(
                            "arm --emit · dest units name two different --env-file paths ({} vs {}) — refuse to pick · pass `nika arm --emit launchd --env-file <file>`",
                            existing.display(),
                            candidate.display()
                        )));
                    }
                }
            }
            None if body.contains(" && exec ") || body.contains("&amp;&amp; exec") => {
                return Err(weaker_refusal(None));
            }
            None => {}
        }
    }
    match named {
        None => Ok(None),
        Some(file) => prove_readable(file).map(Some),
    }
}

/// Bytes, then lossy UTF-8 — a binary plist still carries the wrap as
/// ASCII; `read_to_string` would skip it and reopen the strip.
fn unit_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Fail closed: never emit a short argv over a unit that sources keys.
fn weaker_refusal(file: Option<&Path>) -> VerbOutput {
    match file {
        Some(file) => VerbOutput::env(format!(
            "arm --emit · --env-file {} · illisible — un emit sans le drapeau enlèverait le wrap `. env && exec` (plus de clés) · remède: `nika arm --emit launchd --env-file {}`",
            file.display(),
            file.display()
        )),
        None => VerbOutput::env(
            "arm --emit · une unité source déjà un env file (`. env && exec`) · un emit sans --env-file l'enlèverait (plus de clés) · remède: `nika arm --emit launchd --env-file <fichier>`"
                .to_owned(),
        ),
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
    dir: &Path,
    target: EmitTarget,
    units: &[Unit],
    skips: &[String],
    log_dir: &Path,
) -> VerbOutput {
    if let Err(e) = std::fs::create_dir_all(dir).and_then(|()| std::fs::create_dir_all(log_dir)) {
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
        "\n{} · rien n'est chargé — la charge demeure ton geste:",
        crate::text::count(units.len(), "unité")
    );
    for command in load_commands(units, target, Some(dir)) {
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
