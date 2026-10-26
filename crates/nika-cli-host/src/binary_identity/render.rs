// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The binary-identity facts in the doctor's rows, the welcome's line and
//! the machine lane. Pure: every observation was made in the parent module.
//!
//! Prose never puts a word right after a bare `nika ` (the taught-commands
//! gate reads `nika <word>` as a verb): the command name is written `` `nika` ``,
//! and a path, which ends in `nika`, is followed by punctuation or a symbol.

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::path::Path;

use super::{Answer, BinaryIdentity, Entry, Installer, VERSION_DEADLINE, version_order};
use crate::doctor::{Finding, Level};

/// The fix when the order of two copies cannot be judged.
const SORT_FIX: &str =
    "which -a nika   # the first one runs: keep one copy, remove the others, then hash -r";

/// The fix for a copy reached through a relative PATH entry.
const RELATIVE_FIX: &str =
    "echo \"$PATH\"   # replace the relative entry with an absolute directory, then hash -r";

/// The doctor rows, after the `binary` row: the running binary, what typing
/// `nika` runs, then every other copy on PATH (a fix under each problem).
pub(crate) fn doctor_findings(id: &BinaryIdentity) -> Vec<Finding> {
    let show = |p: &Path| p.display().to_string();
    let winner = id.winner();
    let mut out = vec![running_finding(id, &show), first_finding(id, &show)];
    out.extend(
        id.entries
            .iter()
            .enumerate()
            .filter(|(i, e)| !e.running && Some(*i) != winner)
            .map(|(i, e)| entry_finding(id, i, e, &show)),
    );
    out
}

/// The welcome's line under `this binary`, and its notice when typing
/// `nika` runs another build or other copies on PATH differ.
pub(crate) fn welcome_lines(
    id: &BinaryIdentity,
    show: &dyn Fn(&Path) -> String,
) -> (String, Option<String>) {
    let line = match &id.path {
        Ok(path) => format!(
            "{} · installer: {} · {}",
            id.build.short(),
            id.installer.as_str(),
            show(path)
        ),
        Err(why) => format!(
            "{} · installer: {} · path unreadable ({why})",
            id.build.short(),
            id.installer.as_str()
        ),
    };
    (line, welcome_notice(id, show))
}

fn welcome_notice(id: &BinaryIdentity, show: &dyn Fn(&Path) -> String) -> Option<String> {
    let first = id.winner().and_then(|w| id.entries.get(w));
    if let Some(first) = first
        && !first.running
        && !same_build(first, id)
    {
        let at = show(&first.path);
        return Some(match &first.answer {
            Answer::Reported(build) => {
                format!(
                    "`nika` runs another build, {at} ({}) → nika doctor",
                    build.label()
                )
            }
            _ => format!("`nika` runs {at}, not this binary → nika doctor"),
        });
    }
    let differing = id
        .entries
        .iter()
        .filter(|e| {
            !e.running
                && match &e.answer {
                    Answer::Reported(build) => !build.same_build(&id.build),
                    Answer::TimedOut | Answer::Failed(_) => true,
                    Answer::NotExecutable | Answer::Relative => false,
                }
        })
        .count();
    (differing > 0).then(|| {
        format!(
            "{} of `nika` on PATH {} from this one → nika doctor",
            crate::text::count(differing, "other copy"),
            if differing == 1 { "differs" } else { "differ" }
        )
    })
}

/// Add `running` and `others[]` to a machine document (additive keys). A
/// document that is not an object is left as it is.
pub(crate) fn insert_json(value: &mut serde_json::Value, id: &BinaryIdentity) {
    let Some(document) = value.as_object_mut() else {
        return;
    };
    let running_at = id.running_at();
    document.insert(
        "running".to_owned(),
        serde_json::json!({
            "path": id.path.as_ref().ok().map(|p| p.display().to_string()),
            "path_error": id.path.as_ref().err(),
            "version": id.build.version,
            "commit": id.build.commit,
            "installer": id.installer.as_str(),
            "on_path": running_at.is_some(),
            "first_on_path": running_at.is_some() && running_at == id.winner(),
        }),
    );
    let others: Vec<serde_json::Value> = id
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.running)
        .map(|(i, e)| other_json(id, i, e))
        .collect();
    document.insert("others".to_owned(), others.into());
}

/// [`insert_json`] over a rendered document; a document that does not
/// parse is returned unchanged.
pub(crate) fn with_json(raw: String, id: &BinaryIdentity) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return raw;
    };
    insert_json(&mut value, id);
    format!("{value:#}")
}

fn other_json(id: &BinaryIdentity, i: usize, e: &Entry) -> serde_json::Value {
    let (probe, build, detail) = match &e.answer {
        Answer::Reported(build) => ("reported", Some(build), None),
        Answer::TimedOut => ("timed_out", None, None),
        Answer::Failed(why) => ("failed", None, Some(why.as_str())),
        Answer::NotExecutable => ("not_executable", None, None),
        Answer::Relative => (
            "not_probed",
            None,
            Some("a relative PATH entry is never executed"),
        ),
    };
    serde_json::json!({
        "path": e.path.display().to_string(),
        "resolved": e.resolved.as_ref().map(|p| p.display().to_string()),
        "installer": e.installer.as_str(),
        "executable": e.executable,
        "first_on_path": Some(i) == id.winner(),
        "shadows_running": id.shadows_running(i),
        "running_shadows_this": id.running_shadows(i),
        "probe": probe,
        "version": build.map(|b| b.version.clone()),
        "commit": build.and_then(|b| b.commit.clone()),
        "same_build": build.map(|b| b.same_build(&id.build)),
        "order": build
            .and_then(|b| version_order(&b.version, &id.build.version))
            .map(|o| match o {
                Ordering::Greater => "newer",
                Ordering::Less => "older",
                Ordering::Equal => "same-version",
            }),
        "detail": detail,
    })
}

fn row(level: Level, detail: String, fix: Option<String>) -> Finding {
    Finding {
        level,
        label: "path".to_owned(),
        detail,
        fix,
    }
}

fn running_finding(id: &BinaryIdentity, show: &dyn Fn(&Path) -> String) -> Finding {
    let (level, detail) = match &id.path {
        Ok(path) => (
            Level::Ok,
            format!(
                "{} · {} · installer: {}",
                show(path),
                id.build.label(),
                id.installer.as_str()
            ),
        ),
        Err(why) => (
            Level::Unknown,
            format!(
                "{} · this binary's path could not be read ({why})",
                id.build.label()
            ),
        ),
    };
    Finding {
        level,
        label: "running".to_owned(),
        detail,
        fix: None,
    }
}

/// What typing `nika` runs: this binary, the same build through another
/// file (a launcher or a copy), or another build, which warns.
fn first_finding(id: &BinaryIdentity, show: &dyn Fn(&Path) -> String) -> Finding {
    let Some(first) = id.winner().and_then(|w| id.entries.get(w)) else {
        let detail = if id.entries.is_empty() {
            "no `nika` on PATH · this binary was started by its path"
        } else {
            "no executable `nika` on PATH · this binary was started by its path"
        };
        return row(Level::Ok, detail.to_owned(), None);
    };
    if first.running {
        let others = id.entries.len().saturating_sub(1);
        let mut detail = "this binary is the first `nika` on PATH".to_owned();
        if others > 0 {
            let _ = write!(
                detail,
                " · {} on PATH, listed below",
                crate::text::count(others, "other `nika` file")
            );
        }
        return row(Level::Ok, detail, None);
    }
    let at = show(&first.path);
    match &first.answer {
        Answer::Reported(build) if build.same_build(&id.build) => row(
            Level::Ok,
            format!(
                "the first `nika` on PATH is {at}, another file that reports this same build ({}) · a launcher or a copy",
                build.label()
            ),
            None,
        ),
        Answer::Reported(build) => row(
            Level::Warn,
            format!(
                "typing `nika` runs {at} ({} · installer: {}), not this binary",
                build.label(),
                first.installer.as_str()
            ),
            Some(first_fix(id, first, &build.version, show)),
        ),
        Answer::Relative => row(
            Level::Warn,
            format!(
                "typing `nika` runs {at}, from a relative PATH entry, not this binary · not executed here"
            ),
            Some(RELATIVE_FIX.to_owned()),
        ),
        other => row(
            Level::Warn,
            format!(
                "typing `nika` runs {at}, not this binary · {}",
                answer_words(other)
            ),
            Some(SORT_FIX.to_owned()),
        ),
    }
}

/// The repair when another build runs first: remove it when it is older,
/// run its own doctor when it is newer (this binary is the stale one).
fn first_fix(
    id: &BinaryIdentity,
    first: &Entry,
    version: &str,
    show: &dyn Fn(&Path) -> String,
) -> String {
    match version_order(&id.build.version, version) {
        Some(Ordering::Greater) => remove_hint(first, show).map_or_else(
            || SORT_FIX.to_owned(),
            |remove| format!("{remove} && hash -r   # the older copy runs first on PATH"),
        ),
        Some(Ordering::Less) => format!(
            "{} doctor   # the newer copy that `nika` runs; this one is older",
            shell_word(&show(&first.path))
        ),
        _ => SORT_FIX.to_owned(),
    }
}

/// One other copy on PATH: its path, what it answered, its installer, and
/// where it stands against this binary.
fn entry_finding(
    id: &BinaryIdentity,
    i: usize,
    e: &Entry,
    show: &dyn Fn(&Path) -> String,
) -> Finding {
    let mut detail = show(&e.path);
    if let Some(resolved) = &e.resolved
        && resolved != &e.path
    {
        let _ = write!(detail, " → {}", show(resolved));
    }
    let _ = write!(
        detail,
        " · {} · installer: {}",
        answer_words(&e.answer),
        e.installer.as_str()
    );
    if e.executable {
        detail.push_str(if id.running_shadows(i) {
            " · shadowed by this binary"
        } else if id.shadows_running(i) && id.running_at().is_some() {
            " · before this binary on PATH"
        } else {
            " · further down PATH"
        });
    }
    let (level, fix) = match &e.answer {
        Answer::Reported(build) if build.same_build(&id.build) => (Level::Ok, None),
        Answer::Reported(build) => {
            let order = version_order(&build.version, &id.build.version);
            detail.push_str(match order {
                Some(Ordering::Less) => " · older than this binary",
                Some(Ordering::Greater) => " · newer than this binary",
                _ => " · another build",
            });
            (Level::Warn, Some(other_fix(e, order, show)))
        }
        Answer::TimedOut => (Level::Unknown, None),
        Answer::Failed(_) => (Level::Warn, Some(SORT_FIX.to_owned())),
        Answer::NotExecutable => (Level::Ok, None),
        Answer::Relative => (Level::Warn, Some(RELATIVE_FIX.to_owned())),
    };
    row(level, detail, fix)
}

/// The repair for a copy behind the one that runs: an older one goes, a
/// newer one is moved ahead.
fn other_fix(e: &Entry, order: Option<Ordering>, show: &dyn Fn(&Path) -> String) -> String {
    match order {
        Some(Ordering::Less) => remove_hint(e, show).map_or_else(
            || SORT_FIX.to_owned(),
            |remove| format!("{remove} && hash -r   # the older copy"),
        ),
        Some(Ordering::Greater) => {
            let dir = e.path.parent().unwrap_or(&e.path);
            format!(
                "export PATH={}:\"$PATH\" && hash -r   # the newer copy then runs first",
                shell_word(&show(dir))
            )
        }
        _ => SORT_FIX.to_owned(),
    }
}

/// The command that removes one copy, by its installer. A Nix store path
/// is removed from its profile, which this path does not name.
fn remove_hint(e: &Entry, show: &dyn Fn(&Path) -> String) -> Option<String> {
    Some(match e.installer {
        Installer::Homebrew => "brew uninstall nika".to_owned(),
        Installer::NpmGlobal => "npm uninstall -g @supernovae-st/nika".to_owned(),
        Installer::NpmPayload => "npm uninstall @supernovae-st/nika".to_owned(),
        Installer::Cargo => "cargo uninstall nika-cli".to_owned(),
        Installer::Nix => return None,
        Installer::InstallSh
        | Installer::ReleaseDir
        | Installer::SourceBuild
        | Installer::Unknown => format!("rm {}", shell_word(&show(&e.path))),
    })
}

/// What one copy answered, in words.
fn answer_words(answer: &Answer) -> String {
    match answer {
        Answer::Reported(build) => build.label(),
        Answer::TimedOut => format!(
            "no `--version` answer within {}s",
            VERSION_DEADLINE.as_secs()
        ),
        Answer::Failed(why) => format!("`--version` failed: {why}"),
        Answer::NotExecutable => "not executable, the shell skips it".to_owned(),
        Answer::Relative => "a relative PATH entry, not executed here".to_owned(),
    }
}

/// The first copy reports this binary's build.
fn same_build(e: &Entry, id: &BinaryIdentity) -> bool {
    matches!(&e.answer, Answer::Reported(build) if build.same_build(&id.build))
}

/// A path as one shell word: quoted only when it carries a character the
/// shell would split or expand.
fn shell_word(text: &str) -> String {
    let plain = text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+@%:,=~".contains(c));
    if plain && !text.is_empty() {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}
