// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika arm` — what this project has ARMED, and when each beat next
//! fires. Read-only: it schedules nothing, it says what the file
//! proposes (D-2026-08-11-N1 · THE FILE PROPOSES, THE MACHINE DISPOSES).
//!
//! ⭐ **THE CLOCK LIVES HERE.** `nika-cadence` is L0 and pure — its
//! calculator takes a `jiff::Zoned` and never reads one. Reading the
//! wall clock is an L4 act, so this verb owns it and the calculator
//! stays deterministic (its tests use literal instants, no clock to
//! drive).
//!
//! ## The arbitration the crate-spec deferred, resolved by measurement
//!
//! `nika-cadence` parses the WHOLE `nika.yaml`, and so does
//! `nika_vocab::project`. The crate-spec left three ways open: re-parse
//! the text, convert `&[ArmEntry] → Vec<Beat>`, or make cadence a pure
//! calculator. Writing the verb answered it:
//!
//! **The conversion is REFUTED.** `Beat` carries `chevauchement:` and
//! `après_saut:` — two policies `ArmEntry` does not have. Converting
//! would drop them silently, which is the worst of the three.
//!
//! So the two parsers stay, and they are NOT duplicates: `nika-vocab`
//! judges the project file's SHAPE (so a broken `arm:` refuses before
//! any spend, alongside `ceiling:` and `registry:`), `nika-cadence`
//! owns the cadence GRAMMAR and its policies. What they must never do
//! is DISAGREE about the same bytes — so this verb checks that they
//! counted the same beats, and refuses loudly if they did not.
//!
//! And the walk is not duplicated either: [`project::discover`] hands
//! back the PATH it found, so the cadence parser reads the file the
//! ladder already located (the one-walk law, shared with the ceiling ·
//! retention · registry rungs).

use nika_cadence::next::next_slots;
use nika_cadence::parse::{parse_registry, validate};
use nika_cadence::registry::Cadence;
use nika_vocab::project;

use super::VerbOutput;

/// How many upcoming slots each beat shows.
const SLOTS_SHOWN: usize = 3;

/// `nika arm` — the read-only arming report.
///
/// The CWD door. Discovery walks up from the invocation directory,
/// git-style — the same calm fallback the ceiling ladder takes when the
/// CWD is unresolvable (a broken CWD must never block a read).
#[must_use]
pub fn run() -> VerbOutput {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    run_at(&cwd)
}

/// The report at an explicit root — the tempdir-injectable half
/// (the `run::ceiling::ladder` precedent).
#[must_use]
pub fn run_at(start: &std::path::Path) -> VerbOutput {
    let found = match project::discover(start) {
        Ok(found) => found,
        Err(e) => return VerbOutput::file(format!("PROJECT ✗  {e}")),
    };
    let Some((path, project)) = found else {
        return VerbOutput::ok(
            "nothing armed — this project has no `nika.yaml`\n  \
             fix: `nika init --project-file` lays a commented starter"
                .to_owned(),
        );
    };
    // The cadence grammar reads the file the walk already located —
    // never a second discovery.
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return VerbOutput::env(format!("cannot read {}: {e}", path.display())),
    };
    let registry = match parse_registry(&text) {
        Ok(registry) => registry,
        Err(e) => return VerbOutput::file(format!("ARM ✗  {e}")),
    };

    // Every law, named. An empty walk IS the green verdict.
    let faults: Vec<String> = validate(&registry).map(|e| format!("  {e}")).collect();
    if !faults.is_empty() {
        return VerbOutput::file(format!(
            "ARM ✗  {} in {}\n{}",
            crate::text::count(faults.len(), "refusal"),
            path.display(),
            faults.join("\n")
        ));
    }

    // ⭐ The two parsers read the SAME bytes and must agree. They judge
    // different subsets on purpose, but a count that differs means one
    // of them saw an entry the other did not — and a file that passes
    // one gate and not the other is the fault this check exists to
    // refuse out loud rather than resolve silently.
    let (shape, grammar) = (project.arm().len(), registry.beat_count());
    if shape != grammar {
        return VerbOutput::file(format!(
            "ARM ✗  the two readers of {} disagree · the project shape counts {shape}, \
             the cadence grammar counts {grammar}\n  \
             this is an ENGINE fault, not a file fault — report it with the file",
            path.display()
        ));
    }

    report(&registry, &path)
}

/// The green report — one block per beat, and the next slots for each.
fn report(registry: &nika_cadence::registry::ArmRegistry, path: &std::path::Path) -> VerbOutput {
    use std::fmt::Write as _;

    if registry.beat_count() == 0 {
        return VerbOutput::ok(format!(
            "nothing armed — {} carries no `arm:` entry",
            path.display()
        ));
    }

    let now = jiff::Zoned::now();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} in {}",
        crate::text::count(registry.beat_count(), "beat"),
        path.display()
    );

    for beat in registry.beats() {
        let state = if beat.is_active() { "armed" } else { "idle " };
        let _ = writeln!(
            out,
            "\n  [{state}] {} · {}",
            beat.workflow,
            beat.cadence.trim()
        );
        // An inactive beat is REPORTED, never COMPUTED — asking a
        // disarmed beat for its next slot would print a date nobody
        // will ever see fire.
        if !beat.is_active() {
            continue;
        }
        match Cadence::parse(&beat.cadence) {
            Err(e) => {
                let _ = writeln!(out, "         ✗ {e}");
            }
            Ok(cadence) => {
                let mut any = false;
                for slot in next_slots(&cadence, &now, SLOTS_SHOWN) {
                    any = true;
                    let _ = writeln!(out, "         → {}", slot.at.strftime("%Y-%m-%d %H:%M %Z"));
                }
                if !any {
                    let _ = writeln!(
                        out,
                        "         → no upcoming slot (a webhook beat fires on its event)"
                    );
                }
            }
        }
    }
    let _ = write!(
        out,
        "\nnothing was scheduled — `nika arm` READS the file. \
         The machine that fires them is the arming edge."
    );
    VerbOutput::ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::verbs::exit;

    fn project_at(tag: &str, body: &str) -> tempfile::TempDir {
        let dir = tempfile::Builder::new()
            .prefix(&format!("nika-arm-{tag}-"))
            .tempdir()
            .expect("tmp dir");
        std::fs::write(dir.path().join("nika.yaml"), body).expect("project file");
        dir
    }

    const TWO_BEATS: &str = concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/weekly.nika.yaml\n",
        "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
        "  - workflow: workflows/nightly.nika.yaml\n",
        "    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n",
        "    plafond: 1.0\n",
        "    manqué: rattraper\n",
    );

    #[test]
    fn a_project_with_no_file_says_so_without_failing() {
        let dir = tempfile::Builder::new()
            .prefix("nika-arm-bare-")
            .tempdir()
            .expect("tmp dir");
        let out = run_at(dir.path());
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("nothing armed"), "{}", out.text);
    }

    #[test]
    fn each_armed_beat_names_its_next_slots() {
        let dir = project_at("two", TWO_BEATS);
        let out = run_at(dir.path());
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("2 beats"), "the count: {}", out.text);
        assert!(
            out.text.contains("workflows/weekly.nika.yaml"),
            "the workflow: {}",
            out.text
        );
        // Both cadence FORMS resolve, and the zone rides the expression.
        assert_eq!(
            out.text.matches("→ ").count(),
            2 * SLOTS_SHOWN,
            "three slots per armed beat: {}",
            out.text
        );
        // The verb SAYS it scheduled nothing — the file proposes.
        assert!(out.text.contains("READS the file"), "{}", out.text);
    }

    #[test]
    fn a_registry_that_breaks_a_law_refuses_with_its_line() {
        // `plafond:` is REQUIRED, no default — choosing for the operator
        // is choosing who pays.
        let dir = project_at(
            "nolaw",
            concat!(
                "nika: v1\n",
                "arm:\n",
                "  - workflow: w.nika.yaml\n",
                "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
                "    manqué: sauter\n",
            ),
        );
        let out = run_at(dir.path());
        assert_ne!(out.code, exit::OK, "a lawless registry must refuse");
    }

    /// ⭐ THE DIVERGENCE, PINNED — measured 2026-08-15.
    ///
    /// `nika-cadence::Beat` defines THIRTEEN keys; `nika_vocab::ArmEntry`
    /// accepts FIVE. And `nika_vocab` runs FIRST (it is what `discover`
    /// parses), so the other eight are UNREACHABLE through the project
    /// file: the cadence grammar defines them, validates them, and no
    /// author can ever write one.
    ///
    /// Among the unreachable: `actif:` — the disarm switch that law N4
    /// says is the ONLY way to disarm (« removing a line does NOT
    /// disarm ») — and `chevauchement:`/`après_saut:`, the whole of law
    /// ⑥. The overlap policy this verb reports is a policy nobody can
    /// set.
    ///
    /// This test does not FIX the gap (widening `ArmEntry` means
    /// duplicating cadence's enums at L0, narrowing `Beat` means
    /// deleting a grammar its crate was built for — an operator's
    /// arbitration, not a session's). It makes the gap VISIBLE, so
    /// closing it either way is a deliberate act and not a surprise.
    #[test]
    fn eight_cadence_keys_are_unreachable_through_the_project_file() {
        const CADENCE_ONLY: &[&str] = &[
            "chevauchement: sauter",
            "après_saut: prochain-créneau",
            "actif: false",
            "raison: \"pause\"",
            "jusqu_au: \"2026-12-31\"",
            "tolérance: \"5m\"",
            "décalage: \"1m\"",
            "par: \"thibaut\"",
        ];
        for key in CADENCE_ONLY {
            let body = format!(
                concat!(
                    "nika: v1\n",
                    "arm:\n",
                    "  - workflow: w.nika.yaml\n",
                    "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
                    "    plafond: 0.25\n",
                    "    manqué: sauter\n",
                    "    {}\n",
                ),
                key
            );
            // The cadence grammar KNOWS it …
            assert!(
                nika_cadence::parse::parse_registry(&body).is_ok(),
                "cadence must accept `{key}` — it is in its closed set"
            );
            // … and the project shape refuses it first.
            let dir = project_at("gap", &body);
            let out = run_at(dir.path());
            assert_ne!(
                out.code,
                exit::OK,
                "`{key}` reaches the cadence grammar today — the gap closed, \
                 update this pin (and say which way it closed)"
            );
        }
    }
}
