// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ARM unit context and host I/O; parsing, presentation and process exit stay at the interface.
//! These operations preserve the existing explicit emission behavior. They never load a unit.

use nika_cadence::emit::{self, Target, Unit};
use std::path::{Path, PathBuf};

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

/// Remembering the explicit environment path prevents a later emit from stripping its wrap.
const ENV_FILE_SIDECAR: &str = ".nika/arm/env-file";

/// Resolve explicit environment path, then persisted context, then an existing unit wrap.
/// A named path that cannot be read refuses — never a weaker unit.
///
/// # Errors
/// A named file is unavailable or existing unit contexts disagree.
pub fn resolve_env_file(
    explicit: Option<&Path>,
    cwd: &Path,
    root: &Path,
    dest: Option<&Path>,
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

/// Read a dest that already carries the wrap — recovering the path is
/// what makes `--write` over field units non-destructive. The wrap is
/// the env-exec pattern itself (GENERATED header optional — stripping
/// the comment must not reopen a weaker overwrite). Two named paths
/// refuse rather than pick. A named path that is gone refuses rather
/// than emit the short argv over the wrap.
fn env_file_from_existing_units(dir: &Path) -> Result<Option<PathBuf>, UnitIoError> {
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
///
/// # Errors
/// The destination, log directory or a unit cannot be written. Earlier writes may exist.
pub fn write_units(dir: &Path, log_dir: &Path, units: &[Unit]) -> Result<(), UnitIoError> {
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
                let is_serve = unit.file_name == "nika.serve.service";
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
