// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The profile the strict door admits ([`super::ADMISSION_PROFILE`]), closed at every level. It
//! fixes:
//! - the kinds a release ships and their row files;
//! - each kind's fields and their types;
//! - the proof levels a kind may claim;
//! - the relations the engine reads;
//! - the text and path grammar every admitted value follows (the r1 contract §5, §6, §9).
//!
//! The producer writes to this same contract. A change here is a change of the profile, made on
//! both sides with the same vectors.

use serde_json::{Map, Value};

use super::{Checked, RefusalCode, refuse};

// ── the closed, typed row schema ───────────────────────────────────────────────────────────────

/// The type of an admitted field.
#[derive(Clone, Copy, Debug)]
pub(super) enum Ty {
    /// One line: 1 to 300 admitted code points, no TAB or LF, not blank.
    Line,
    /// Text of at most 8192 bytes, TAB and LF admitted, not blank.
    Text,
    /// Text that may be empty or blank.
    MaybeText,
    /// At most 64 lines.
    Lines,
    /// One of the admitted licences ([`LICENCES`]).
    Licence,
    /// `project` · `derived-from-third-party` · `third-party`.
    Ownership,
    /// A lowercase sha256.
    Sha256,
    /// A specification commit (40 or 64 lowercase hex).
    Spec,
    /// `blocks/<slug>.nika`.
    BlockFile,
    /// `{facet: [value]}`: at most 32 facets of at most 32 lines each.
    Facets,
    /// At most 64 holes, each exactly `{name, owner}` with an optional `note`.
    Holes,
    /// Exactly `{verifier_sha256, spec_sha, sha256, verdict}`.
    Receipt,
    /// Exactly `{binary, spec_sha}`.
    Pin,
    /// Exactly `{sources: [source id, …]}`, 1 to 64 of them.
    Provenance,
}

/// The fields of an object a row nests (`required`, then `optional`), by the type that holds it:
/// a pin, a provenance, a receipt, or each hole of a list of holes. `None` for every other type.
fn nested(ty: Ty) -> Option<Fields> {
    match ty {
        Ty::Pin => Some((&[("binary", Ty::Line), ("spec_sha", Ty::Spec)], &[])),
        Ty::Provenance => Some((&[("sources", Ty::Lines)], &[])),
        Ty::Receipt => Some((
            &[
                ("verifier_sha256", Ty::Sha256),
                ("spec_sha", Ty::Spec),
                ("sha256", Ty::Sha256),
                ("verdict", Ty::Line),
            ],
            &[],
        )),
        Ty::Holes => Some((
            &[("name", Ty::Line), ("owner", Ty::Line)],
            &[("note", Ty::Text)],
        )),
        _ => None,
    }
}

/// An object's fields: the required ones, then the optional ones.
type Fields = (&'static [(&'static str, Ty)], &'static [(&'static str, Ty)]);

/// One of F7's first two passes, applied to one object.
type Judge = fn(&Map<String, Value>, &str, &[(&str, Ty)], &[(&str, Ty)]) -> Checked<()>;

/// F7 (the r1 contract §10): a row's closed schema, in three passes over the whole row. First no
/// unknown key at any depth, then no missing key at any depth, then each value's type in schema
/// order, nested objects depth-first. The first two passes go into a value only when it has its
/// expected container type; any other value is left to the third.
pub(super) fn closed_row(row: &Value, id: &str, fields: &[(&str, Ty)]) -> Checked<()> {
    walk(row, id, fields, &[], unknown_key)?;
    walk(row, id, fields, &[], missing_key)?;
    types(row, id, fields, &[])
}

/// `judge` the object `value`, then walk on, depth-first in schema order, into each field that
/// holds an object where an object is expected, or a list where a list of holes is expected
/// (into each of its objects).
fn walk(
    value: &Value,
    what: &str,
    required: &[(&str, Ty)],
    optional: &[(&str, Ty)],
    judge: Judge,
) -> Checked<()> {
    let Some(map) = value.as_object() else {
        return Ok(());
    };
    judge(map, what, required, optional)?;
    for (name, ty) in required.iter().chain(optional) {
        let (Some(item), Some((inner, maybe))) = (map.get(*name), nested(*ty)) else {
            continue;
        };
        match (ty, item) {
            (Ty::Holes, Value::Array(holes)) => {
                for (at, hole) in holes.iter().enumerate() {
                    walk(hole, &format!("{what}.{name}[{at}]"), inner, maybe, judge)?;
                }
            }
            (Ty::Pin | Ty::Provenance | Ty::Receipt, Value::Object(_)) => {
                walk(item, &format!("{what}.{name}"), inner, maybe, judge)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// The first pass on one object: no key outside its fields.
fn unknown_key(
    map: &Map<String, Value>,
    what: &str,
    required: &[(&str, Ty)],
    optional: &[(&str, Ty)],
) -> Checked<()> {
    let known = |key: &str| {
        required
            .iter()
            .chain(optional)
            .any(|(name, _)| *name == key)
    };
    match map.keys().find(|key| !known(key.as_str())) {
        Some(key) => refuse("F7", RefusalCode::UnknownField, format!("{what}.{key}")),
        None => Ok(()),
    }
}

/// The second pass on one object: every required field present.
fn missing_key(
    map: &Map<String, Value>,
    what: &str,
    required: &[(&str, Ty)],
    _optional: &[(&str, Ty)],
) -> Checked<()> {
    match required.iter().find(|(name, _)| !map.contains_key(*name)) {
        Some((name, _)) => refuse("F7", RefusalCode::MissingField, format!("{what}.{name}")),
        None => Ok(()),
    }
}

/// The third pass: `value` an object, each of its fields of its type, in schema order, nested
/// objects depth-first (`what` names it in a refusal).
fn types(
    value: &Value,
    what: &str,
    required: &[(&str, Ty)],
    optional: &[(&str, Ty)],
) -> Checked<()> {
    let Some(map) = value.as_object() else {
        return refuse(
            "F7",
            RefusalCode::FieldType,
            format!("{what} is not an object"),
        );
    };
    for (name, ty) in required.iter().chain(optional) {
        if let Some(field) = map.get(*name) {
            typed(field, *ty, &format!("{what}.{name}"))?;
        }
    }
    Ok(())
}

/// `value` is of type `ty` (`what` names it in a refusal).
pub(super) fn typed(value: &Value, ty: Ty, what: &str) -> Checked<()> {
    if let Some((required, optional)) = nested(ty) {
        if matches!(ty, Ty::Holes) {
            let Some(holes) = value.as_array().filter(|holes| holes.len() <= 64) else {
                return refuse(
                    "F7",
                    RefusalCode::FieldType,
                    format!("{what} is not a list of holes"),
                );
            };
            for (at, hole) in holes.iter().enumerate() {
                types(hole, &format!("{what}[{at}]"), required, optional)?;
            }
            return Ok(());
        }
        types(value, what, required, optional)?;
        if matches!(ty, Ty::Provenance) && value["sources"].as_array().is_none_or(Vec::is_empty) {
            return refuse(
                "F7",
                RefusalCode::FieldType,
                format!("{what} names no source"),
            );
        }
        return Ok(());
    }
    let ok = match ty {
        Ty::Line => value.as_str().is_some_and(line),
        Ty::Text => value.as_str().is_some_and(|t| text(t, false)),
        Ty::MaybeText => value.as_str().is_some_and(|t| text(t, true)),
        Ty::Lines => list(value, 64, |item| item.as_str().is_some_and(line)),
        Ty::Licence => value.as_str().is_some_and(|l| LICENCES.contains(&l)),
        Ty::Ownership => value.as_str().is_some_and(|o| OWNERSHIP.contains(&o)),
        Ty::Sha256 => hex64(value),
        Ty::Spec => spec_sha(value),
        Ty::BlockFile => value.as_str().is_some_and(block_path),
        Ty::Facets => value.as_object().is_some_and(|facets| {
            facets.len() <= 32
                && facets.iter().all(|(facet, values)| {
                    token(facet, 64, |c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || "_-".contains(c)
                    }) && list(values, 32, |item| item.as_str().is_some_and(line))
                })
        }),
        // Judged above, as the objects they hold.
        Ty::Holes | Ty::Receipt | Ty::Pin | Ty::Provenance => true,
    };
    if ok {
        Ok(())
    } else {
        refuse(
            "F7",
            RefusalCode::FieldType,
            format!("{what} is not {ty:?}"),
        )
    }
}

/// A list of at most `max` items, each `item`.
pub(super) fn list(value: &Value, max: usize, item: impl Fn(&Value) -> bool) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.len() <= max && items.iter().all(item))
}

// ── the text grammar (§9), shared with profile r2 ──────────────────────────────────────────────

pub(super) use nika_compile_seats::foundry::release::grammar::{
    base_name, blank, hex64, id_name, line, lower_hex, policy_token, safe_relative, spec_sha, stem,
    token,
};

/// Text of at most 8192 bytes of admitted code points (TAB and LF too), blank only where
/// `maybe` allows.
pub(super) fn text(text: &str, maybe: bool) -> bool {
    nika_compile_seats::foundry::release::grammar::text(text, 8192, maybe)
}

/// A file a block or the notices present: UTF-8, 1 to 65536 bytes of admitted code points (TAB
/// and LF too), not blank.
pub(super) fn free_text(bytes: &[u8]) -> bool {
    nika_compile_seats::foundry::release::grammar::file_text(bytes, 65_536)
}

/// `blocks/<slug>.nika`, the slug `[a-z0-9][a-z0-9-]{0,99}`.
pub(super) fn block_path(path: &str) -> bool {
    path.strip_prefix("blocks/")
        .and_then(|rest| rest.strip_suffix(".nika"))
        .is_some_and(|slug| {
            token(slug, 100, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
            })
        })
}

/// The admitted licence a `LICENSES/<licence>.txt` path names.
pub(super) fn licence_path(path: &str) -> Option<&str> {
    path.strip_prefix("LICENSES/")
        .and_then(|rest| rest.strip_suffix(".txt"))
        .filter(|licence| LICENCES.contains(licence))
}

/// The target every row is pinned to and every check receipt is bound to, as the manifest
/// names it: a label, a specification, and the verifier the producer actually ran.
pub(super) struct Target {
    pub(super) binary: String,
    pub(super) spec_sha: String,
    pub(super) verifier_sha256: String,
}

impl Target {
    pub(super) fn of(manifest: &Value) -> Self {
        let text = |key: &str| {
            manifest["target"][key]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        };
        Self {
            binary: text("binary"),
            spec_sha: text("spec_sha"),
            verifier_sha256: text("verifier_sha256"),
        }
    }
}

// ── the profile ────────────────────────────────────────────────────────────────────────────────

/// The statuses a shipped row may hold (a deprecated row is the producer's to exclude).
pub(super) const STATUSES: [&str; 4] = ["CANDIDATE", "EXPERIMENTAL", "QUALIFIED", "PROMOTED"];
/// Who owns a source's material.
pub(super) const OWNERSHIP: [&str; 3] = ["project", "derived-from-third-party", "third-party"];
/// The licences r1 admits: the project's own, the engine's and the specification's.
pub(super) const LICENCES: [&str; 2] = ["AGPL-3.0-or-later", "Apache-2.0"];

/// One kind the profile ships: its row file, its id prefix, the proof levels it may claim, and
/// its fields beyond the ones every row carries (`id` · `kind` · `sha256` · `title` · `status` ·
/// `proof_level` · `pin`, and `provenance` for every kind but a source).
pub(super) struct KindProfile {
    pub(super) kind: &'static str,
    pub(super) file: &'static str,
    pub(super) prefix: &'static str,
    pub(super) proofs: &'static [&'static str],
    pub(super) fields: &'static [(&'static str, Ty)],
}

/// The kinds a release ships: what the engine reads, and the sources their lineage cites. Only a
/// block may claim `CHECKED`, with its receipt; every other kind carries `NONE` (§6.4).
pub(super) static KINDS: &[KindProfile] = &[
    KindProfile {
        kind: "block",
        file: "knowledge/blocks.jsonl",
        prefix: "block",
        proofs: &["CHECKED"],
        fields: &[
            ("purpose", Ty::Text),
            ("file", Ty::BlockFile),
            ("file_sha256", Ty::Sha256),
            ("holes", Ty::Holes),
            ("effects", Ty::Lines),
            ("authority", Ty::Lines),
            ("interfaces", Ty::Lines),
            ("callables", Ty::Lines),
            ("known_failure_modes", Ty::Lines),
            ("check_receipt", Ty::Receipt),
        ],
    },
    KindProfile {
        kind: "diagnostic",
        file: "knowledge/diagnostics.jsonl",
        prefix: "diagnostic",
        proofs: &["NONE"],
        fields: &[],
    },
    KindProfile {
        kind: "family",
        file: "knowledge/families.jsonl",
        prefix: "family",
        proofs: &["NONE"],
        fields: &[("need", Ty::Text), ("facets", Ty::Facets)],
    },
    KindProfile {
        kind: "pattern",
        file: "knowledge/patterns.jsonl",
        prefix: "pattern",
        proofs: &["NONE"],
        fields: &[("purpose", Ty::Text), ("notes", Ty::MaybeText)],
    },
    KindProfile {
        kind: "pattern_pack",
        file: "knowledge/pattern_packs.jsonl",
        prefix: "pack",
        proofs: &["NONE"],
        fields: &[],
    },
    KindProfile {
        kind: "repair_principle",
        file: "knowledge/repair_principles.jsonl",
        prefix: "repair",
        proofs: &["NONE"],
        fields: &[("strategy", Ty::Text)],
    },
    KindProfile {
        kind: "source_artifact",
        file: "knowledge/source_artifacts.jsonl",
        prefix: "src",
        proofs: &["NONE"],
        fields: &[
            ("licence", Ty::Licence),
            ("ownership", Ty::Ownership),
            ("upstream", Ty::MaybeText),
        ],
    },
];

/// The relations the engine reads: name, domain kind, range kind.
pub(super) static RELATIONS: &[(&str, &str, &str)] = &[
    ("CONTAINS", "pattern_pack", "pattern"),
    ("REALIZES", "block", "pattern"),
    ("RECOMMENDS", "family", "pattern_pack"),
    ("SUGGESTS_REPAIR", "diagnostic", "repair_principle"),
];

/// Every kind's row file, in the profile's order (a fixture writes them all).
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn kind_files() -> impl Iterator<Item = (&'static str, &'static str)> {
    KINDS.iter().map(|kind| (kind.kind, kind.file))
}
