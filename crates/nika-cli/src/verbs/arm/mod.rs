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
//! **The conversion is REFUTED.** `Beat` judges the VALUES of the
//! cadence grammar's thirteen keys; `ArmEntry` carries eight of them
//! VERBATIM (vocab judges the shape, cadence the law — law 8, deux
//! parseurs jamais en désaccord). Converting one type into the other
//! would duplicate the value judgments silently, which is the worst of
//! the three.
//!
//! So the two parsers stay, and they are NOT duplicates: `nika-vocab`
//! judges the project file's SHAPE (so a broken `arm:` refuses before
//! any spend, alongside `ceiling:` and `registry:`), `nika-cadence`
//! owns the cadence GRAMMAR and its policies. What they must never do
//! is DISAGREE about the same bytes — so this verb checks that they
//! counted the same beats, and refuses loudly if they did not. (Until
//! 2026-08-19 the vocab set was five keys and cadence's eight others
//! were unreachable through the file — the divergence the flipped test
//! below now pins CLOSED.)
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

pub mod args;
pub mod emit;
pub mod fire;
pub mod migrate;
pub use nika_arm::state;

/// How many upcoming slots each beat shows.
const SLOTS_SHOWN: usize = 3;

/// `nika arm` — the verb. Bare (no subcommand, no flag) it is the
/// read-only arming report below; the subcommands are the machine's
/// edge, and `--emit` is the W3 bridge to the OS ([`mod@emit`]).
#[must_use]
pub fn run(args: args::ArmArgs) -> VerbOutput {
    use args::ArmSub;
    match args.sub {
        Some(ArmSub::Fire(f)) => fire::run(&f),
        Some(ArmSub::Migrate) => migrate::run(),
        Some(ArmSub::Disarm { label, write }) => emit::disarm(&label, write),
        None => {
            if let Some(target) = args.emit {
                return emit::run(&args, target);
            }
            if emits_requested(&args) {
                return VerbOutput::file(
                    "arm --write · --out · --mode · --env-file · --nika-bin vivent avec --emit — `nika arm --emit launchd` (ou systemd)"
                        .to_owned(),
                );
            }
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            run_at(&cwd)
        }
    }
}

/// Any `--emit`-family flag set WITHOUT `--emit`? Those flags only make
/// sense WITH the emission — set alone they refuse rather than silently
/// report (a flag that does nothing is a lie).
fn emits_requested(args: &args::ArmArgs) -> bool {
    args.write
        || args.out.is_some()
        || args.mode.is_some()
        || args.env_file.is_some()
        || args.nika_bin.is_some()
}

/// The shared door — every arming edge walks it (the report below ·
/// the W3 `--emit`): discover the project file the ladder locates, read
/// it, parse + law-check the cadence grammar, and verify the two
/// readers AGREE on the beat count (law 8 — deux parseurs, jamais en
/// désaccord). `Err` carries the output to hand back AS-IS, whatever
/// its code.
pub(crate) fn load(
    start: &std::path::Path,
) -> Result<(std::path::PathBuf, nika_cadence::registry::ArmRegistry), VerbOutput> {
    let found = match project::discover(start) {
        Ok(found) => found,
        Err(e) => return Err(VerbOutput::file(format!("PROJECT ✗  {e}"))),
    };
    let Some((path, project)) = found else {
        return Err(VerbOutput::ok(
            "nothing armed — this project has no `nika.yaml`\n  \
             fix: `nika init --project-file` lays a commented starter"
                .to_owned(),
        ));
    };
    // The cadence grammar reads the file the walk already located —
    // never a second discovery.
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            return Err(VerbOutput::env(format!(
                "cannot read {}: {e}",
                path.display()
            )));
        }
    };
    let registry = match parse_registry(&text) {
        Ok(registry) => registry,
        Err(e) => return Err(VerbOutput::file(format!("ARM ✗  {e}"))),
    };

    // Every law, named. An empty walk IS the green verdict.
    let faults: Vec<String> = validate(&registry).map(|e| format!("  {e}")).collect();
    if !faults.is_empty() {
        return Err(VerbOutput::file(format!(
            "ARM ✗  {} in {}\n{}",
            crate::text::count(faults.len(), "refusal"),
            path.display(),
            faults.join("\n")
        )));
    }

    // ⭐ The two parsers read the SAME bytes and must agree. They judge
    // different subsets on purpose, but a count that differs means one
    // of them saw an entry the other did not — and a file that passes
    // one gate and not the other is the fault this check exists to
    // refuse out loud rather than resolve silently.
    let (shape, grammar) = (project.arm().len(), registry.beat_count());
    if shape != grammar {
        return Err(VerbOutput::file(format!(
            "ARM ✗  the two readers of {} disagree · the project shape counts {shape}, \
             the cadence grammar counts {grammar}\n  \
             this is an ENGINE fault, not a file fault — report it with the file",
            path.display()
        )));
    }
    Ok((path, registry))
}

/// The report at an explicit root — the tempdir-injectable half
/// (the `run::ceiling::ladder` precedent).
#[must_use]
pub fn run_at(start: &std::path::Path) -> VerbOutput {
    let (path, registry) = match load(start) {
        Ok(loaded) => loaded,
        Err(out) => return out,
    };
    report(&registry, &path)
}

/// The green report — one block per beat, and the next slots for each.
///
/// The block also tells PROUVÉ from DÉCLARÉ (law N3's honesty: the
/// registry DECLARES, only the sidecar PROVES the machine fired), and
/// after the beats the ORPHELINS — the sidecar directories no registry
/// entry names (law N4: reported, NEVER erased).
fn report(registry: &nika_cadence::registry::ArmRegistry, path: &std::path::Path) -> VerbOutput {
    use std::fmt::Write as _;

    if registry.beat_count() == 0 {
        return VerbOutput::ok(format!(
            "nothing armed — {} carries no `arm:` entry",
            path.display()
        ));
    }

    let root = path.parent().map_or_else(
        || std::path::PathBuf::from("."),
        std::path::Path::to_path_buf,
    );
    let sidecar = state::ArmState::at_project(&root);
    let labels = fire::labels(registry);
    let now = jiff::Zoned::now();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} in {}",
        crate::text::count(registry.beat_count(), "beat"),
        path.display()
    );

    for (index, beat) in registry.beats().enumerate() {
        let state = if beat.is_active() { "armed" } else { "idle " };
        let _ = writeln!(
            out,
            "\n  [{state}] {} · {}",
            beat.workflow,
            beat.cadence.trim()
        );
        if let Err(error) = proof_line(
            &sidecar,
            labels.get(index).map_or("?", String::as_str),
            beat,
            &now.timestamp(),
            &mut out,
        ) {
            return VerbOutput::env(format!(
                "arm report refused · corrupt sidecar for {}: {error}",
                labels.get(index).map_or("?", String::as_str)
            ));
        }
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

    let orphans = sidecar.orphans(&labels);
    if !orphans.is_empty() {
        let _ = writeln!(out, "\norphelins (N4 — rapportés, JAMAIS effacés):");
        for name in orphans {
            let _ = writeln!(out, "  · {name} — .nika/arm/{name}/");
        }
    }

    let _ = write!(
        out,
        "\nnothing was scheduled — `nika arm` READS the file. \
         The machine that fires them is the arming edge."
    );
    VerbOutput::ok(out)
}

/// The proof line of one beat: `✓ PROUVÉ` when the sidecar attests the
/// machine fired (last.json), `DÉCLARÉ` when only the registry speaks.
/// The history's tallies (`x sauts / y tirs`) and the declared
/// `tolérance: m/k` ride the same line when they exist; `par:` is
/// shown for what it is — déclaré, non vérifié (N3).
fn proof_line(
    sidecar: &state::ArmState,
    label: &str,
    beat: &nika_cadence::Beat,
    now: &jiff::Timestamp,
    out: &mut String,
) -> std::io::Result<()> {
    use std::fmt::Write as _;

    let mut line = match sidecar.last(label)? {
        Some(last) => {
            let generation = last
                .generation
                .as_ref()
                .map_or(String::new(), |generation| {
                    format!(" · gen {}", generation.short())
                });
            format!(
                "✓ PROUVÉ · {} · {} · slot {}{generation}",
                last.kind.as_str(),
                last.fired_at,
                last.slot
            )
        }
        None => "· DÉCLARÉ — le registre le dit, la machine ne l'a jamais tiré".to_owned(),
    };
    if let Some(folded) = sidecar.folded(label, now)? {
        let lifecycle =
            folded
                .slot()
                .and_then(|slot| slot.get(..8))
                .map_or(String::new(), |slot| {
                    if folded.is_beyond_last() {
                        format!(" · slot courant {slot}")
                    } else {
                        String::new()
                    }
                });
        let _ = write!(line, " · état {}{lifecycle}", folded.state().as_str());
    }
    if let Some((skips, fires)) = sidecar.tallies(label) {
        let _ = write!(
            line,
            " · {} / {}",
            crate::text::count(skips, "saut"),
            crate::text::count(fires, "tir")
        );
    }
    if let Some(tol) = &beat.tolerance {
        let _ = write!(line, " · tolérance {tol}");
    }
    let _ = writeln!(out, "         {line}");
    if let Some(par) = &beat.par {
        let _ = writeln!(out, "         par: {par} — déclaré · non vérifié");
    }
    Ok(())
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

    /// The bare dispatch: no subcommand, no flag → the report. The
    /// report half itself is pinned by the `run_at` tests (the
    /// refactor's zero-behavior-change claim); the CWD door is covered
    /// end-to-end by the binary tests (`tests/arm_fire.rs` ·
    /// `tests/arm_emit.rs`), never by moving the test process's own CWD
    /// (parallel tests race on it). A `--emit`-family flag WITHOUT
    /// `--emit` refuses honestly — a flag that does nothing is a lie.
    #[test]
    fn emit_flags_without_emit_refuse() {
        let base = crate::verbs::arm::args::ArmArgs {
            sub: None,
            emit: None,
            write: true,
            out: None,
            mode: None,
            env_file: None,
            nika_bin: None,
        };
        let out = run(base);
        assert_eq!(out.code, exit::FILE, "--write alone refuses: {}", out.text);
        assert!(out.text.contains("--emit"), "names the flag: {}", out.text);
    }

    /// The teaching half (no `--write`) is unit-testable here; the
    /// teardown half needs a redirected HOME, so it rides
    /// `tests/arm_emit.rs` in a spawned process.
    #[test]
    fn disarm_teaches_the_n4_gesture() {
        let teach = emit::disarm("doctor", false);
        assert_eq!(teach.code, exit::OK, "{}", teach.text);
        assert!(teach.text.contains("actif: false"), "{}", teach.text);
        assert!(teach.text.contains("jusqu_au"), "{}", teach.text);
    }

    /// The report tells PROUVÉ from DÉCLARÉ and names the ORPHELINS —
    /// the three states in one tempdir, snapshotted. The beats sleep
    /// (`actif: false`): an idle beat is REPORTED, never computed, so
    /// the text is deterministic (no next-slot line rides the clock).
    #[test]
    fn the_report_tells_proven_from_declared_and_names_the_orphans() {
        let body = concat!(
            "nika: v1\n",
            "arm:\n",
            "  - workflow: workflows/prouve.nika.yaml\n",
            "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "    actif: false\n",
            "    raison: \"pause estivale\"\n",
            "    jusqu_au: \"2099-12-31\"\n",
            "    tolérance: \"3/4\"\n",
            "    par: \"thibaut\"\n",
            "  - workflow: workflows/declare.nika.yaml\n",
            "    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n",
            "    plafond: 1.0\n",
            "    manqué: rattraper-une-fois\n",
            "    actif: false\n",
            "    raison: \"en sommeil\"\n",
            "    jusqu_au: \"2099-12-31\"\n",
        );
        let dir = project_at("proof", body);
        let sidecar = state::ArmState::at_project(dir.path());
        let registry = parse_registry(body).expect("cadence registry");
        let generation = nika_cadence::ArmGeneration::compute(
            registry.beats().next().expect("first beat"),
            b"nika: prouve\n",
        );
        let generation_short = generation.short().to_owned();
        // prouve: the machine fired twice, skipped once — the sidecar
        // attests (last.json + the history's tallies).
        let mut fired = state::HistoryEntry::new(
            Some("2026-08-18T07:07:00Z".parse().expect("ts")),
            "2026-08-18T07:07:04Z".parse().expect("ts"),
            state::FireKind::Fired,
        );
        fired.trace = Some(".nika/traces/2026-08-18T07-07-04Z_cafe.ndjson".to_owned());
        fired.exit = Some(0);
        fired.generation = Some(generation);
        sidecar.record_fixture("prouve", &fired).expect("record");
        sidecar.record_fixture("prouve", &fired).expect("record");
        let mut skipped = fired.clone();
        skipped.kind = state::FireKind::Skipped;
        skipped.reason = Some("missed:1".to_owned());
        skipped.trace = None;
        skipped.generation = None;
        sidecar.record_fixture("prouve", &skipped).expect("record");
        // ghost: a sidecar the registry no longer names (N4).
        sidecar.record_fixture("ghost", &fired).expect("record");
        // The order is the LAST decision's: re-fire so last.json says fired.
        sidecar.record_fixture("prouve", &fired).expect("record");

        let out = run_at(dir.path());
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("✓ PROUVÉ"), "{}", out.text);
        assert!(
            out.text.contains(&format!("gen {generation_short}")),
            "the report shows the pinned generation: {}",
            out.text
        );
        assert!(
            out.text.contains("état succeeded"),
            "the report shows the folded machine state: {}",
            out.text
        );
        assert!(out.text.contains("DÉCLARÉ"), "{}", out.text);
        assert!(
            out.text.contains("· ghost — .nika/arm/ghost/"),
            "{}",
            out.text
        );
        // The tmpdir path is the only non-deterministic byte (insta's
        // `filters` feature is not in the workspace set — a replace is
        // the same redaction, zero new feature).
        let shown = out
            .text
            .replace(&dir.path().to_string_lossy().into_owned(), "[PROJET]");
        insta::assert_snapshot!(shown);
    }

    #[test]
    fn the_report_refuses_tampered_or_truncated_history_loudly() {
        let body = concat!(
            "nika: v1\n",
            "arm:\n",
            "  - workflow: workflows/doctor.nika.yaml\n",
            "    cadence: \"TZ=UTC 0 3 * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "    actif: false\n",
            "    raison: \"maintenance\"\n",
            "    jusqu_au: \"2099-12-31\"\n",
        );
        for mutation in ["tamper", "truncate"] {
            let dir = project_at(mutation, body);
            let sidecar = state::ArmState::at_project(dir.path());
            let mut entry = state::HistoryEntry::new(
                Some("2026-08-18T03:00:00Z".parse().expect("slot")),
                "2026-08-18T03:01:00Z".parse().expect("decision"),
                state::FireKind::Skipped,
            );
            entry.reason = Some("missed:1".to_owned());
            entry.exit = Some(0);
            sidecar.record_fixture("doctor", &entry).expect("record");
            let history = dir.path().join(".nika/arm/doctor/history.ndjson");
            let text = std::fs::read_to_string(&history).expect("history");
            let corrupt = if mutation == "tamper" {
                text.replacen("\"seq\":1", "\"seq\":9", 1)
            } else {
                text[..text.len() - 12].to_owned()
            };
            std::fs::write(&history, corrupt).expect("corrupt evidence");
            let out = run_at(dir.path());
            assert_eq!(out.code, exit::ENV, "{mutation}: {}", out.text);
            assert!(
                out.text.contains("arm report refused · corrupt sidecar"),
                "{mutation}: {}",
                out.text
            );
            assert!(!out.text.contains("DÉCLARÉ"), "{mutation}: {}", out.text);
        }
    }

    /// D5: a durable claim without receipt is classified by the pure
    /// machine. Once its injected deadline is in the past, the report
    /// says `ambiguous` and names the current slot identity.
    #[test]
    fn the_report_surfaces_the_folded_ambiguous_claim() {
        let body = concat!(
            "nika: v1\n",
            "arm:\n",
            "  - workflow: workflows/doctor.nika.yaml\n",
            "    cadence: \"TZ=UTC 0 3 * * *\"\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "    actif: false\n",
            "    raison: \"test\"\n",
            "    jusqu_au: \"2099-12-31\"\n",
        );
        let dir = project_at("ambiguous", body);
        let slot = "2026-08-19T03:00:00Z"
            .parse::<jiff::Timestamp>()
            .expect("slot")
            .to_zoned(jiff::tz::TimeZone::UTC);
        let slot_id =
            nika_cadence::SlotId::derive("workflows/doctor.nika.yaml", "TZ=UTC 0 3 * * *", &slot);
        let short = slot_id.short().to_owned();
        let claim = state::Claim::new(
            slot_id,
            "2026-08-19T04:00:00Z".parse().expect("deadline"),
            "2026-08-19T03:02:00Z".parse().expect("claimed"),
        );
        state::ArmState::at_project(dir.path())
            .record_claim_fixture("doctor", &claim)
            .expect("claim");

        let out = run_at(dir.path());
        assert_eq!(out.code, exit::OK, "{}", out.text);
        assert!(out.text.contains("état ambiguous"), "{}", out.text);
        assert!(
            out.text.contains(&format!("slot courant {short}")),
            "{}",
            out.text
        );
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

    /// ⭐ THE DIVERGENCE, CLOSED — measured 2026-08-15, fixed 2026-08-19.
    ///
    /// `nika-cadence::Beat` defines THIRTEEN keys; `nika_vocab::ArmEntry`
    /// used to accept FIVE, and `nika_vocab` runs FIRST (it is what
    /// `discover` parses), so the other eight were UNREACHABLE through
    /// the project file: the cadence grammar defined them, validated
    /// them, and no author could write one (measured again 2026-08-18:
    /// a file carrying `chevauchement:` cost `nika arm` exit 2 and
    /// `nika run` exit 3, `project.unknown-key`).
    ///
    /// The close keeps the two parsers on their own planes (law 8 —
    /// deux parseurs, jamais en désaccord): vocab judges the SHAPE and
    /// carries the cadence arc's eight keys VERBATIM (`actif` alone is
    /// shape-judged, a bool); the VALUES' law stays cadence's. A
    /// `chevauchement: nimporte` passes vocab and is refused by
    /// cadence — the agreement is on the key SET, never on the
    /// semantics.
    ///
    /// Among the newly reachable: `actif:` — the disarm switch that law
    /// N4 says is the ONLY way to disarm (« removing a line does NOT
    /// disarm ») — and `chevauchement:`/`après_saut:`, the whole of law
    /// ⑥. The overlap policy this verb reports is a policy an author
    /// can now set.
    ///
    /// This test pins the fixed state: the thirteen keys written
    /// together pass BOTH readers — the report is green, one beat, and
    /// (suspended as it is) it is REPORTED, never computed.
    #[test]
    fn every_cadence_key_is_reachable_through_the_project_file() {
        let body = concat!(
            "nika: v1\n",
            "arm:\n",
            "  - workflow: w.nika.yaml\n",
            "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
            "    où: local\n",
            "    plafond: 0.25\n",
            "    manqué: sauter\n",
            "    chevauchement: sauter\n",
            "    après_saut: prochain-créneau\n",
            "    actif: false\n",
            "    raison: \"pause estivale\"\n",
            "    jusqu_au: \"2026-12-31\"\n",
            "    tolérance: \"3/4\"\n",
            "    décalage: hash\n",
            "    par: \"thibaut\"\n",
        );
        // The cadence grammar knows all thirteen …
        assert!(
            nika_cadence::parse::parse_registry(body).is_ok(),
            "cadence accepts its own closed set"
        );
        // … and so does the project shape, FIRST — the whole file is
        // lawful to both readers, and the verb's report is green.
        let dir = project_at("closed", body);
        let out = run_at(dir.path());
        assert_eq!(
            out.code,
            exit::OK,
            "the thirteen keys pass both readers: {}",
            out.text
        );
        assert!(out.text.contains("1 beat"), "{}", out.text);
        assert!(
            out.text.contains("[idle ]"),
            "actif: false is REPORTED, never computed: {}",
            out.text
        );
    }
}
