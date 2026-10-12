// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika arm disarm <label>` — law N4 made machine-real. Removing a line
//! does NOT disarm: the gesture is the beat's suspension in `nika.yaml`
//! (`actif: false` · `raison:` · `jusqu_au:`). `--write` then KEEPS the
//! beat's OS units, rewritten in place for the current binary and env
//! file: the firer journals the beat « inactive » until its `jusqu_au`,
//! then wakes it, so nothing has to re-arm it. Only this project's units
//! are ever touched; a unit without the GENERATED mark refuses the whole
//! gesture. « Absence never disarms — this does. »

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use nika_arm::unit_io::{self, ProjectUnits, UnitOwner};
use nika_cadence::emit::{self, EmitCtx, Target};
use nika_cadence::registry::{ArmRegistry, Beat};

use super::VerbOutput;
use super::emit::{canonical_project, machine_zone, unit_error};

/// `arm disarm <label>`: without `--write`, teach the N4 gesture; with it,
/// keep and rewrite the suspended beat's units, and journal the disarm.
#[must_use]
pub fn disarm(label: &str, write: bool) -> VerbOutput {
    if !write {
        return VerbOutput::ok(format!(
            "disarm `{label}` — law N4: removing the line does NOT disarm\n  \
             the gesture, in nika.yaml, on the beat's entry:\n  \
             · actif: false   — the declared intention\n  \
             · raison: \"…\"    — why it sleeps (a suspension is told)\n  \
             · jusqu_au: YYYY-MM-DD — when it wakes or is deleted\n  \
             then `nika arm disarm {label} --write` keeps its unit: it fires nothing until\n  \
             that date, when the beat wakes"
        ));
    }
    let Some(home) = std::env::home_dir() else {
        return VerbOutput::env("arm disarm --write · HOME introuvable".to_owned());
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // A unit carries its project's key: the gesture starts from the project.
    match nika_vocab::project::discover(&cwd) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return VerbOutput::file(format!(
                "arm disarm {label} --write · aucun projet ici — une unité porte la clé de son projet · remède: lance le geste depuis le projet du beat"
            ));
        }
        Err(e) => return VerbOutput::file(format!("arm disarm {label} · {e}")),
    }
    let (path, registry) = match super::load(&cwd) {
        Ok(loaded) => loaded,
        Err(out) => return out,
    };
    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };
    let (root, file, key) = match canonical_project(&path) {
        Ok(project) => project,
        Err(out) => return out,
    };
    let beat = match suspended_beat(&registry, label) {
        Ok(beat) => beat,
        Err(out) => return out,
    };
    let until = beat.jusqu_au.as_deref().unwrap_or("?");
    let project = ProjectUnits::new(key.clone(), file.clone());
    let (found, kept) = match owned_units(label, &home, &key, &project) {
        Ok(found) => found,
        Err(out) => return out,
    };
    if found.is_empty() {
        let mut text = format!(
            "arm disarm {label} · suspendu jusqu'au {until} — aucune unité émise: rien à garder · `nika arm --emit <cible> --write` la pose, et elle ne tirera rien jusque-là"
        );
        for line in &kept {
            let _ = write!(text, "\n{line}");
        }
        return VerbOutput::ok(text);
    }
    let place = Place {
        cwd: &cwd,
        root: &root,
        file: &file,
        key: &key,
        project: &project,
    };
    let mut out = match rewrite(&registry, label, &found, &place) {
        Ok(out) => out,
        Err(out) => return out,
    };
    for line in &kept {
        let _ = writeln!(out, "{line}");
    }
    let why = beat.raison.as_deref().unwrap_or("sans raison");
    let _ = writeln!(
        out,
        "{label} · suspendu jusqu'au {until} ({why}) — l'unité demeure et ne tire rien jusque-là: le tireur journalise « inactive », puis réveille le beat"
    );
    out.push_str(&journal_disarm(label, &root, until));
    VerbOutput::ok(out)
}

/// The beat `label` names, suspended in `nika.yaml`: the file is the
/// gesture (N4), so an active beat refuses with the gesture to write.
fn suspended_beat<'a>(registry: &'a ArmRegistry, label: &str) -> Result<&'a Beat, VerbOutput> {
    let labels = emit::labels(registry);
    let Some(beat) = registry
        .beats()
        .zip(labels.iter())
        .find_map(|(beat, name)| (name == label).then_some(beat))
    else {
        return Err(VerbOutput::file(format!(
            "arm disarm {label} · aucun beat `{label}` dans nika.yaml · les labels: {}",
            labels.join(", ")
        )));
    };
    if beat.is_active() {
        return Err(VerbOutput::file(format!(
            "arm disarm {label} --write · le beat est actif dans nika.yaml — loi N4: le geste est dans le fichier · ajoute à son entrée actif: false · raison: \"…\" · jusqu_au: AAAA-MM-JJ, puis relance: son unité demeure et ne tire rien jusqu'à cette date"
        )));
    }
    Ok(beat)
}

/// Where the project's units are rewritten from.
struct Place<'a> {
    cwd: &'a Path,
    root: &'a Path,
    file: &'a Path,
    key: &'a str,
    project: &'a ProjectUnits,
}

/// Rewrite the beat's units in each OS directory that holds one of this
/// project's (under the keyed name), and say an older engine's unkeyed
/// unit with the commands that retire it.
fn rewrite(
    registry: &ArmRegistry,
    label: &str,
    found: &[UnitFile],
    place: &Place<'_>,
) -> Result<String, VerbOutput> {
    let mut out = String::new();
    let mut done: Vec<Target> = Vec::new();
    for unit_file in found {
        if done.contains(&unit_file.target) {
            continue;
        }
        done.push(unit_file.target);
        let dir = unit_file
            .path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let ctx = context(place, &dir)?;
        let units = emit::render_beat(registry, &ctx, unit_file.target, label)
            .map_err(|refusal| VerbOutput::file(format!("arm disarm {label} · {refusal}")))?;
        unit_io::write_units(&dir, &ctx.log_dir, &units, place.project).map_err(unit_error)?;
        for unit in &units {
            let _ = writeln!(out, "réécrit {}", dir.join(&unit.file_name).display());
        }
        let labels = [label.to_owned()];
        for legacy in unit_io::legacy_units(&dir, &labels, unit_file.target, place.project) {
            if legacy.owner != UnitOwner::ThisProject {
                continue;
            }
            let _ = writeln!(
                out,
                "ancienne unité {} (sans la clé du projet) · retire-la:",
                legacy.unit
            );
            for command in
                unit_io::retire_commands(&legacy, unit_file.target).map_err(unit_error)?
            {
                let _ = writeln!(out, "  {command}");
            }
        }
    }
    Ok(out)
}

/// The emission context a rewrite renders with: the current binary, the
/// env file this project's units already source, the machine's zone.
fn context(place: &Place<'_>, dir: &Path) -> Result<EmitCtx, VerbOutput> {
    let nika_bin = unit_io::binary_path(None).map_err(unit_error)?;
    let env_file = unit_io::resolve_env_file(None, place.cwd, place.root, Some(dir), place.project)
        .map_err(unit_error)?;
    Ok(EmitCtx::new(
        nika_bin,
        place.root.to_path_buf(),
        place.file.to_path_buf(),
        place.key.to_owned(),
        env_file,
        place.root.join(".nika/arm/logs"),
        machine_zone()?,
    ))
}

/// One unit file of a beat in an OS unit directory.
struct UnitFile {
    path: PathBuf,
    target: Target,
}

/// The beat's unit files this project owns, and a line for each other
/// project's unit left in place. Both OS homes are scanned on ANY host —
/// a checkout can hold units written for another machine. READ ONLY: one
/// candidate without the GENERATED mark refuses the whole gesture before
/// anything is written.
fn owned_units(
    label: &str,
    home: &Path,
    key: &str,
    project: &ProjectUnits,
) -> Result<(Vec<UnitFile>, Vec<String>), VerbOutput> {
    let homes = [
        (home.join("Library/LaunchAgents"), Target::Launchd),
        (home.join(".config/systemd/user"), Target::SystemdUser),
    ];
    let mut present = Vec::new();
    let mut kept = Vec::new();
    for (dir, target) in homes {
        for unit in [emit::beat_unit(key, label), emit::legacy_beat_unit(label)] {
            for name in emit::beat_unit_files(target, &unit) {
                let path = dir.join(name);
                let body = match std::fs::read_to_string(&path) {
                    Ok(body) => body,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => {
                        return Err(VerbOutput::env(format!(
                            "arm disarm {label} · {}: {e}",
                            path.display()
                        )));
                    }
                };
                let owner = project.owner(&body);
                if owner == UnitOwner::ThisProject {
                    present.push(UnitFile { path, target });
                } else if let UnitOwner::Other(source) = owner {
                    kept.push(format!(
                        "· {} demeure — l'unité du projet {}",
                        path.display(),
                        source.display()
                    ));
                } else {
                    return Err(VerbOutput::file(format!(
                        "arm disarm {label} · {} ne porte pas la marque GENERATED — une unité étrangère, JAMAIS touchée · retire-la à la main si elle est à toi",
                        path.display()
                    )));
                }
            }
        }
    }
    Ok((present, kept))
}

/// Journal the disarm in the beat's history (N4 made machine-real). The
/// unit is already rewritten; a missing journal is SAID, never silent.
fn journal_disarm(label: &str, root: &Path, until: &str) -> String {
    let state = super::state::ArmState::at_project(root);
    match state.record_disarm(
        label,
        jiff::Zoned::now().timestamp(),
        std::process::id(),
        &format!("unité gardée jusqu'au {until}"),
    ) {
        Ok(_) => format!("· journalé: disarmed dans .nika/arm/{label}/history.ndjson\n"),
        Err(e) => format!("· historique NON journalé ({e}) — l'unité, elle, demeure\n"),
    }
}
