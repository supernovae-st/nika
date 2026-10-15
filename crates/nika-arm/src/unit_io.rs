// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ARM unit context and host I/O; parsing, presentation and process exit stay at the interface.
//! These operations never load a unit. [`ProjectUnits`] tells this project's units from another
//! project's, which is never read for an env path, replaced or removed.

use nika_cadence::emit::{self, Target, Unit};
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// Refusal from explicit unit emission; input faults and host faults stay distinct.
#[derive(Debug)]
#[non_exhaustive]
pub struct UnitIoError {
    input: bool,
    message: String,
}

impl UnitIoError {
    fn host(message: String) -> Self {
        Self {
            input: false,
            message,
        }
    }
    fn input(message: String) -> Self {
        Self {
            input: true,
            message,
        }
    }
    /// Whether the operator's conflicting input, rather than host access, refused.
    #[must_use]
    pub fn is_input(&self) -> bool {
        self.input
    }

    /// Consume the refusal's presentation text at the interface edge.
    #[must_use]
    pub fn into_message(self) -> String {
        self.message
    }
}

impl std::fmt::Display for UnitIoError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for UnitIoError {}

/// One project's units in a unit directory: every file named with its key
/// ([`emit::project_key`]), and an older engine's unkeyed unit whose
/// GENERATED header names its project file. Another project's unit is
/// never read for an env path, overwritten or removed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ProjectUnits {
    key: String,
    project_file: PathBuf,
}

/// Whose unit a file in a unit directory is, by its GENERATED header.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnitOwner {
    /// The header names this project.
    ThisProject,
    /// The header names another project file.
    Other(PathBuf),
    /// No GENERATED mark: the unit was written or edited by hand.
    Unmarked,
}

impl ProjectUnits {
    /// A project's unit key and its canonical project file.
    #[must_use]
    pub fn new(key: String, project_file: PathBuf) -> Self {
        Self { key, project_file }
    }

    /// Whether `file_name` carries this project's key.
    #[must_use]
    pub fn keyed(&self, file_name: &str) -> bool {
        file_name.starts_with(&emit::beat_unit(&self.key, ""))
            || file_name.starts_with(&format!("{}.", emit::serve_unit(&self.key)))
    }

    /// Whose unit `body` is. An older engine wrote the project file as the
    /// operator's shell spelled it, so the header's path is canonicalized
    /// before the comparison.
    #[must_use]
    pub fn owner(&self, body: &str) -> UnitOwner {
        let Some(source) = emit::generated_source(body) else {
            return UnitOwner::Unmarked;
        };
        let source = Path::new(source);
        if source == self.project_file
            || std::fs::canonicalize(source).is_ok_and(|canonical| canonical == self.project_file)
        {
            UnitOwner::ThisProject
        } else {
            UnitOwner::Other(source.to_path_buf())
        }
    }

    /// Whether the unit file `file_name` holding `body` is this project's:
    /// its header names this project, or it carries this project's key and
    /// no mark (a stripped header leaves the unit its project's).
    fn holds(&self, file_name: &str, body: &str) -> bool {
        match self.owner(body) {
            UnitOwner::ThisProject => true,
            UnitOwner::Unmarked => self.keyed(file_name),
            UnitOwner::Other(_) => false,
        }
    }
}

/// An older engine's unit for one of this project's beats, under the name
/// without the project key ([`emit::legacy_beat_unit`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LegacyUnit {
    /// The old unit label (`nika.arm.<beat label>`).
    pub unit: String,
    /// The files found under the old name.
    pub paths: Vec<PathBuf>,
    /// Whose unit it is, by the first file's GENERATED header.
    pub owner: UnitOwner,
}

/// The units an older engine wrote in `dir` for `labels` on `target`.
/// An emit reports them and never removes one: the unload has to come
/// first, and an emit never unloads.
#[must_use]
pub fn legacy_units(
    dir: &Path,
    labels: &[String],
    target: Target,
    project: &ProjectUnits,
) -> Vec<LegacyUnit> {
    let mut found = Vec::new();
    for label in labels {
        let unit = emit::legacy_beat_unit(label);
        let mut paths = Vec::new();
        let mut owner = None;
        for file in emit::beat_unit_files(target, &unit) {
            let path = dir.join(file);
            if let Some(body) = unit_text(&path) {
                owner.get_or_insert_with(|| project.owner(&body));
                paths.push(path);
            }
        }
        if let Some(owner) = owner {
            found.push(LegacyUnit { unit, paths, owner });
        }
    }
    found
}

/// The commands that retire a legacy unit — PRINTED, never run: the
/// unload while the file still exists, then its removal.
///
/// # Errors
/// The platform is not supported by this host adapter.
pub fn retire_commands(legacy: &LegacyUnit, target: Target) -> Result<Vec<String>, UnitIoError> {
    let unload = match target {
        Target::Launchd => format!("launchctl bootout gui/$UID/{}", legacy.unit),
        Target::SystemdUser => format!("systemctl --user disable --now {}.timer", legacy.unit),
        _ => {
            return Err(UnitIoError::input(
                "arm --emit · unsupported unit platform".to_owned(),
            ));
        }
    };
    let files: Vec<String> = legacy
        .paths
        .iter()
        .map(|path| sh_quote(&path.display().to_string()))
        .collect();
    Ok(vec![unload, format!("rm {}", files.join(" "))])
}

/// One of this project's beat units in a unit directory that `nika.yaml` no
/// longer arms: its beat is gone, renamed, or now runs in the cloud.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OrphanUnit {
    /// The unit label (`nika.arm.<project key>.<beat label>`).
    pub unit: String,
    /// The beat label it was written for.
    pub label: String,
    /// Its files in the directory, in name order.
    pub paths: Vec<PathBuf>,
    /// Whose unit it is, by the first file's GENERATED header.
    pub owner: UnitOwner,
}

/// The units in `dir` named with this project's key whose beat label is not
/// in `armed` (the labels the registry gives a unit now), in name order.
/// READ ONLY: the caller retires only those whose header names this project.
#[must_use]
pub fn orphan_units(
    dir: &Path,
    armed: &[String],
    target: Target,
    project: &ProjectUnits,
) -> Vec<OrphanUnit> {
    let prefix = emit::beat_unit(&project.key, "");
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut units: std::collections::BTreeMap<String, Vec<PathBuf>> =
        std::collections::BTreeMap::new();
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(unit) = unit_stem(&name, target) else {
            continue;
        };
        let Some(label) = unit.strip_prefix(&prefix) else {
            continue;
        };
        if label.is_empty() || armed.iter().any(|armed| armed == label) {
            continue;
        }
        units.entry(unit.to_owned()).or_default().push(entry.path());
    }
    units
        .into_iter()
        .map(|(unit, mut paths)| {
            paths.sort();
            let owner = (paths.iter().find_map(|path| unit_text(path)))
                .map_or(UnitOwner::Unmarked, |body| project.owner(&body));
            let label = unit[prefix.len()..].to_owned();
            OrphanUnit {
                unit,
                label,
                paths,
                owner,
            }
        })
        .collect()
}

/// The unit label a file of `target` names: its name without the `.plist`,
/// `.timer` or `.service` extension.
fn unit_stem(file_name: &str, target: Target) -> Option<&str> {
    match target {
        Target::Launchd => file_name.strip_suffix(".plist"),
        Target::SystemdUser => {
            (file_name.strip_suffix(".timer")).or_else(|| file_name.strip_suffix(".service"))
        }
        _ => None,
    }
}

/// The commands that unload an orphan unit once its files are gone —
/// PRINTED, never run (an emit never unloads): launchd boots the label out,
/// systemd stops the loaded timer and service, then forgets the files.
///
/// # Errors
/// The platform is not supported by this host adapter.
pub fn orphan_unload_commands(
    orphan: &OrphanUnit,
    target: Target,
) -> Result<Vec<String>, UnitIoError> {
    match target {
        Target::Launchd => Ok(vec![format!("launchctl bootout gui/$UID/{}", orphan.unit)]),
        Target::SystemdUser => Ok(vec![
            format!(
                "systemctl --user stop {}.timer {}.service",
                orphan.unit, orphan.unit
            ),
            "systemctl --user daemon-reload".to_owned(),
        ]),
        _ => Err(UnitIoError::input(
            "arm --emit · unsupported unit platform".to_owned(),
        )),
    }
}

/// Remove an orphan unit's files, refusing one whose header does not name
/// this project (`project`): another project's unit, or one without the
/// GENERATED mark, is never removed.
///
/// # Errors
/// The unit is not this project's, or a file cannot be removed (the files
/// before it are already gone).
pub fn remove_orphan(orphan: &OrphanUnit, project: &ProjectUnits) -> Result<(), UnitIoError> {
    for path in &orphan.paths {
        let owner = unit_text(path).map_or(UnitOwner::Unmarked, |body| project.owner(&body));
        if owner != UnitOwner::ThisProject {
            return Err(UnitIoError::input(format!(
                "arm --emit --write · {} ne porte pas la marque GENERATED de ce projet — jamais supprimée",
                path.display()
            )));
        }
        std::fs::remove_file(path).map_err(|e| {
            UnitIoError::host(format!("arm --emit --write · {}: {e}", path.display()))
        })?;
    }
    Ok(())
}

/// POSIX single-quoting for a printed path.
fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Remembering the explicit environment path prevents a later emit from stripping its wrap.
const ENV_FILE_SIDECAR: &str = ".nika/arm/env-file";

/// Resolve explicit environment path, then persisted context, then the wrap
/// of one of this project's existing units (another project's wrap names
/// its own keys). A named path that cannot be read refuses — never a
/// weaker unit.
///
/// # Errors
/// A named file is unavailable or existing unit contexts disagree.
pub fn resolve_env_file(
    explicit: Option<&Path>,
    cwd: &Path,
    root: &Path,
    dest: Option<&Path>,
    project: &ProjectUnits,
) -> Result<Option<PathBuf>, UnitIoError> {
    if let Some(file) = explicit {
        let file = absolute(file, cwd);
        return match std::fs::metadata(&file) {
            Ok(meta) if meta.is_file() => Ok(Some(file)),
            _ => Err(UnitIoError::host(format!(
                "arm --emit --env-file {} · illisible — les clés y vivent, il doit exister avant l'unité",
                file.display()
            ))),
        };
    }
    if let Some(file) = persisted_env_file(root)? {
        return Ok(Some(file));
    }
    match dest {
        Some(dir) => env_file_from_existing_units(dir, project),
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

fn prove_readable(file: PathBuf) -> Result<PathBuf, UnitIoError> {
    match std::fs::metadata(&file) {
        Ok(meta) if meta.is_file() => Ok(file),
        _ => Err(weaker_refusal(Some(&file))),
    }
}

fn persisted_env_file(root: &Path) -> Result<Option<PathBuf>, UnitIoError> {
    let sidecar = root.join(ENV_FILE_SIDECAR);
    let text = match std::fs::read_to_string(&sidecar) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(UnitIoError::host(format!(
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

/// Persist an explicitly authorized unit environment path.
///
/// # Errors
/// The project sidecar cannot be created or written.
pub fn persist_env_file(root: &Path, file: &Path) -> Result<(), UnitIoError> {
    let sidecar = root.join(ENV_FILE_SIDECAR);
    let parent = root.join(".nika/arm");
    if let Err(e) = std::fs::create_dir_all(&parent) {
        return Err(UnitIoError::host(format!(
            "arm --emit · {}: {e}",
            parent.display()
        )));
    }
    std::fs::write(&sidecar, format!("{}\n", file.display()))
        .map_err(|e| UnitIoError::host(format!("arm --emit · {}: {e}", sidecar.display())))
}

/// Read this project's units in a dest that already carries the wrap —
/// recovering the path is what makes `--write` over field units
/// non-destructive. The wrap is the env-exec pattern itself (GENERATED
/// header optional on a unit named with this project's key — stripping
/// the comment must not reopen a weaker overwrite). Two named paths
/// refuse rather than pick. A named path that is gone refuses rather
/// than emit the short argv over the wrap.
fn env_file_from_existing_units(
    dir: &Path,
    project: &ProjectUnits,
) -> Result<Option<PathBuf>, UnitIoError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(UnitIoError::host(format!(
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
                return Err(UnitIoError::host(format!(
                    "arm --emit · {}: {e}",
                    dir.display()
                )));
            }
        };
        // Every unit a Nika engine writes is named `nika.…`: another
        // application's agent is never read.
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with("nika.") {
            continue;
        }
        let Some(body) = unit_text(&path) else {
            continue;
        };
        if !project.holds(name, &body) {
            continue;
        }
        match emit::env_file_named_in_unit(&body) {
            Some(raw) => {
                let candidate = PathBuf::from(raw);
                match &named {
                    None => named = Some(candidate),
                    Some(existing) if existing == &candidate => {}
                    Some(existing) => {
                        return Err(UnitIoError::input(format!(
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
fn weaker_refusal(file: Option<&Path>) -> UnitIoError {
    match file {
        Some(file) => UnitIoError::host(format!(
            "arm --emit · --env-file {} · illisible — un emit sans le drapeau enlèverait le wrap `. env && exec` (plus de clés) · remède: `nika arm --emit launchd --env-file {}`",
            file.display(),
            file.display()
        )),
        None => UnitIoError::host(
            "arm --emit · une unité source déjà un env file (`. env && exec`) · un emit sans --env-file l'enlèverait (plus de clés) · remède: `nika arm --emit launchd --env-file <fichier>`"
                .to_owned(),
        ),
    }
}

/// Write explicit units and create their log directory; never load them into the OS.
/// A unit file whose GENERATED header names another project is never
/// replaced: one such file refuses the whole write before anything lands.
///
/// # Errors
/// Another project's unit holds one of the names, or the destination, log
/// directory or a unit cannot be written. Earlier writes may exist.
pub fn write_units(
    dir: &Path,
    log_dir: &Path,
    units: &[Unit],
    project: &ProjectUnits,
) -> Result<(), UnitIoError> {
    for unit in units {
        let path = dir.join(&unit.file_name);
        if let Some(body) = unit_text(&path)
            && let UnitOwner::Other(source) = project.owner(&body)
        {
            return Err(UnitIoError::input(format!(
                "arm --emit --write · {} appartient au projet {} — jamais écrasée (ces deux racines partagent la clé {}) · remède: démonte-la depuis ce projet-là (`nika arm disarm <beat> --write`), ou déplace l'une des deux racines",
                path.display(),
                source.display(),
                project.key
            )));
        }
    }
    std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::create_dir_all(log_dir))
        .map_err(|e| UnitIoError::host(format!("arm --emit --write · {}: {e}", dir.display())))?;
    for unit in units {
        let path = dir.join(&unit.file_name);
        std::fs::write(&path, &unit.body).map_err(|e| {
            UnitIoError::host(format!("arm --emit --write · {}: {e}", path.display()))
        })?;
    }
    Ok(())
}

/// The load commands — PRINTED, never run (the bridge's honesty: the
/// operator's hands stay on the OS). launchd bootstraps each plist;
/// systemd enables the timers (the services ride along — no
/// `[Install]` there), and `serve` its service.
/// # Errors
/// The platform is not supported by this host adapter.
pub fn load_commands(
    units: &[Unit],
    target: Target,
    dir: Option<&Path>,
) -> Result<Vec<String>, UnitIoError> {
    let mut out = Vec::new();
    for unit in units {
        match target {
            Target::Launchd => {
                let path = dir.map_or_else(
                    || format!("~/Library/LaunchAgents/{}", unit.file_name),
                    |dir| dir.join(&unit.file_name).display().to_string(),
                );
                out.push(format!("launchctl bootstrap gui/$UID {path}"));
            }
            Target::SystemdUser => {
                let is_timer = std::path::Path::new(&unit.file_name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("timer"));
                let is_serve = unit.file_name.starts_with("nika.serve.");
                if is_timer || is_serve {
                    out.push(format!("systemctl --user enable --now {}", unit.file_name));
                }
            }
            _ => {
                return Err(UnitIoError::input(
                    "arm --emit · unsupported unit platform".to_owned(),
                ));
            }
        }
    }
    Ok(out)
}

/// Resolve the explicit destination or the platform's operator-owned unit directory.
///
/// # Errors
/// The default needs an unavailable home directory, or the platform is unsupported.
pub fn destination(
    target: Target,
    explicit: Option<&Path>,
    home: Option<PathBuf>,
) -> Result<PathBuf, UnitIoError> {
    if let Some(out) = explicit {
        return Ok(out.to_path_buf());
    }
    let home = home.ok_or_else(|| {
        UnitIoError::host("arm --emit --write · HOME introuvable — où poser l'unité ?".to_owned())
    })?;
    match target {
        Target::Launchd => Ok(home.join("Library/LaunchAgents")),
        Target::SystemdUser => Ok(home.join(".config/systemd/user")),
        _ => Err(UnitIoError::input(
            "arm --emit · unsupported unit platform".to_owned(),
        )),
    }
}

/// The binary the units invoke (D9 — ABSOLUTE): `--nika-bin` when
/// given, else `argv[0]` when it is absolute (the brew LINK stays stable
/// across upgrades), else the resolved exe.
///
/// # Errors
/// An explicit binary is relative, or the current executable cannot be resolved.
pub fn binary_path(explicit: Option<&Path>) -> Result<PathBuf, UnitIoError> {
    match explicit {
        Some(bin) if bin.is_absolute() => Ok(bin.to_path_buf()),
        Some(bin) => Err(UnitIoError::input(format!(
            "arm --emit --nika-bin {} · D9: le chemin du binaire est ABSOLU — l'unité survit au shell qui l'a posée",
            bin.display()
        ))),
        None => match std::env::args_os().next().map(PathBuf::from) {
            Some(argv0) if argv0.is_absolute() => Ok(argv0),
            _ => std::env::current_exe().map_err(|e| {
                UnitIoError::host(format!("arm --emit · impossible de nommer le binaire: {e}"))
            }),
        },
    }
}
