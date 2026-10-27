// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Spec 02 §Vision inputs: a `vision:` file is a read inside `permits.fs.read`. A literal
//! path, or a whole bare `const` reference, is judged here, before any token: under an absent
//! block it escapes the zero boundary (`NIKA-AUTH-006`), outside a declared one it escapes the
//! boundary (`NIKA-SEC-004`). A rendered path is the run's to judge, and a `source: url` is not
//! an fs effect at all: the provider fetches it.

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

/// One `infer:` task reading `entry` under the `permits` line (`""` = no block).
fn escapes(permits: &str, entry: &str) -> Vec<CapabilityEscape> {
    let yaml = format!(
        "nika: look\n{permits}const:\n  diagram: \"./private/flow.png\"\ninputs:\n  \
         picked: {{ type: string, required: true }}\ntasks:\n  describe:\n    infer:\n      \
         prompt: \"Describe the flow\"\n      vision:\n        - {entry}\n"
    );
    match parse(&yaml, FileId::new(0), ParseMode::Strict) {
        Ok(wf) => scan_escapes(&wf),
        Err(e) => panic!("the fixture must parse: {e}\n{yaml}"),
    }
}

const FILE: &str = "{ source: file, path: \"./diagrams/flow.png\" }";
const DIAGRAMS: &str = "permits:\n  fs: { read: [\"./diagrams/**\"] }\n";

#[test]
fn a_literal_vision_file_under_an_absent_block_escapes_the_zero_boundary() {
    let found = escapes("", FILE);
    assert_eq!(found.len(), 1, "{found:?}");
    let escape = &found[0];
    assert_eq!((escape.task.as_str(), escape.category), ("describe", "fs"));
    assert!(
        escape.undeclared,
        "absent block = zero authority (NIKA-AUTH-006)"
    );
    assert!(!escape.floor);
    assert!(
        escape.detail.contains("`./diagrams/flow.png`") && escape.detail.contains("fs.read"),
        "{}",
        escape.detail
    );
    assert_eq!(
        escape.fix.as_deref(),
        Some("add \"./diagrams/flow.png\" to permits.fs.read"),
        "an empty direction keeps the machine fix"
    );
}

#[test]
fn a_declared_read_that_misses_the_file_refuses_it() {
    let found = escapes("permits:\n  fs: { read: [\"./docs/**\"] }\n", FILE);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(!found[0].undeclared, "a declared block: NIKA-SEC-004");
    assert_eq!(found[0].category, "fs");
    assert!(
        found[0].detail.contains("`vision`"),
        "the finding names the field: {}",
        found[0].detail
    );
    assert_eq!(
        found[0].fix, None,
        "a declared direction is never widened by the printed repair"
    );
    // A `permits:` block with no `fs:` category grants no read either.
    let no_fs = escapes("permits: {}\n", FILE);
    assert_eq!(no_fs.len(), 1, "{no_fs:?}");
    assert!(!no_fs[0].undeclared);
}

#[test]
fn a_read_grant_that_covers_the_file_admits_it() {
    assert!(escapes(DIAGRAMS, FILE).is_empty());
    assert!(
        escapes(
            "permits:\n  fs: { read: [\"./diagrams/flow.png\"] }\n",
            FILE
        )
        .is_empty(),
        "an exact-file grant"
    );
}

#[test]
fn a_vision_url_is_not_an_fs_effect() {
    let url = "{ source: url, url: \"https://example.com/chart.png\" }";
    assert!(
        escapes("permits: {}\n", url).is_empty(),
        "the declared zero"
    );
    assert!(escapes("", url).is_empty(), "the absent block");
}

#[test]
fn a_bare_const_path_is_judged_like_its_literal() {
    let constant = "{ source: file, path: \"${{ const.diagram }}\" }";
    let found = escapes(DIAGRAMS, constant);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].detail.contains("`./private/flow.png`"),
        "the judged path is the constant's value: {}",
        found[0].detail
    );
    let absent = escapes("", constant);
    assert_eq!(absent.len(), 1, "{absent:?}");
    assert!(absent[0].undeclared);
}

const RENDERED: [&str; 2] = [
    "{ source: file, path: \"${{ inputs.picked }}\" }",
    "{ source: file, path: \"./diagrams/${{ inputs.picked }}\" }",
];

/// Which file a rendered path names is the run's to judge, under a boundary that grants reads.
#[test]
fn a_rendered_path_under_a_read_grant_is_the_runs_to_judge() {
    for entry in RENDERED {
        assert!(escapes(DIAGRAMS, entry).is_empty(), "{entry}");
        assert!(
            escapes("permits:\n  fs: { read: [\"./docs/**\"] }\n", entry).is_empty(),
            "{entry}: a grant the value may still fit"
        );
    }
}

/// Whether ANY read is granted is decidable without the value (the waiver rule in `lib.rs`):
/// a boundary whose `fs.read` admits nothing refuses every rendered path, so the check refuses
/// the entry instead of calling the run ready — `NIKA-AUTH-006` under an absent block,
/// `NIKA-SEC-004` under a declared one (a URL entry needs no read and stays clean).
#[test]
fn a_rendered_path_under_a_boundary_that_grants_no_read_is_refused() {
    for entry in RENDERED {
        let absent = escapes("", entry);
        assert_eq!(absent.len(), 1, "{entry}: {absent:?}");
        assert!(
            absent[0].undeclared,
            "{entry}: zero authority (NIKA-AUTH-006)"
        );
        assert_eq!(absent[0].category, "fs", "{entry}");
        assert!(
            absent[0].detail.contains("`vision`") && absent[0].detail.contains("fs.read"),
            "{entry}: {}",
            absent[0].detail
        );
        for permits in [
            "permits: {}\n",
            "permits:\n  net: { http: [\"example.com\"] }\n",
            "permits:\n  tools: [\"nika:jq\"]\n",
            "permits:\n  fs: { write: [\"./out/**\"] }\n",
            "permits:\n  fs: { read: [] }\n",
        ] {
            let found = escapes(permits, entry);
            assert_eq!(found.len(), 1, "{entry} under {permits}: {found:?}");
            assert!(
                !found[0].undeclared,
                "{entry} under {permits}: NIKA-SEC-004"
            );
            assert_eq!(found[0].category, "fs");
            assert!(
                found[0]
                    .fix
                    .as_deref()
                    .is_some_and(|f| f.contains("permits.fs.read")),
                "{entry} under {permits}: {:?}",
                found[0].fix
            );
        }
    }
}

#[test]
fn a_dot_dot_path_is_judged_where_it_lands() {
    let found = escapes(
        DIAGRAMS,
        "{ source: file, path: \"./diagrams/../secrets/key.png\" }",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].detail.contains("lands at"),
        "the lexical fold is shown: {}",
        found[0].detail
    );
    assert!(
        escapes(
            DIAGRAMS,
            "{ source: file, path: \"./diagrams/raw/../flow.png\" }"
        )
        .is_empty(),
        "a `..` that stays inside the grant is a read inside it"
    );
}

#[test]
fn every_vision_file_is_judged_and_named() {
    let yaml = format!(
        "nika: look\n{DIAGRAMS}tasks:\n  describe:\n    infer:\n      prompt: \"Compare\"\n      \
         vision:\n        - {{ source: file, path: \"./diagrams/flow.png\" }}\n        - {{ \
         source: url, url: \"https://example.com/legend.png\" }}\n        - {{ source: file, \
         path: \"./photos/team.jpg\" }}\n        - {{ source: file, path: \"/etc/hosts\" }}\n"
    );
    let found = scan_escapes(&parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse"));
    let paths: Vec<bool> = ["./photos/team.jpg", "/etc/hosts"]
        .iter()
        .map(|p| found.iter().any(|e| e.detail.contains(&format!("`{p}`"))))
        .collect();
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(paths, [true, true], "{found:?}");
}
