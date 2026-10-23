// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::unreachable
)]

//! The W3 pins of [`crate::emit`] — « LE PONT », the pure OS-unit renderer.
//!
//! - insta snapshots of the four combinations (launchd/systemd ×
//!   per-beat/serve) plus the env-file halves (D7: the secret never
//!   lands in a unit — the env FILE rides by path);
//! - the four refusal classes (D10 tz · webhook · the D6 v0 set · the
//!   interval budget), each with its exact fields;
//! - `deux_beats_meme_workflow_rendent_deux_labels` — the D4 identity;
//! - the property: any cadence `Cadence::parse` accepts renders at most
//!   [`emit::MAX_INTERVALS`] dicts, or refuses naming the count.

use std::path::PathBuf;

use proptest::prelude::*;

use crate::emit::{self, EmitCtx, EmitRefusal, Mode, Target, Unit};
use crate::{ArmRegistry, Cadence, parse_registry, validate};

/// A lawful registry — the law pass runs BEFORE the renderer (the door).
fn registry(text: &str) -> ArmRegistry {
    let reg = parse_registry(text).expect("le registre du test se parse");
    assert_eq!(validate(&reg).count(), 0, "le registre du test est sain");
    reg
}

/// The project key the fixtures carry, as the L4 edge derives it from the
/// canonical root's fingerprint ([`emit::project_key`]).
const KEY: &str = "0a1b2c3d";

/// The machine facts, literal — the renderer is pure, nothing is read.
fn ctx() -> EmitCtx {
    EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/projet"),
        PathBuf::from("/projet/nika.yaml"),
        KEY.to_owned(),
        None,
        PathBuf::from("/projet/.nika/arm/logs"),
        "Europe/Paris".to_owned(),
    )
}

/// The same facts with an env file (D7 — the unit names its PATH, and
/// the VALUES never cross).
fn ctx_with_env() -> EmitCtx {
    EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/projet"),
        PathBuf::from("/projet/nika.yaml"),
        KEY.to_owned(),
        Some(PathBuf::from("/projet/.env")),
        PathBuf::from("/projet/.nika/arm/logs"),
        "Europe/Paris".to_owned(),
    )
}

/// Two Paris beats: the readable weekly (one dict) and a cron monthly
/// with two minutes (two dicts).
const TWO_BEATS: &str = concat!(
    "nika: proj\n",
    "arm:\n",
    "  - workflow: workflows/weekly.nika\n",
    "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
    "    plafond: 0.25\n",
    "    manqué: rattraper-une-fois\n",
    "  - workflow: workflows/nightly.nika\n",
    "    cadence: \"TZ=Europe/Paris 0,30 3 1 * *\"\n",
    "    plafond: 1.0\n",
    "    manqué: sauter\n",
);

/// The print shape the L4 verb reuses: separator + body.
fn joined(units: &[Unit]) -> String {
    units
        .iter()
        .map(|u| format!("# ── {}\n{}", u.file_name, u.body))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Snapshot into `src/snapshots/` (one level up from this test file).
fn snap(name: &str, text: &str) {
    insta::with_settings!({snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!(name, text);
    });
}

/// D2, pinned on every unit of a render: the ONE firer is
/// `arm fire <label>` — no unit ever invokes `run`.
fn assert_never_runs(units: &[Unit]) {
    for unit in units {
        assert!(
            !unit.body.contains("<string>run</string>"),
            "D2 — jamais run: {}",
            unit.file_name
        );
        for line in unit.body.lines().filter(|l| l.starts_with("ExecStart=")) {
            assert!(!line.contains(" run"), "D2 — jamais run: {line}");
            assert!(line.contains(" arm "), "l'unité parle arm: {line}");
        }
    }
}

// ── the four combinations, snapshotted ─────────────────────────────

#[test]
fn launchd_per_beat() {
    let reg = registry(TWO_BEATS);
    let units = emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect("rendu");
    assert_eq!(units.len(), 2, "une unité par beat local actif");
    assert_eq!(units[0].file_name, "nika.arm.0a1b2c3d.weekly.plist");
    assert_eq!(units[1].file_name, "nika.arm.0a1b2c3d.nightly.plist");
    assert!(units[0].body.contains(emit::GENERATED_MARK), "la marque");
    assert!(
        units[0]
            .body
            .contains("<key>Label</key>\n\t<string>nika.arm.0a1b2c3d.weekly</string>"),
        "{}",
        units[0].body
    );
    assert_never_runs(&units);
    assert!(
        units[0].body.contains("<string>fire</string>"),
        "le tireur unique"
    );
    snap("launchd_per_beat", &joined(&units));
}

#[test]
fn launchd_per_beat_with_env_file() {
    let reg = registry(TWO_BEATS);
    let units = emit::render(&reg, &ctx_with_env(), Target::Launchd, Mode::PerBeat).expect("rendu");
    // D7: the env file rides by PATH, handed to the firer — never a value,
    // never a shell (NIK-198: launchd starts the unit with none).
    assert!(
        units[0]
            .body
            .contains("<string>--env-file</string>\n\t\t<string>/projet/.env</string>"),
        "le firer reçoit le fichier: {}",
        units[0].body
    );
    assert!(!units[0].body.contains("/bin/sh"), "{}", units[0].body);
    snap("launchd_per_beat_env", &joined(&units));
}

/// The serve unit's verb takes no `--env-file`: it keeps the shell wrap
/// that exports the file, and a wrapped unit still names its env file (an
/// earlier engine wrapped every beat so; the L4 edge recovers it).
#[test]
fn the_serve_unit_keeps_the_wrap_and_a_wrap_names_its_env_file() {
    let reg = registry(TWO_BEATS);
    let serve = emit::render(&reg, &ctx_with_env(), Target::Launchd, Mode::Serve).expect("serve");
    assert!(
        serve[0].body.contains("<string>/bin/sh</string>")
            && serve[0]
                .body
                .contains("set -a &amp;&amp; . &apos;/projet/.env&apos;"),
        "{}",
        serve[0].body
    );
    assert!(!serve[0].body.contains("--env-file"), "{}", serve[0].body);
    assert_eq!(
        emit::env_file_named_in_unit(&serve[0].body).as_deref(),
        Some("/projet/.env")
    );
}

/// #1069 — a unit that loads an env file names it; a short-argv unit does
/// not. Replacing the first with the second is the strip the L4 edge
/// must refuse.
#[test]
fn a_wrapped_unit_names_its_env_file_and_a_bare_unit_does_not() {
    let reg = registry(TWO_BEATS);
    let wrapped =
        emit::render(&reg, &ctx_with_env(), Target::Launchd, Mode::PerBeat).expect("wrap");
    assert_eq!(
        emit::env_file_named_in_unit(&wrapped[0].body).as_deref(),
        Some("/projet/.env")
    );
    assert!(
        wrapped[0].body.contains("<string>--env-file</string>"),
        "le firer reçoit `--env-file`: {}",
        wrapped[0].body
    );
    let bare = emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect("bare");
    assert_eq!(
        emit::env_file_named_in_unit(&bare[0].body),
        None,
        "le short argv ne source rien: {}",
        bare[0].body
    );
    assert!(
        !bare[0].body.contains("/bin/sh"),
        "sans env file, pas de wrap: {}",
        bare[0].body
    );

    let systemd =
        emit::render(&reg, &ctx_with_env(), Target::SystemdUser, Mode::PerBeat).expect("systemd");
    assert_eq!(
        emit::env_file_named_in_unit(&systemd[1].body).as_deref(),
        Some("/projet/.env"),
        "EnvironmentFile= on the service unit: {}",
        systemd[1].body
    );
}

#[test]
fn env_file_named_in_unit_does_not_need_the_generated_mark() {
    let reg = registry(TWO_BEATS);
    let units = emit::render(&reg, &ctx_with_env(), Target::Launchd, Mode::PerBeat).expect("wrap");
    let stripped = units[0].body.replace(emit::GENERATED_MARK, "hand-kept");
    assert!(
        !stripped.contains(emit::GENERATED_MARK),
        "la marque est partie"
    );
    assert_eq!(
        emit::env_file_named_in_unit(&stripped).as_deref(),
        Some("/projet/.env"),
        "le wrap se lit sans GENERATED: {stripped}"
    );
}

#[test]
fn env_file_named_in_unit_survives_an_apostrophe_in_the_path() {
    let reg = registry(TWO_BEATS);
    let ctx = EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/projet"),
        PathBuf::from("/projet/nika.yaml"),
        KEY.to_owned(),
        Some(PathBuf::from("/projet/o's.env")),
        PathBuf::from("/projet/.nika/arm/logs"),
        "Europe/Paris".to_owned(),
    );
    let units = emit::render(&reg, &ctx, Target::Launchd, Mode::PerBeat).expect("rendu");
    assert_eq!(
        emit::env_file_named_in_unit(&units[0].body).as_deref(),
        Some("/projet/o's.env"),
        "{}",
        units[0].body
    );
}

#[test]
fn launchd_serve() {
    let reg = registry(TWO_BEATS);
    let units = emit::render(&reg, &ctx(), Target::Launchd, Mode::Serve).expect("rendu");
    assert_eq!(units.len(), 1, "une seule unité serve");
    assert_eq!(units[0].file_name, "nika.serve.0a1b2c3d.plist");
    assert!(units[0].body.contains("KeepAlive"), "le démon veillé");
    snap("launchd_serve", &joined(&units));
}

#[test]
fn systemd_per_beat() {
    let reg = registry(TWO_BEATS);
    let units = emit::render(&reg, &ctx(), Target::SystemdUser, Mode::PerBeat).expect("rendu");
    // timer + service per beat.
    assert_eq!(units.len(), 4, "le couple timer/service par beat");
    assert_eq!(units[0].file_name, "nika.arm.0a1b2c3d.weekly.timer");
    assert_eq!(units[1].file_name, "nika.arm.0a1b2c3d.weekly.service");
    assert!(
        units[0]
            .body
            .contains("OnCalendar=Mon *-*-* 09:07:00 Europe/Paris"),
        "le fuseau voyage dans OnCalendar: {}",
        units[0].body
    );
    // manqué: rattraper-une-fois ⇒ Persistent=true · sauter ⇒ absent.
    assert!(
        units[0].body.contains("Persistent=true"),
        "{}",
        units[0].body
    );
    assert!(
        !units[2].body.contains("Persistent=true"),
        "{}",
        units[2].body
    );
    assert_never_runs(&units);
    assert!(
        units[1]
            .body
            .contains("ExecStart=/usr/local/bin/nika arm fire weekly"),
        "{}",
        units[1].body
    );
    snap("systemd_per_beat", &joined(&units));
}

#[test]
fn systemd_per_beat_with_env_file() {
    let reg = registry(TWO_BEATS);
    let units =
        emit::render(&reg, &ctx_with_env(), Target::SystemdUser, Mode::PerBeat).expect("rendu");
    assert!(
        units[1].body.contains("EnvironmentFile=/projet/.env"),
        "D7 — le chemin, jamais la valeur: {}",
        units[1].body
    );
    snap("systemd_per_beat_env", &joined(&units));
}

#[test]
fn systemd_serve() {
    let reg = registry(TWO_BEATS);
    let units = emit::render(&reg, &ctx(), Target::SystemdUser, Mode::Serve).expect("rendu");
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].file_name, "nika.serve.0a1b2c3d.service");
    assert!(units[0].body.contains("Type=simple"));
    assert!(units[0].body.contains("Restart=on-failure"));
    assert!(
        units[0]
            .body
            .contains("ExecStart=/usr/local/bin/nika serve")
    );
    snap("systemd_serve", &joined(&units));
}

// ── the refusal classes, exact fields ──────────────────────────────

/// D10: a beat in another zone wakes, on launchd, at each machine hour its slots fall on over the
/// year after the render: Tokyo's 03:00 is 19:00 or 20:00 in Paris, as Paris keeps winter or
/// summer time, every day (the firer reads the day in Tokyo), and nothing else.
#[test]
fn a_beat_in_another_zone_wakes_at_the_machine_hours_its_slots_fall_on() {
    let reg = registry(concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika\n",
        "    cadence: \"TZ=Asia/Tokyo 0 3 * * 1-5\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    let now: jiff::Timestamp = "2026-10-16T12:00:00Z".parse().expect("now");
    let units = emit::render(&reg, &ctx().at(now), Target::Launchd, Mode::PerBeat)
        .expect("a foreign zone wakes at the machine's hours");
    let body = &units[0].body;
    let dict = |hour: u8| {
        format!(
            "\t\t<dict>\n\t\t\t<key>Hour</key>\n\t\t\t<integer>{hour}</integer>\n\t\t\t<key>Minute</key>\n\t\t\t<integer>0</integer>\n\t\t</dict>\n"
        )
    };
    assert!(
        body.contains(&dict(19)) && body.contains(&dict(20)),
        "{body}"
    );
    assert_eq!(
        body.matches("<dict>").count(),
        3,
        "the plist and two wakes: {body}"
    );
    assert!(
        !body.contains("Weekday") && !body.contains("<key>Day</key>"),
        "{body}"
    );
    assert!(
        body.contains("<string>arm</string>\n\t\t<string>fire</string>"),
        "{body}"
    );
    // Mauritius keeps no summer time, Paris does: 08:00 Paris is 10:00 or 11:00 there.
    let paris = registry(concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/daily.nika\n",
        "    cadence: \"TZ=Europe/Paris 0 8 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    let mut mauritius = ctx().at(now);
    mauritius.machine_tz = "Indian/Mauritius".to_owned();
    let units = emit::render(&paris, &mauritius, Target::Launchd, Mode::PerBeat).expect("paris");
    assert!(
        units[0].body.contains(&dict(10)) && units[0].body.contains(&dict(11)),
        "{}",
        units[0].body
    );
}

#[test]
fn a_beat_in_another_zone_refuses_on_launchd_and_rides_systemd() {
    // D10: launchd fires in the MACHINE's zone — rendered without the
    // edge's clock, a beat whose TZ= differs is refused, naming both
    // zones. systemd carries the zone, so the same beat renders there.
    let reg = registry(concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika\n",
        "    cadence: \"TZ=Asia/Tokyo 0 3 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    let refusal =
        emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect_err("D10 refuse");
    assert_eq!(
        refusal,
        EmitRefusal::TzMismatch {
            beat: "doctor".to_owned(),
            cadence_tz: "Asia/Tokyo".to_owned(),
            machine_tz: "Europe/Paris".to_owned(),
        }
    );
    let units = emit::render(&reg, &ctx(), Target::SystemdUser, Mode::PerBeat)
        .expect("systemd porte le fuseau");
    assert!(units[0].body.contains("Asia/Tokyo"), "{}", units[0].body);
}

#[test]
fn a_webhook_beat_refuses_the_clock_surfaces() {
    let reg = registry(concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/hook.nika\n",
        "    cadence: \"on-webhook\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    for target in [Target::Launchd, Target::SystemdUser] {
        let refusal = emit::render(&reg, &ctx(), target, Mode::PerBeat).expect_err("webhook");
        assert_eq!(
            refusal,
            EmitRefusal::Webhook {
                beat: "hook".to_owned()
            }
        );
    }
}

#[test]
fn past_the_interval_budget_the_render_refuses_with_the_count() {
    // 30 minutes × 12 heures × 2 jours = 720 dicts — au-delà du budget.
    let reg = registry(concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/dense.nika\n",
        "    cadence: \"TZ=Europe/Paris */2 */2 1,15 * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    let refusal =
        emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect_err("720 > 500");
    assert_eq!(
        refusal,
        EmitRefusal::TooManyIntervals {
            beat: "dense".to_owned(),
            n: 720,
        }
    );
    // systemd inlines the sets — no dict explosion, no refusal.
    emit::render(&reg, &ctx(), Target::SystemdUser, Mode::PerBeat).expect("systemd rend dense");
}

#[test]
fn the_v0_unsupported_policies_refuse_with_their_version() {
    for (extra, what) in [
        ("    chevauchement: remplacer\n", "chevauchement: remplacer"),
        ("    après_saut: à-complétion\n", "après_saut: à-complétion"),
        ("    décalage: hash\n", "décalage:"),
    ] {
        let reg = registry(&format!(
            "nika: proj\narm:\n  - workflow: workflows/doctor.nika\n    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n    plafond: 0.25\n    manqué: sauter\n{extra}"
        ));
        let refusal =
            emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect_err("D6 refuse");
        match refusal {
            EmitRefusal::UnsupportedInV0 {
                beat,
                what: got,
                arrives,
            } => {
                assert_eq!(beat, "doctor");
                assert_eq!(got, what);
                assert_eq!(arrives, "serve v0.2", "la version est nommée");
            }
            other => panic!("D6 attendu, reçu {other:?}"),
        }
    }
    let reg = registry(concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika\n",
        "    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: rattraper\n",
    ));
    let refusal =
        emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect_err("D6 refuse");
    assert!(
        matches!(
            refusal,
            EmitRefusal::UnsupportedInV0 { ref what, .. } if what == "manqué: rattraper"
        ),
        "{refusal:?}"
    );
}

// ── the D4 identity ────────────────────────────────────────────────

#[test]
fn deux_beats_meme_workflow_rendent_deux_labels() {
    // Two DIFFERENT paths, one radical: the second takes `-2` (D4), and
    // each gets its own unit file.
    let reg = registry(concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: a/doctor.nika\n",
        "    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
        "  - workflow: b/doctor.nika\n",
        "    cadence: \"TZ=Europe/Paris 0 4 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    assert_eq!(emit::labels(&reg), ["doctor", "doctor-2"], "l'identité D4");
    let units = emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect("rendu");
    assert_eq!(units[0].file_name, "nika.arm.0a1b2c3d.doctor.plist");
    assert_eq!(units[1].file_name, "nika.arm.0a1b2c3d.doctor-2.plist");
    assert!(
        units[1].body.contains("<string>doctor-2</string>"),
        "le label D4 dans l'appel"
    );
}

// ── a unit carries its project ─────────────────────────────────────

/// Two projects arming a beat of the same name. Without the key both
/// rendered `nika.arm.<label>`: the second `--write` replaced the first
/// project's unit, and the first project's disarm deleted the second's.
#[test]
fn two_projects_arming_the_same_beats_never_share_a_unit() {
    let reg = registry(TWO_BEATS);
    let other = EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/autre"),
        PathBuf::from("/autre/nika.yaml"),
        "9f8e7d6c".to_owned(),
        None,
        PathBuf::from("/autre/.nika/arm/logs"),
        "Europe/Paris".to_owned(),
    );
    for target in [Target::Launchd, Target::SystemdUser] {
        for mode in [Mode::PerBeat, Mode::Serve] {
            let ours = emit::render(&reg, &ctx(), target, mode).expect("rendu");
            let theirs = emit::render(&reg, &other, target, mode).expect("rendu");
            assert_eq!(ours.len(), theirs.len(), "the same units");
            for (a, b) in ours.iter().zip(&theirs) {
                assert_ne!(a.file_name, b.file_name, "one unit per project");
                assert_eq!(emit::generated_source(&a.body), Some("/projet/nika.yaml"));
                assert_eq!(emit::generated_source(&b.body), Some("/autre/nika.yaml"));
            }
        }
    }
}

#[test]
fn the_unit_names_spell_the_project_key_then_the_beat_label() {
    let fingerprint = "0a1b2c3d4e5f60718293a4b5c6d7e8f9";
    assert_eq!(emit::project_key(fingerprint), KEY);
    assert_eq!(emit::beat_unit(KEY, "daily"), "nika.arm.0a1b2c3d.daily");
    assert_eq!(emit::legacy_beat_unit("daily"), "nika.arm.daily");
    assert_eq!(emit::serve_unit(KEY), "nika.serve.0a1b2c3d");
    assert_eq!(
        emit::beat_unit_files(Target::Launchd, "nika.arm.0a1b2c3d.daily"),
        ["nika.arm.0a1b2c3d.daily.plist"]
    );
    assert_eq!(
        emit::beat_unit_files(Target::SystemdUser, "nika.arm.0a1b2c3d.daily"),
        [
            "nika.arm.0a1b2c3d.daily.timer",
            "nika.arm.0a1b2c3d.daily.service"
        ]
    );
}

/// The L4 edge finds a beat's units by the helpers alone (disarm renders
/// nothing): they spell exactly the files the render writes.
#[test]
fn the_naming_helpers_spell_the_rendered_files() {
    let reg = registry(TWO_BEATS);
    for target in [Target::Launchd, Target::SystemdUser] {
        let units = emit::render(&reg, &ctx(), target, Mode::PerBeat).expect("rendu");
        let spelled: Vec<String> = emit::labels(&reg)
            .iter()
            .flat_map(|label| emit::beat_unit_files(target, &emit::beat_unit(KEY, label)))
            .collect();
        let rendered: Vec<&str> = units.iter().map(|u| u.file_name.as_str()).collect();
        assert_eq!(rendered, spelled);
    }
}

#[test]
fn generated_source_reads_the_project_back_and_none_without_the_mark() {
    let reg = registry(TWO_BEATS);
    for target in [Target::Launchd, Target::SystemdUser] {
        for mode in [Mode::PerBeat, Mode::Serve] {
            let units = emit::render(&reg, &ctx_with_env(), target, mode).expect("rendu");
            for unit in &units {
                assert_eq!(
                    emit::generated_source(&unit.body),
                    Some("/projet/nika.yaml"),
                    "{}",
                    unit.file_name
                );
                let stripped = unit.body.replace(emit::GENERATED_MARK, "hand-kept from");
                assert_eq!(
                    emit::generated_source(&stripped),
                    None,
                    "{}",
                    unit.file_name
                );
            }
        }
    }
    // A project path that holds the separator itself still reads back whole.
    let ctx = EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/a · b"),
        PathBuf::from("/a · b/nika.yaml"),
        KEY.to_owned(),
        None,
        PathBuf::from("/a · b/.nika/arm/logs"),
        "Europe/Paris".to_owned(),
    );
    let units = emit::render(&reg, &ctx, Target::SystemdUser, Mode::PerBeat).expect("rendu");
    assert_eq!(
        emit::generated_source(&units[0].body),
        Some("/a · b/nika.yaml")
    );
}

/// A suspended beat keeps its unit: the firer journals it « inactive » until
/// its `jusqu_au`, then wakes it, so nothing has to re-arm it. A cloud beat's
/// calendar stays the operator's: no unit.
#[test]
fn a_suspended_beat_keeps_its_unit_and_a_cloud_beat_has_none() {
    let reg = registry(concat!(
        "nika: proj\n",
        "arm:\n",
        "  - workflow: workflows/local.nika\n",
        "    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
        "  - workflow: workflows/sommeil.nika\n",
        "    cadence: \"TZ=Europe/Paris 0 4 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
        "    actif: false\n",
        "    raison: \"pause\"\n",
        "    jusqu_au: \"2099-12-31\"\n",
        "  - workflow: workflows/nuage.nika\n",
        "    cadence: \"TZ=Europe/Paris 0 5 * * *\"\n",
        "    où: cloud\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    let units = emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect("rendu");
    let names: Vec<&str> = units.iter().map(|u| u.file_name.as_str()).collect();
    assert_eq!(
        names,
        [
            "nika.arm.0a1b2c3d.local.plist",
            "nika.arm.0a1b2c3d.sommeil.plist"
        ],
        "every local beat, suspended or not, has its unit; the cloud beat none"
    );
}

// ── the budget property ────────────────────────────────────────────

/// How many launchd dicts a parsed cadence SHOULD produce: the product
/// of the restricted fields' sizes (a full field contributes a factor
/// of 1 — a wildcard is omitted, and the all-wildcard beat is one
/// empty dict).
fn expected_dicts(cadence: &Cadence) -> usize {
    fn restricted<const LO: u8, const HI: u8>(f: crate::cron::Field<LO, HI>) -> usize {
        if f.is_full() {
            1
        } else {
            usize::try_from(f.len()).unwrap_or(0)
        }
    }
    let spec = match cadence {
        Cadence::Cron { spec, .. } => *spec,
        // The anchored interval wakes weekly: one dict.
        Cadence::Every { .. } => return 1,
        _ => return 0,
    };
    restricted(*spec.minutes())
        * restricted(*spec.hours())
        * restricted(*spec.dom())
        * restricted(*spec.months())
        * restricted(*spec.dow())
}

/// Render one cadence (UTC both sides) and pin the budget law.
fn check_budget(cadence_text: &str) {
    let cadence = Cadence::parse(cadence_text).expect("le corpus se parse");
    let expected = expected_dicts(&cadence);
    let reg = registry(&format!(
        "nika: proj\narm:\n  - workflow: w.nika\n    cadence: \"{cadence_text}\"\n    plafond: 0.25\n    manqué: sauter\n"
    ));
    let ctx = EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/projet"),
        PathBuf::from("/projet/nika.yaml"),
        KEY.to_owned(),
        None,
        PathBuf::from("/projet/.nika/arm/logs"),
        "UTC".to_owned(),
    );
    match emit::render(&reg, &ctx, Target::Launchd, Mode::PerBeat) {
        Ok(units) => {
            assert!(
                expected <= emit::MAX_INTERVALS,
                "{cadence_text} · {expected} dicts auraient dû refuser"
            );
            assert_eq!(units.len(), 1);
            let dicts = units[0].body.matches("<dict").count() - 1; // moins le dict racine
            assert_eq!(
                dicts, expected,
                "{cadence_text} · le produit cartésien exact"
            );
        }
        Err(EmitRefusal::TooManyIntervals { n, .. }) => {
            assert!(
                n > emit::MAX_INTERVALS,
                "{cadence_text} · {n} sous le budget"
            );
            assert_eq!(
                n, expected,
                "{cadence_text} · le refus nomme le vrai compte"
            );
        }
        Err(other) => panic!("{cadence_text} · refus inattendu {other:?}"),
    }
}

/// The deterministic corpus — every form the grammar accepts, both
/// sides of the budget boundary.
const CORPUS: &[&str] = &[
    "TZ=UTC * * * * *",
    "TZ=UTC 0 3 * * *",
    "TZ=UTC lundi 9h07",
    "TZ=UTC dimanche 0h00",
    "TZ=UTC */15 9-17 * * 1-5",
    "TZ=UTC 0,30 3 1 * *",
    "TZ=UTC 0 9 29 2 *",
    "TZ=UTC 0 9 * jan mon",
    "TZ=UTC */1 * * * *",
    "TZ=UTC * */1 * * *",
    "TZ=UTC 0 0 1 1 *",
    "TZ=UTC 7 9 * * 1,3,5",
    "TZ=UTC 0/10 8 * * *",
    "TZ=UTC 0 9 1-15 * *",
    // The month end wakes on its superset (28 to 31), the interval weekly.
    "TZ=UTC 0 9 L * *",
    "TZ=UTC 0,30 9 L 1,7 *",
    "TZ=UTC every 2 weeks from 2026-10-05 09:00",
    // The boundary: 30×12 = 360 rends, 30×12×2 = 720 refuses.
    "TZ=UTC */2 */2 * * *",
    "TZ=UTC */2 */2 1,15 * *",
    "TZ=UTC */2 */2 */2 * *",
];

#[test]
fn the_corpus_renders_within_budget_or_refuses_the_count() {
    for text in CORPUS {
        check_budget(text);
    }
}

/// A unit's schedule revision names what decides when the OS wakes it: the
/// cadence as written, and on launchd the machine zone it was rendered in.
/// Twelve hex characters, stable for the same schedule; a systemd calendar
/// names its own zone, so the machine zone never moves its revision.
#[test]
fn a_unit_revision_moves_with_the_schedule_it_encodes() {
    let at_three = "TZ=Europe/Paris 0 3 * * *";
    let at_four = "TZ=Europe/Paris 0 4 * * *";
    let rev = emit::unit_revision(at_three, Target::Launchd, "Europe/Paris");
    assert_eq!(rev.len(), emit::UNIT_REV_LEN);
    assert!(rev.bytes().all(|b| b.is_ascii_hexdigit()), "{rev}");
    assert_eq!(
        rev,
        emit::unit_revision(at_three, Target::Launchd, "Europe/Paris")
    );
    assert_ne!(
        rev,
        emit::unit_revision(at_four, Target::Launchd, "Europe/Paris"),
        "an edited cadence"
    );
    assert_ne!(
        rev,
        emit::unit_revision(at_three, Target::Launchd, "Asia/Tokyo"),
        "a launchd unit rendered in another machine zone"
    );
    assert_eq!(
        emit::unit_revision(at_three, Target::SystemdUser, "Europe/Paris"),
        emit::unit_revision(at_three, Target::SystemdUser, "Asia/Tokyo"),
        "systemd's calendar names its own zone"
    );
    assert_ne!(
        rev,
        emit::unit_revision(at_three, Target::SystemdUser, "Europe/Paris"),
        "the two OS forms never share a revision"
    );
}

/// Every rendered beat unit names its revision in its header and hands it to
/// the firer; the header still names its project for the ownership law.
#[test]
fn a_beat_unit_names_its_revision_and_hands_it_to_the_firer() {
    let reg = registry(TWO_BEATS);
    for target in [Target::Launchd, Target::SystemdUser] {
        let units = emit::render(&reg, &ctx(), target, Mode::PerBeat).expect("rendu");
        let expected = emit::unit_revision("TZ=Europe/Paris lundi 9h07", target, "Europe/Paris");
        let weekly = &units[0].body;
        assert_eq!(
            emit::revision_in_unit(weekly),
            Some(expected.as_str()),
            "{target:?}"
        );
        assert!(weekly.contains(&expected), "{weekly}");
        // launchd's one plist, or systemd's service beside the timer.
        assert!(
            units
                .iter()
                .any(|unit| unit.body.contains("--unit-rev") && unit.body.contains(&expected)),
            "{target:?}: the firer is handed the revision"
        );
        assert_eq!(emit::generated_source(weekly), Some("/projet/nika.yaml"));
    }
    assert_eq!(
        emit::revision_in_unit(
            "<!-- nika arm · GENERATED from /p/nika.yaml · beat weekly · do not hand-edit · regenerated by nika arm emit launchd -->"
        ),
        None,
        "a unit written before revisions"
    );
}

proptest! {
    /// Any cadence `Cadence::parse` accepts renders at most
    /// MAX_INTERVALS dicts or refuses naming the count — never more,
    /// never a silent truncation.
    #[test]
    fn any_accepted_cadence_renders_within_budget_or_refuses(
        minute in cron_field(0, 59),
        hour in cron_field(0, 23),
        dom in cron_field(1, 31),
        month in cron_field(1, 12),
        dow in cron_field(0, 6),
    ) {
        let text = format!("TZ=UTC {minute} {hour} {dom} {month} {dow}");
        // Only ACCEPTED cadences owe a render — the grammar's own
        // refusals (Vixie OR · 31 avril · …) are another suite's pins.
        if Cadence::parse(&text).is_ok() {
            check_budget(&text);
        }
    }
}

/// One cron field as text: `*` · `*/n` · a value · a list · a range.
fn cron_field(lo: u8, hi: u8) -> BoxedStrategy<String> {
    prop_oneof![
        Just("*".to_owned()),
        (2u8..=15u8).prop_map(|n| format!("*/{n}")),
        (lo..=hi).prop_map(|v| format!("{v}")),
        prop::collection::vec(lo..=hi, 2..=3).prop_map(|mut vs| {
            vs.sort_unstable();
            vs.dedup();
            vs.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
        }),
        (lo..=hi, lo..=hi).prop_map(|(a, b)| {
            if a <= b {
                format!("{a}-{b}")
            } else {
                format!("{b}-{a}")
            }
        }),
    ]
    .boxed()
}
