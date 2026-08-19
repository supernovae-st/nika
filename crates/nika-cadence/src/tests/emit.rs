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

/// The machine facts, literal — the renderer is pure, nothing is read.
fn ctx() -> EmitCtx {
    EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/projet"),
        PathBuf::from("/projet/nika.yaml"),
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
        Some(PathBuf::from("/projet/.env")),
        PathBuf::from("/projet/.nika/arm/logs"),
        "Europe/Paris".to_owned(),
    )
}

/// Two Paris beats: the readable weekly (one dict) and a cron monthly
/// with two minutes (two dicts).
const TWO_BEATS: &str = concat!(
    "nika: v1\n",
    "arm:\n",
    "  - workflow: workflows/weekly.nika.yaml\n",
    "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
    "    plafond: 0.25\n",
    "    manqué: rattraper-une-fois\n",
    "  - workflow: workflows/nightly.nika.yaml\n",
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
    assert_eq!(units[0].file_name, "nika.arm.weekly.plist");
    assert_eq!(units[1].file_name, "nika.arm.nightly.plist");
    assert!(units[0].body.contains(emit::GENERATED_MARK), "la marque");
    assert!(
        units[0]
            .body
            .contains("<key>Label</key>\n\t<string>nika.arm.weekly</string>"),
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
    // D7: the env file rides by PATH, through a sh wrapper — never a value.
    assert!(
        units[0].body.contains("/bin/sh"),
        "le wrap env: {}",
        units[0].body
    );
    assert!(units[0].body.contains("/projet/.env"), "le chemin, nommé");
    snap("launchd_per_beat_env", &joined(&units));
}

#[test]
fn launchd_serve() {
    let reg = registry(TWO_BEATS);
    let units = emit::render(&reg, &ctx(), Target::Launchd, Mode::Serve).expect("rendu");
    assert_eq!(units.len(), 1, "une seule unité serve");
    assert_eq!(units[0].file_name, "nika.serve.plist");
    assert!(units[0].body.contains("KeepAlive"), "le démon veillé");
    snap("launchd_serve", &joined(&units));
}

#[test]
fn systemd_per_beat() {
    let reg = registry(TWO_BEATS);
    let units = emit::render(&reg, &ctx(), Target::SystemdUser, Mode::PerBeat).expect("rendu");
    // timer + service per beat.
    assert_eq!(units.len(), 4, "le couple timer/service par beat");
    assert_eq!(units[0].file_name, "nika.arm.weekly.timer");
    assert_eq!(units[1].file_name, "nika.arm.weekly.service");
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
    assert_eq!(units[0].file_name, "nika.serve.service");
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

#[test]
fn a_beat_in_another_zone_refuses_on_launchd_and_rides_systemd() {
    // D10: launchd fires in the MACHINE's zone — a beat whose TZ=
    // differs is refused, naming both zones. systemd carries the zone,
    // so the same beat renders there.
    let reg = registry(concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
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
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/hook.nika.yaml\n",
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
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/dense.nika.yaml\n",
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
            "nika: v1\narm:\n  - workflow: workflows/doctor.nika.yaml\n    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n    plafond: 0.25\n    manqué: sauter\n{extra}"
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
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/doctor.nika.yaml\n",
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
        "nika: v1\n",
        "arm:\n",
        "  - workflow: a/doctor.nika.yaml\n",
        "    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
        "  - workflow: b/doctor.nika.yaml\n",
        "    cadence: \"TZ=Europe/Paris 0 4 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    assert_eq!(emit::labels(&reg), ["doctor", "doctor-2"], "l'identité D4");
    let units = emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect("rendu");
    assert_eq!(units[0].file_name, "nika.arm.doctor.plist");
    assert_eq!(units[1].file_name, "nika.arm.doctor-2.plist");
    assert!(
        units[1].body.contains("<string>doctor-2</string>"),
        "le label D4 dans l'appel"
    );
}

#[test]
fn suspended_and_cloud_beats_emit_nothing() {
    let reg = registry(concat!(
        "nika: v1\n",
        "arm:\n",
        "  - workflow: workflows/local.nika.yaml\n",
        "    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
        "  - workflow: workflows/sommeil.nika.yaml\n",
        "    cadence: \"TZ=Europe/Paris 0 4 * * *\"\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
        "    actif: false\n",
        "    raison: \"pause\"\n",
        "    jusqu_au: \"2099-12-31\"\n",
        "  - workflow: workflows/nuage.nika.yaml\n",
        "    cadence: \"TZ=Europe/Paris 0 5 * * *\"\n",
        "    où: cloud\n",
        "    plafond: 0.25\n",
        "    manqué: sauter\n",
    ));
    let units = emit::render(&reg, &ctx(), Target::Launchd, Mode::PerBeat).expect("rendu");
    assert_eq!(units.len(), 1, "seul le beat local actif émet");
    assert_eq!(units[0].file_name, "nika.arm.local.plist");
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
    let Cadence::Cron { spec, .. } = cadence else {
        return 0;
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
        "nika: v1\narm:\n  - workflow: w.nika.yaml\n    cadence: \"{cadence_text}\"\n    plafond: 0.25\n    manqué: sauter\n"
    ));
    let ctx = EmitCtx::new(
        PathBuf::from("/usr/local/bin/nika"),
        PathBuf::from("/projet"),
        PathBuf::from("/projet/nika.yaml"),
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
