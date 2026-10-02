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

// ── the text grammar (§9) ──────────────────────────────────────────────────────────────────────

/// The code points no admitted text carries (§9.3): a closed table, not a Unicode category. TAB
/// and LF are judged apart ([`printable`]), and so is every noncharacter `U+xFFFE`/`U+xFFFF`.
/// The presentation selectors U+FE0E and U+FE0F are admitted.
pub(super) const FORBIDDEN: [(char, char); 31] = [
    ('\u{0}', '\u{8}'),
    ('\u{B}', '\u{1F}'),
    ('\u{7F}', '\u{9F}'),
    ('\u{AD}', '\u{AD}'),
    ('\u{34F}', '\u{34F}'),
    ('\u{600}', '\u{605}'),
    ('\u{61C}', '\u{61C}'),
    ('\u{6DD}', '\u{6DD}'),
    ('\u{70F}', '\u{70F}'),
    ('\u{890}', '\u{891}'),
    ('\u{8E2}', '\u{8E2}'),
    ('\u{115F}', '\u{1160}'),
    ('\u{17B4}', '\u{17B5}'),
    ('\u{180B}', '\u{180F}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{2028}', '\u{202E}'),
    ('\u{2060}', '\u{206F}'),
    ('\u{3164}', '\u{3164}'),
    ('\u{E000}', '\u{F8FF}'),
    ('\u{FDD0}', '\u{FDEF}'),
    ('\u{FE00}', '\u{FE0D}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{FFA0}', '\u{FFA0}'),
    ('\u{FFF9}', '\u{FFFF}'),
    ('\u{110BD}', '\u{110BD}'),
    ('\u{110CD}', '\u{110CD}'),
    ('\u{13430}', '\u{1343F}'),
    ('\u{1BCA0}', '\u{1BCA3}'),
    ('\u{1D173}', '\u{1D17A}'),
    ('\u{E0000}', '\u{E0FFF}'),
    ('\u{F0000}', '\u{10FFFF}'),
];

/// A code point a presented text may carry: TAB and LF only where `multiline`, never a
/// noncharacter, never one of [`FORBIDDEN`]. (Surrogates cannot occur in a `str`.)
pub(super) fn printable(c: char, multiline: bool) -> bool {
    if c == '\t' || c == '\n' {
        return multiline;
    }
    let point = u32::from(c);
    point & 0xFFFE != 0xFFFE
        && !FORBIDDEN
            .iter()
            .any(|(low, high)| (*low..=*high).contains(&c))
}

/// Every code point is `White_Space` (Rust's `char::is_whitespace`, §9.2): the empty text included.
pub(super) fn blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// One line: 1 to 300 admitted code points, no TAB or LF, not blank.
pub(super) fn line(text: &str) -> bool {
    !blank(text) && text.chars().count() <= 300 && text.chars().all(|c| printable(c, false))
}

/// Text of at most 8192 bytes of admitted code points (TAB and LF too), blank only where
/// `maybe` allows.
pub(super) fn text(text: &str, maybe: bool) -> bool {
    (maybe || !blank(text)) && text.len() <= 8192 && text.chars().all(|c| printable(c, true))
}

/// A file a block or the notices present: UTF-8, 1 to 65536 bytes of admitted code points (TAB
/// and LF too), not blank.
pub(super) fn free_text(bytes: &[u8]) -> bool {
    (1..=65_536).contains(&bytes.len())
        && std::str::from_utf8(bytes)
            .is_ok_and(|text| !blank(text) && text.chars().all(|c| printable(c, true)))
}

/// A relative POSIX path with no empty, `.` or `..` segment, no backslash, NUL, absolute or
/// drive form.
pub(super) fn safe_relative(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains('\0')
        && !path.starts_with('/')
        && !drive
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
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

/// A lowercase sha256.
pub(super) fn hex64(value: &Value) -> bool {
    value.as_str().is_some_and(|s| lower_hex(s, 64))
}

/// A specification commit: 40 or 64 lowercase hex.
pub(super) fn spec_sha(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| lower_hex(s, 40) || lower_hex(s, 64))
}

/// Exactly `len` lowercase hex digits.
pub(super) fn lower_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// A policy id: `[a-z0-9][a-z0-9-]{0,63}`.
pub(super) fn policy_token(text: &str) -> bool {
    token(text, 64, |c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
    }) && text
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// A token: a first character that is an ASCII letter or digit, at most `max` characters, each
/// admitted by `rest`.
pub(super) fn token(text: &str, max: usize, rest: impl Fn(char) -> bool) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && text.chars().count() <= max
        && text.chars().all(rest)
}

/// A row id's name after its prefix: `[A-Za-z0-9][A-Za-z0-9._:/@+-]{0,199}`.
pub(super) fn id_name(name: &str) -> bool {
    token(name, 200, |c| {
        c.is_ascii_alphanumeric() || "._:/@+-".contains(c)
    })
}

/// `knowledge/families.jsonl` → `families`.
pub(super) fn stem(path: &str) -> String {
    base_name(path).trim_end_matches(".jsonl").to_owned()
}

/// `knowledge/families.jsonl` → `families.jsonl`.
pub(super) fn base_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
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
