// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The D-2026-08-11-N5 contract, pinned: absent = defaults · the file
//! beats the built-in · the closed grammar refuses by name (unknown
//! key · frozen tag · bad value) WITH its line · `arm:` validated but
//! inert · the starter the wizard lays parses to the default.
//!
//! The pins compare lawful money exactly (`0.50` parses to exactly
//! `0.5` — the round-trip is bit-exact by construction), so the
//! float-equality lint is waived HERE, module-locally.
#![allow(clippy::float_cmp)]

use super::{
    ArmLocus, MissPolicy, Project, ProjectErrorKind, ProvenanceFloor, SCHEMA, STARTER, discover,
    parse,
};
use proptest::strategy::Strategy as _;
use std::time::Duration;

/// The ratified example, VERBATIM (D-2026-08-11-N5) — comments and
/// all. If this drifts, the decision and the parser drift together.
const CANONICAL: &str = r#"
nika: v1

ceiling: 0.50          # default max-cost-usd · the per-invocation --max-cost-usd flag ALWAYS wins

arm:                   # the TEAM arming registry (the personal one stays at ~/.nika/arm.yaml)
  - workflow: workflows/compost-beat.nika.yaml
    cadence: "dimanche 18h07"
    où:      local     # local | cloud
    plafond: 2.00      # overrides ceiling for this beat
    manqué:  rattraper-une-fois

traces:
  keep: 30d            # retention policy · the 3 env vars win when set

registry:
  floor: provenanced   # refuse artifacts below this provenance tier
"#;

fn fresh_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nika-project-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// The canonical form parses, every key landing its semantics.
#[test]
fn the_canonical_example_parses_verbatim() {
    let p = parse(CANONICAL).expect("the ratified example parses");
    assert_eq!(p.ceiling, Some(0.50));
    assert_eq!(
        p.traces.map(|t| t.keep),
        Some(Duration::from_secs(30 * 86_400)),
        "keep: 30d is the 30-day age cap"
    );
    assert_eq!(
        p.registry.map(|r| r.floor),
        Some(ProvenanceFloor::Provenanced)
    );
    let arm = p.arm();
    assert_eq!(arm.len(), 1, "one beat armed: {arm:?}");
    let beat = &arm[0];
    assert_eq!(beat.workflow, "workflows/compost-beat.nika.yaml");
    assert_eq!(beat.cadence, "dimanche 18h07");
    assert_eq!(beat.ou, Some(ArmLocus::Local));
    assert_eq!(beat.plafond, 2.00);
    assert_eq!(beat.manque, MissPolicy::RattraperUneFois);
}

/// Absent = defaults, zero ceremony: the bare tag, an empty file, and
/// a comments-only file all parse to the SAME default — and the
/// wizard's starter is one of them (the example cannot drift).
#[test]
fn absence_is_the_default() {
    let default = Project::default();
    assert_eq!(parse("nika: v1\n").expect("bare tag"), default);
    assert_eq!(parse("").expect("empty"), default);
    assert_eq!(parse("   \n  \n").expect("whitespace"), default);
    assert_eq!(
        parse("# only a comment\n# and another\n").expect("comments-only"),
        default,
        "a file that declares nothing governs nothing"
    );
    assert_eq!(
        parse(STARTER).expect("the wizard's starter parses"),
        default,
        "the starter is all-commented examples — absence IS the defaults"
    );
}

/// An unknown key anywhere is a NAMED error WITH ITS LINE — never a
/// silent drop. Pinned at every level of the grammar, line exact.
#[test]
fn unknown_keys_refuse_by_name_with_their_line() {
    // Top level — the typo'd `celing` sits on line 3.
    let err = parse("nika: v1\nceiling: 0.50\nceling: 0.25\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::UnknownKey);
    assert_eq!(err.line(), Some(3), "the key's own line: {err}");
    assert!(err.detail().contains("celing"), "the key is named: {err}");
    assert!(err.remedy().contains("ceiling"), "the set is taught: {err}");

    // `traces:` level.
    let err = parse("nika: v1\ntraces:\n  keep: 30d\n  budget: 1\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::UnknownKey);
    assert_eq!(err.line(), Some(4));

    // `registry:` level.
    let err = parse("nika: v1\nregistry:\n  flor: provenanced\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::UnknownKey);
    assert_eq!(err.line(), Some(3));

    // The `arm:` entry level is pinned by
    // `a_fourteenth_key_still_refuses_by_name` — since 2026-08-19 the
    // cadence arc's full thirteen-key vocabulary IS this file's.
}

/// Malformed YAML refuses with the grammar's name AND a line — the
/// operator is never pointed at nothing.
#[test]
fn malformed_yaml_refuses_with_a_line() {
    let err = parse("nika: v1\nceiling: [0.50\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::Grammar);
    assert!(err.line().is_some(), "a line rides the refusal: {err}");
    // A duplicate key is YAML-loud too (never last-wins).
    let err = parse("nika: v1\nnika: v1\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::Grammar, "{err}");
    // A bare scalar at the top is not a project file.
    let err = parse("just a string\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::Grammar);
}

/// The tag is frozen: absent or non-`v1` refuses by name.
#[test]
fn the_tag_is_frozen() {
    let err = parse("ceiling: 0.50\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::TagFrozen, "{err}");
    let err = parse("nika: v2\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::TagFrozen, "{err}");
    assert_eq!(
        parse(&format!("nika: {SCHEMA}\n")).expect("v1"),
        Project::default()
    );
}

/// `ceiling:` — a positive finite number, unquoted. The flag-side
/// ladder (flag wins) is pinned at the consuming seam; here the
/// VALUE law.
#[test]
fn ceiling_values_are_lawful_money() {
    assert_eq!(
        parse("nika: v1\nceiling: 0.50\n").expect("ok").ceiling,
        Some(0.5)
    );
    for bad in [
        "ceiling: -1",
        "ceiling: 0",
        "ceiling: \"0.50\"",
        "ceiling: soon",
    ] {
        let err = parse(&format!("nika: v1\n{bad}\n")).unwrap_err();
        assert_eq!(err.kind(), ProjectErrorKind::BadValue, "{bad}: {err}");
    }
}

/// `traces.keep:` — `<N>d`, days-grained, and the block refuses to
/// govern nothing.
#[test]
fn traces_keep_is_days_grained() {
    let p = parse("nika: v1\ntraces:\n  keep: 45d\n").expect("ok");
    assert_eq!(
        p.traces.map(|t| t.keep),
        Some(Duration::from_secs(45 * 86_400))
    );
    for bad in ["keep: 30", "keep: 30h", "keep: soon", "keep: -1d"] {
        let err = parse(&format!("nika: v1\ntraces:\n  {bad}\n")).unwrap_err();
        assert_eq!(err.kind(), ProjectErrorKind::BadValue, "{bad}: {err}");
    }
    let err = parse("nika: v1\ntraces:\n").unwrap_err();
    assert_eq!(
        err.kind(),
        ProjectErrorKind::BadValue,
        "an empty block: {err}"
    );
}

/// `registry.floor:` — the closed ladder, each spelling accepted,
/// anything else refused.
#[test]
fn registry_floor_is_the_closed_ladder() {
    for tier in ProvenanceFloor::ALL {
        let p = parse(&format!("nika: v1\nregistry:\n  floor: {tier}\n")).expect("tier parses");
        assert_eq!(p.registry.map(|r| r.floor), Some(tier));
    }
    let err = parse("nika: v1\nregistry:\n  floor: bogus\n").unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::BadValue, "{err}");
    let err = parse("nika: v1\nregistry:\n").unwrap_err();
    assert_eq!(
        err.kind(),
        ProjectErrorKind::BadValue,
        "an empty block: {err}"
    );
}

/// The spelling round-trip the consuming seam rides: every tier's
/// `as_str` parses back to itself (the drift guard, this side).
#[test]
fn the_floor_spellings_round_trip() {
    for tier in ProvenanceFloor::ALL {
        assert_eq!(ProvenanceFloor::parse(tier.as_str()), Some(tier));
    }
    assert!(
        ProvenanceFloor::parse("Provenanced").is_none(),
        "case-closed"
    );
}

/// `arm:` — validated, INERT: required fields refuse by name, the
/// closed vocabularies hold, and the parsed entries sit on the type
/// for the cadence arc to consume.
#[test]
fn arm_entries_validate_their_shape() {
    let ok = "nika: v1\narm:\n  - workflow: workflows/w.nika.yaml\n    cadence: \"TZ=Europe/Paris 0 9 * * 1\"\n    plafond: 0.35\n    manqué: rattraper\n  - workflow: workflows/v.nika.yaml\n    cadence: on-webhook\n    où: cloud\n    plafond: 2.00\n    manqué: rattraper-une-fois\n";
    let p = parse(ok).expect("two beats parse");
    assert_eq!(p.arm().len(), 2);
    assert_eq!(
        p.arm()[0].ou,
        None,
        "`où:` absent = the safe local default downstream"
    );
    assert_eq!(p.arm()[1].ou, Some(ArmLocus::Cloud));
    assert_eq!(p.arm()[0].manque, MissPolicy::Rattraper);

    // Each REQUIRED field's absence is named.
    let cases = [
        (
            "cadence: \"lundi 9h00\"\n    plafond: 1.0\n    manqué: sauter",
            "workflow",
        ),
        (
            "workflow: w.nika.yaml\n    plafond: 1.0\n    manqué: sauter",
            "cadence",
        ),
        (
            "workflow: w.nika.yaml\n    cadence: \"lundi 9h00\"\n    manqué: sauter",
            "plafond",
        ),
        (
            "workflow: w.nika.yaml\n    cadence: \"lundi 9h00\"\n    plafond: 1.0",
            "manqué",
        ),
    ];
    for (entry, missing_key) in cases {
        let err = parse(&format!("nika: v1\narm:\n  - {entry}\n")).unwrap_err();
        assert_eq!(
            err.kind(),
            ProjectErrorKind::BadValue,
            "{missing_key}: {err}"
        );
        assert!(
            err.detail().contains(missing_key),
            "{missing_key} named: {err}"
        );
    }

    // The shape laws — each broken entry against one full document.
    let base = "nika: v1\narm:\n  - workflow: w.nika.yaml\n    cadence: \"lundi 9h00\"\n    plafond: 1.0\n    manqué: sauter\n";
    let bad_docs = [
        base.replace("workflow: w.nika.yaml", "workflow: w.yaml"), // not *.nika.yaml
        base.replace("workflow: w.nika.yaml", "workflow: ''"),     // empty path
        base.replace("manqué: sauter", "où: moon\n    manqué: sauter"), // unknown locus
        base.replace("plafond: 1.0", "plafond: -1"),               // the pay law
        base.replace("plafond: 1.0", "plafond: soon"),             // not a number
        base.replace("manqué: sauter", "manqué: jamais"),          // outside the closed set
        base.replace("cadence: \"lundi 9h00\"", "cadence: ''"),    // when does it fire?
    ];
    for doc in bad_docs {
        let err = parse(&doc).unwrap_err();
        assert_eq!(err.kind(), ProjectErrorKind::BadValue, "{doc}: {err}");
    }
    // `arm:` that is not a sequence, an entry that is not a mapping.
    for doc in [
        "nika: v1\narm: yes\n",
        "nika: v1\narm:\n  - just-a-string\n",
    ] {
        let err = parse(doc).unwrap_err();
        assert_eq!(err.kind(), ProjectErrorKind::BadValue, "{doc}: {err}");
    }
}

/// The cadence arc's THIRTEEN keys all pass the shape gate — measured
/// 2026-08-18, eight of them were refused as unknown (`project.unknown-key`,
/// `nika arm` exit 2 · `nika run` exit 3) while the cadence grammar
/// defined and validated them. The grammar lives in `nika-cadence`
/// (`registry.rs`); this file judges the SHAPE only, so a project file
/// carrying the thirteen keys must refuse NEITHER `nika arm` NOR
/// `nika run` — and a value outside cadence's law (`chevauchement:
/// nimporte`) still passes HERE and is refused THERE (law 8 — deux
/// parseurs, jamais en désaccord).
#[test]
fn the_thirteen_beat_keys_are_reachable_through_the_project_file() {
    let src = "nika: v1\nceiling: 0.50\narm:\n  - workflow: t.nika.yaml\n    cadence: \"TZ=Europe/Paris 0 3 * * *\"\n    plafond: 0.10\n    manqué: sauter\n    chevauchement: file\n    après_saut: prochain-créneau\n    actif: false\n    raison: pause estivale\n    jusqu_au: 2026-09-01\n    tolérance: 3/4\n    décalage: hash\n    par: thibaut\n    où: local\n";
    let project = parse(src).expect("13 keys are the shape · none is unknown");
    assert_eq!(project.arm().len(), 1);
    let beat = &project.arm()[0];
    assert_eq!(beat.actif, Some(false), "the one judged shape: a bool");
    assert_eq!(beat.chevauchement.as_deref(), Some("file"));
    assert_eq!(beat.apres_saut.as_deref(), Some("prochain-créneau"));
    assert_eq!(beat.raison.as_deref(), Some("pause estivale"));
    assert_eq!(beat.jusqu_au.as_deref(), Some("2026-09-01"));
    assert_eq!(beat.tolerance.as_deref(), Some("3/4"));
    assert_eq!(beat.decalage.as_deref(), Some("hash"));
    assert_eq!(beat.par.as_deref(), Some("thibaut"));

    // A value outside cadence's law passes THIS gate untouched — the
    // grammar refuses it downstream, never here (law 8).
    let nimporte = src.replace("chevauchement: file", "chevauchement: nimporte");
    assert_eq!(
        parse(&nimporte).expect("shape ok").arm()[0]
            .chevauchement
            .as_deref(),
        Some("nimporte"),
        "vocab judges the shape, cadence the grammar"
    );
    // …but `actif:` non-bool IS a shape refusal here.
    let quoted = src.replace("actif: false", "actif: \"false\"");
    let err = parse(&quoted).unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::BadValue, "{err}");
}

/// The closed set still bites: a FOURTEENTH key, outside the thirteen,
/// refuses by name with its line — widening the grammar never meant
/// opening it.
#[test]
fn a_fourteenth_key_still_refuses_by_name() {
    let err = parse(
        "nika: v1\narm:\n  - workflow: w.nika.yaml\n    cadence: \"lundi 9h00\"\n    plafond: 1.00\n    manqué: sauter\n    quatorze: true\n",
    )
    .unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::UnknownKey);
    assert_eq!(err.line(), Some(7));
    assert!(err.detail().contains("quatorze"), "{err}");
    assert!(
        err.remedy().contains("chevauchement"),
        "the remedy now names the thirteen: {err}"
    );
}

/// Discovery walks UP from a nested dir, the first file found
/// governs — and absence anywhere up the walk is `None`, never a
/// refusal (the optionality law).
#[test]
fn discovery_walks_up_first_found_governs() {
    let root = fresh_dir("walk");
    std::fs::write(root.join("nika.yaml"), "nika: v1\nceiling: 0.50\n").expect("seed");
    let nested = root.join("a/b/c");
    std::fs::create_dir_all(&nested).expect("mkdir");
    let (path, project) = discover(&nested)
        .expect("no refusal")
        .expect("found up the walk");
    assert_eq!(
        path,
        root.join("nika.yaml"),
        "the ROOT file, not a closer one"
    );
    assert_eq!(project.ceiling, Some(0.5));

    // A closer file shadows the root's (git's exact law).
    std::fs::write(root.join("a/b/nika.yaml"), "nika: v1\nceiling: 0.25\n").expect("seed2");
    let (path, project) = discover(&nested).expect("ok").expect("found");
    assert_eq!(path, root.join("a/b/nika.yaml"));
    assert_eq!(project.ceiling, Some(0.25));

    // Absent everywhere = None (defaults downstream, zero ceremony).
    let empty = fresh_dir("empty-walk");
    assert!(discover(&empty).expect("no refusal").is_none());
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&empty).ok();
}

/// A `nika.yaml` that cannot be read (it is a DIRECTORY) is the
/// unreadable class — named, never conflated with absent.
#[test]
fn an_unreadable_file_is_not_absent() {
    let dir = fresh_dir("unreadable");
    std::fs::create_dir_all(dir.join("nika.yaml")).expect("a directory of that name");
    let err = discover(&dir).unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::Unreadable, "{err}");
    assert!(err.path().is_some(), "the path names the file: {err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A malformed file found by DISCOVERY speaks path AND line — the
/// error text is one greppable line.
#[test]
fn discovery_errors_speak_path_and_line() {
    let dir = fresh_dir("speak");
    std::fs::write(dir.join("nika.yaml"), "nika: v1\nunknown: 1\n").expect("seed");
    let err = discover(&dir).unwrap_err();
    assert_eq!(err.kind(), ProjectErrorKind::UnknownKey);
    let shown = format!("{err}");
    assert!(shown.contains("nika.yaml:2"), "path:line: {shown}");
    assert!(shown.contains("project.unknown-key"), "the slug: {shown}");
    assert!(err.detail().contains("unknown"), "{shown}");
    std::fs::remove_dir_all(&dir).ok();
}

proptest::proptest! {
    /// The parser NEVER panics — arbitrary bytes in, a verdict out
    /// (Ok or a named error). The closed grammar's first law is
    /// robustness: a hostile file wastes no run.
    #[test]
    fn arbitrary_text_never_panics(s in proptest::prelude::any::<String>()) {
        let _ = parse(&s);
    }

    /// Valid documents round-trip: any generated lawful document
    /// parses, and every parsed value is the generated one.
    #[test]
    fn valid_documents_round_trip(
        ceiling in 0.000_001f64..1_000_000.0,
        days in 1u64..10_000,
        tier in proptest::prelude::prop::sample::select(ProvenanceFloor::ALL.to_vec()),
        beats in proptest::prelude::prop::collection::vec(
            (
                proptest::prelude::prop::string::string_regex("[a-z]{3,8}").unwrap(),
                proptest::prelude::prop::sample::select(vec!["local", "cloud"]),
                proptest::prelude::prop::sample::select(vec!["rattraper", "rattraper-une-fois", "sauter"]),
                0.000_001f64..1_000.0,
            ),
            0..3,
        ),
    ) {
        use std::fmt::Write as _;
        let mut doc = format!("nika: v1\nceiling: {ceiling}\ntraces:\n  keep: {days}d\nregistry:\n  floor: {tier}\n");
        if !beats.is_empty() {
            doc.push_str("arm:\n");
            for (slug, ou, manque, plafond) in &beats {
                write!(
                    doc,
                    "  - workflow: workflows/{slug}.nika.yaml\n    cadence: \"dimanche 18h07\"\n    où: {ou}\n    plafond: {plafond}\n    manqué: {manque}\n"
                ).unwrap();
            }
        }
        let p = parse(&doc).unwrap_or_else(|e| panic!("generated doc must parse: {e}\n{doc}"));
        assert_eq!(p.ceiling.map(f64::to_bits), Some(ceiling.to_bits()), "exact f64 round-trip");
        assert_eq!(p.traces.map(|t| t.keep), Some(Duration::from_secs(days * 86_400)));
        assert_eq!(p.registry.map(|r| r.floor), Some(tier));
        assert_eq!(p.arm().len(), beats.len());
        for (beat, (slug, ou, manque, plafond)) in p.arm().iter().zip(&beats) {
            assert_eq!(beat.workflow, format!("workflows/{slug}.nika.yaml"));
            assert_eq!(beat.ou.map(ArmLocus::as_str), Some(*ou));
            assert_eq!(beat.manque.as_str(), *manque);
            assert_eq!(beat.plafond.to_bits(), plafond.to_bits());
        }
    }

    /// An injected unknown key ALWAYS refuses as UnknownKey — the
    /// closed grammar's anti-silent-drop law holds for arbitrary
    /// key names (ASCII-only keys: YAML identifier-shaped).
    #[test]
    fn injected_unknown_keys_always_refuse(
        key in proptest::prelude::prop::string::string_regex("[a-z]{4,12}")
            .unwrap()
            .prop_filter("a KNOWN key parses fine — only unknowns refuse", |k| {
                !super::TOP_LEVEL_KEYS.contains(&k.as_str())
            }),
    ) {
        let doc = format!("nika: v1\n{key}: 1\n");
        let err = parse(&doc).unwrap_err();
        proptest::prop_assert_eq!(err.kind(), ProjectErrorKind::UnknownKey);
        proptest::prop_assert_eq!(err.line(), Some(2));
    }
}
