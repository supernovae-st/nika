// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `arm:` entries of a project's `nika.yaml`, rewritten as exact text: one entry's cadence in
//! another zone, or the entry removed, every other byte as it was. The Session proposes the
//! result as a project change, and its yes writes these bytes (live gauntlet wave 7 r01,
//! 2026-10-19). The words that ask for a removal are read here too; the phrases are NIK-204's
//! corpus.

use std::path::Path;

/// The words that remove, before the schedule's name (« retirez cette planification »).
const REMOVES: &[&str] = &[
    "retire",
    "retirez",
    "retirer",
    "supprime",
    "supprimez",
    "supprimer",
    "enlève",
    "enlevez",
    "enlever",
    "efface",
    "effacez",
    "effacer",
    "remove",
    "delete",
    "drop",
];

/// The schedule's name in a removal.
const SCHEDULE: &[&str] = &["planification", "planning", "programmation", "schedule"];

/// What may stand between the verb and the schedule's name.
const DETERMINERS: &[&str] = &[
    "la", "le", "cette", "ce", "sa", "son", "ma", "mon", "the", "this", "its", "my",
];

/// Whether a line asks to remove the declared schedule (« Alors retirez cette planification, s'il
/// vous plaît. », « supprime la planification », « remove the schedule »): a verb that removes,
/// then the schedule's name with at most its determiner between, in a short line.
#[must_use]
pub fn removes_the_schedule(line: &str) -> bool {
    let folded = line.replace('\u{2019}', "'").to_lowercase();
    let words: Vec<&str> = (folded
        .split(|c: char| c.is_whitespace() || matches!(c, '!' | ',' | '?' | ';' | '.')))
    .filter(|word| !word.is_empty())
    .collect();
    if words.len() > 12 {
        return false;
    }
    words.iter().enumerate().any(|(at, word)| {
        REMOVES.contains(word)
            && (words.get(at + 1..).unwrap_or_default().iter())
                .take_while(|next| DETERMINERS.contains(next) || SCHEDULE.contains(next))
                .any(|next| SCHEDULE.contains(next))
    })
}

/// Whether `line` opens the `arm:` entry of `workflow` (`- workflow: <path>`).
fn opens(line: &str, workflow: &Path) -> bool {
    (line.trim().strip_prefix("- workflow:"))
        .is_some_and(|named| Path::new(named.trim().trim_matches(['"', '\''])) == workflow)
}

/// The indentation of `line`, in bytes.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The person's words for when a workflow runs beside what its cadence means, said once when the
/// meaning repeats those words and only adds the zone: « tous les jours à 9h (heure de Maurice) »
/// for « tous les jours à 9h », never « tous les jours à 9h · tous les jours à 9h (heure de
/// Maurice) » (live gauntlet wave 8, 2026-10-19, r01 and r04). Other words stay beside the
/// meaning they were read as.
#[must_use]
pub fn said_and_meant(when: &str, meant: &str) -> String {
    let fold = |words: &str| {
        (words.split_whitespace())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let (said, meaning) = (fold(when), fold(meant));
    if !said.is_empty() && (meaning == said || meaning.starts_with(&format!("{said} ("))) {
        meant.to_owned()
    } else {
        format!("{when} · {meant}")
    }
}

/// `content` with the `cadence:` line of `workflow`'s entry set to `cadence`, every other byte as
/// it was; `None` when the entry or its cadence line is not found.
#[must_use]
pub fn with_cadence(content: &str, workflow: &Path, cadence: &str) -> Option<String> {
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let start = lines.iter().position(|line| opens(line, workflow))?;
    let end = entry_end(&lines, start);
    let at = (start + 1..end).find(|&k| lines[k].trim_start().starts_with("cadence:"))?;
    let line = lines[at];
    let newline = if line.ends_with('\n') { "\n" } else { "" };
    let replaced = format!("{}cadence: \"{cadence}\"{newline}", &line[..indent(line)]);
    let mut out: Vec<&str> = lines[..at].to_vec();
    out.push(&replaced);
    out.extend_from_slice(&lines[at + 1..]);
    Some(out.concat())
}

/// The line after `workflow`'s entry, which opens at `start`: the next one indented no more than
/// its `-` and not blank.
fn entry_end(lines: &[&str], start: usize) -> usize {
    let opened = indent(lines[start]);
    (start + 1..lines.len())
        .find(|&k| !lines[k].trim().is_empty() && indent(lines[k]) <= opened)
        .unwrap_or(lines.len())
}

/// `content` without `workflow`'s `arm:` entry, and without `arm:` once its list holds no entry;
/// `None` when no entry declares it.
#[must_use]
pub fn without_entry(content: &str, workflow: &Path) -> Option<String> {
    let lines: Vec<&str> = content.split_inclusive('\n').collect();
    let start = lines.iter().position(|line| opens(line, workflow))?;
    let end = entry_end(&lines, start);
    let mut kept: Vec<&str> = lines[..start].to_vec();
    kept.extend_from_slice(&lines[end..]);
    if let Some(arm) = kept.iter().position(|line| line.trim_end() == "arm:") {
        let listed = (kept[arm + 1..].iter())
            .take_while(|line| line.trim().is_empty() || line.starts_with([' ', '\t']))
            .any(|line| line.trim_start().starts_with("- "));
        if !listed {
            kept.remove(arm);
        }
    }
    Some(kept.concat())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    const FILE: &str = "nika: projet\narm:\n  - workflow: daily.nika\n    cadence: \"TZ=Indian/Mauritius 0 8 1 * *\"\n    plafond: 0.1\n    manqué: rattraper\n  - workflow: other.nika\n    cadence: \"TZ=Europe/Paris 0 9 * * *\"\n    plafond: 0.2\n    manqué: sauter\n";

    /// The entry's cadence is set in its new zone, every other byte as it was; another entry is
    /// untouched.
    #[test]
    fn a_zone_moves_only_the_entry_it_names() {
        let moved = with_cadence(FILE, Path::new("daily.nika"), "TZ=Europe/Paris 0 8 1 * *")
            .expect("the entry");
        assert_eq!(
            moved,
            FILE.replace("TZ=Indian/Mauritius 0 8 1 * *", "TZ=Europe/Paris 0 8 1 * *")
        );
        assert!(
            with_cadence(FILE, Path::new("absent.nika"), "TZ=Europe/Paris 0 8 1 * *").is_none()
        );
    }

    /// The entry goes with its fields; the list goes with its last entry.
    #[test]
    fn a_removal_takes_the_entry_and_an_empty_list() {
        let one = without_entry(FILE, Path::new("daily.nika")).expect("the entry");
        assert_eq!(
            one,
            "nika: projet\narm:\n  - workflow: other.nika\n    cadence: \"TZ=Europe/Paris 0 9 * * *\"\n    plafond: 0.2\n    manqué: sauter\n"
        );
        let none = without_entry(&one, Path::new("other.nika")).expect("the entry");
        assert_eq!(none, "nika: projet\n");
    }

    /// The removal's words, measured (live gauntlet wave 7 r01): a verb that removes, then the
    /// schedule's name; a removal of something else is not one (NIK-204's corpus).
    #[test]
    fn the_schedule_words_are_said_once_when_the_cadence_repeats_them() {
        let meant = "tous les jours à 9h (heure de Maurice)";
        assert_eq!(said_and_meant("tous les jours à 9h", meant), meant);
        assert_eq!(said_and_meant("Tous les jours  à 9h", meant), meant);
        let paris = "le 1er de chaque mois à 8h (heure de Paris)";
        assert_eq!(said_and_meant(paris, paris), paris);
        assert_eq!(
            said_and_meant(
                "tous les matins à 7h",
                "tous les jours à 7h (heure de Maurice)"
            ),
            "tous les matins à 7h · tous les jours à 7h (heure de Maurice)"
        );
    }

    #[test]
    fn the_words_that_remove_the_schedule() {
        for line in [
            "Alors retirez cette planification, s'il vous plait.",
            "supprime la planification",
            "Enlève le planning stp",
            "remove the schedule",
        ] {
            assert!(removes_the_schedule(line), "{line}");
        }
        for line in [
            "retire la ligne de planification du rapport",
            "désactive-le",
            "ajoute la planification au rapport",
            "quelle est la planification ?",
        ] {
            assert!(!removes_the_schedule(line), "{line}");
        }
    }
}
