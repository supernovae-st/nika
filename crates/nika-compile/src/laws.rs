// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The laws, schemas and prompt helpers the assembler emits: every jq an anchor law or a
//! fold runs, every schema a language step answers, the head words that make a draft a
//! translation, the layout a bulleted draft is asked. Text only; no node is built here.

use super::plan::Step;
use super::shape;
use serde_json::{Value, json};

/// An anchor law over the whole corpus: every string the step could copy from is a
/// candidate; non-string facts are compared through their JSON text.
/// Every language step the assembler seats carries an explicit deadline: the runtime's
/// buffered default is thirty seconds for a cloud model, and a reasoning model answering a
/// schema in a fresh sandbox routinely needs more. Five minutes is the documented ceiling
/// a human is asked to wait for one step.
pub(super) const INFER_TIMEOUT: &str = "5m";

/// Both sides fold before an anchor is compared: runs of whitespace to one space, spaces
/// around JSON punctuation away, case down. A model may wrap a line, drop a double space
/// or re-serialize a record it was shown; it may not change a word.
pub(super) const FOLD: &str = r#"gsub("\\s*,\\s*"; ",") | gsub("\\s*:\\s*"; ":") | gsub("\\s*\\{\\s*"; "{") | gsub("\\s*\\}\\s*"; "}") | gsub("\\s*\\[\\s*"; "[") | gsub("\\s*\\]\\s*"; "]") | gsub("\\s+"; " ") | ascii_downcase"#;

/// The corpus of a law: every fact of the input except the judged keys, as the value the
/// prompt showed (a string as is, anything else as its JSON text) and as the `name: value`
/// line the prompt renders it on, so an anchor copied from either form is found. Folded.
pub(super) fn corpus(excluded: &str) -> String {
    format!(
        "[$root | del({excluded}) | to_entries[] | (.value | if type == \"string\" then . else tojson end) as $v | ($v, \"\\(.key): \\($v)\") | {FOLD}] as $corpus"
    )
}

pub(super) fn anchor_law(key: &str, required: bool) -> String {
    let empty = if required {
        "($f.anchor | length) > 0 and"
    } else {
        "($f.anchor | length) == 0 or"
    };
    format!(
        ". as $root | {} | all(.{key}[]; . as $f | {empty} any($corpus[]; contains($f.anchor | {FOLD})))",
        corpus(&format!(".{key}"))
    )
}

/// The draft law: a nonempty body, and every declared claim anchored in the corpus the
/// draft was given. The body is judged, never part of its own corpus.
pub(super) fn draft_law() -> String {
    format!(
        ". as $root | {} | ($root.body | length) > 0 and all(.facts_used[]; . as $f | ($f.anchor | length) > 0 and any($corpus[]; contains($f.anchor | {FOLD})))",
        corpus(".facts_used, .body")
    )
}

/// The record keyed by the invocation's `record_id` in an object directory.
pub(super) const SELECT_BY_KEY: &str = ". as $lookup | ($lookup.directory | fromjson)[$lookup.id]";

/// The one record whose field equals the literal identifier: the first match in an array
/// directory, the keyed entry in an object directory.
pub(super) const SELECT_BY_FIELD: &str = ". as $l | ($l.directory | fromjson) | if type == \"array\" then (map(select(type == \"object\" and .[$l.field] == $l.id)) | .[0]) else .[$l.id] end";

/// The header order of a CSV source: its first line, `\r` trimmed, split on commas,
/// the surrounding double quotes stripped from each cell. A quoted header holding a
/// comma is out of scope: the cells are then a superset, still emitted first.
pub(super) const SOURCE_COLUMNS: &str =
    r#"split("\n") | .[0] | rtrimstr("\r") | split(",") | map(ltrimstr("\"") | rtrimstr("\""))"#;

/// The header order of several CSV sources read in a fan-out: each source's header in turn,
/// a column named twice kept where it first appeared. What a join writes back as CSV.
pub(super) const SOURCE_COLUMNS_UNION: &str = r#"[.[] | split("\n") | .[0] | rtrimstr("\r") | split(",") | map(ltrimstr("\"") | rtrimstr("\""))] | add | reduce .[] as $c ([]; if any(.[]; . == $c) then . else . + [$c] end)"#;

/// The lines of a text source: split on newlines, `\r` trimmed, the empty segment after
/// the file's final newline dropped (it is the terminator, not a line).
pub(super) const LINES: &str =
    r#"split("\n") | map(rtrimstr("\r")) | if .[-1] == "" then .[:-1] else . end"#;

/// The zip of a fan-out: one `{path, text}` per read file, in item order.
pub(super) const ZIP: &str =
    ". as $r | [range(0; $r.texts | length) as $i | {path: $r.paths[$i], text: $r.texts[$i]}]";

/// The fold of a fan-out: one document with a heading per file, in item order.
pub(super) const FOLD_DOCUMENTS: &str = ". as $r | [range(0; $r.texts | length) as $i | \"## \\($r.paths[$i])\\n\\n\\($r.texts[$i])\"] | join(\"\\n\\n\")";

/// The deterministic count and totals of a computed result: `{count, totals}` where the
/// totals sum every numeric column of an array of objects (identifier columns excluded),
/// rounded to two decimals. A language step that must state how many rows were kept and
/// what they add up to anchors those claims here, never in its own arithmetic.
pub(super) const SUMMARY: &str = r#". as $c | if ($c | type) == "array" then {count: ($c | length), totals: ([$c[] | select(type == "object") | to_entries[] | select(((.key | test("(^|_)id$")) | not) and (((.value | type) == "number") or (((.value | type) == "string") and (.value | test("^-?[0-9]+([.][0-9]+)?$"))))) | {key, value: (.value | tonumber)}] | group_by(.key) | map({key: .[0].key, value: ((map(.value) | add) * 100 | round / 100)}) | from_entries)} else {count: (if ($c | type) == "object" then ($c | length) else 1 end), totals: {}} end"#;

/// The extract schema every extract step answers: named fields, each with its anchor.
pub(super) fn extract_schema() -> Value {
    json!({"type": "object", "additionalProperties": false, "required": ["fields"], "properties": {"fields": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["name", "value", "anchor"], "properties": {"name": {"type": "string", "minLength": 1}, "value": {"type": "string", "description": "the field's value as the source states it, empty when the source does not state it"}, "anchor": {"type": "string", "description": "the source span the value comes from: one contiguous span of the supplied text copied character for character, never a paraphrase; empty beside an empty value"}}}}}})
}

/// The per-item extract law: one record per item, every anchor copied from its own item's
/// text (an empty anchor allowed beside an empty value, as in the one-shot law).
pub(super) fn per_item_extract_law() -> String {
    format!(
        ". as $r | ($r.extracts | length) == ($r.items | length) and all(range(0; $r.extracts | length); . as $i | ([$r.items[$i].text | {FOLD}] as $corpus | all($r.extracts[$i].fields[]; . as $f | ($f.anchor | length) == 0 or any($corpus[]; contains($f.anchor | {FOLD})))))"
    )
}

/// The fan-in of per-item extracts: one object per item, keyed by the field names the
/// model returned, in item order.
pub(super) const FOLD_FIELDS: &str =
    ". as $r | [$r.extracts[] | .fields | map({key: .name, value: .value}) | from_entries]";

/// The category schema of a classify step: the named categories, or any nonempty word.
pub(super) fn category_schema(step: &Step) -> Value {
    if step.categories.is_empty() {
        json!({"type": "string", "minLength": 1})
    } else {
        json!({"type": "string", "enum": step.categories})
    }
}

/// The records a per-record classification routed to one category, in source order.
pub(super) const ROUTE: &str = ". as $r | [range(0; $r.records | length) as $i | select($r.categories[$i].category == $r.category) | $r.records[$i]]";

/// Every record with the category the classification gave it, in source order.
pub(super) const ANNOTATE: &str = ". as $r | [range(0; $r.records | length) as $i | $r.records[$i] + {category: $r.categories[$i].category}]";

/// Heads that make a draft a translation (EN · FR · ES · IT · PT · DE, folded).
pub(super) const TRANSLATE_HEADS: &[&str] = &[
    "translate",
    "translates",
    "traduis",
    "traduisez",
    "traduire",
    "traduce",
    "traducir",
    "traduci",
    "traducir",
    "traduza",
    "traduzir",
    "ubersetze",
    "ubersetzen",
];

/// A draft whose clause is led by a translation head restates the source in another
/// language: none of its sentences is a substring of the source, so an anchor law cannot
/// judge it. Its admission is a nonempty body.
pub(super) fn translation(step: &Step) -> bool {
    let head = shape::fold(&step.evidence);
    head.split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())
        .is_some_and(|w| TRANSLATE_HEADS.contains(&w))
}

/// The translation law: a nonempty body. Anchors are not required (see [`translation`]).
pub(super) fn translation_law() -> String {
    ". as $root | ($root.body | length) > 0".to_owned()
}

/// Words that ask for bullets or points (EN · FR · ES · IT · PT · DE, folded).
pub(super) const BULLET_WORDS: &[&str] = &[
    "bullet",
    "bullets",
    "point",
    "points",
    "puce",
    "puces",
    "punto",
    "punti",
    "vineta",
    "vinetas",
    "topico",
    "topicos",
    "stichpunkt",
    "stichpunkte",
    "aufzahlungspunkt",
    "aufzahlungspunkte",
];

/// A draft asked as bullets or points ("in 3 punti", "en 3 puces", "as 5 bullets") is laid
/// out one per line: a model that runs three points into one line answers the count with
/// a shape the request did not ask for.
pub(super) fn bullet_layout(text: &str) -> &'static str {
    let folded = shape::fold(text);
    let asks = folded
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| BULLET_WORDS.contains(&w));
    if asks {
        " Put each bullet or point on its own line, each line starting with `- `."
    } else {
        ""
    }
}

/// The draft schema every draft step answers: a body and its anchored claims.
pub(super) fn draft_schema() -> Value {
    json!({"type": "object", "additionalProperties": false, "required": ["body", "facts_used"], "properties": {"body": {"type": "string", "minLength": 1}, "facts_used": {"type": "array", "items": {"type": "object", "additionalProperties": false, "required": ["claim", "anchor"], "properties": {"claim": {"type": "string", "minLength": 1, "description": "the fact as the draft states it, in the draft's own words"}, "anchor": {"type": "string", "minLength": 1, "description": "the source span the claim rests on: one contiguous span of the supplied text copied character for character, never a paraphrase"}}}}}})
}

/// The per-item draft law: one draft per item, a nonempty body each, every claim anchored
/// in its own item's text (whitespace folded on both sides).
pub(super) fn per_item_law() -> String {
    format!(
        ". as $r | ($r.drafts | length) == ($r.items | length) and all(range(0; $r.drafts | length); . as $i | ($r.drafts[$i].body | length) > 0 and ([$r.items[$i].text | {FOLD}] as $corpus | all($r.drafts[$i].facts_used[]; . as $f | ($f.anchor | length) > 0 and any($corpus[]; contains($f.anchor | {FOLD})))))"
    )
}

/// The per-item translation law: one translation per item, a nonempty body each.
pub(super) fn per_item_translation_law() -> String {
    ". as $r | ($r.drafts | length) == ($r.items | length) and all($r.drafts[]; (.body | length) > 0)".to_owned()
}

/// The fan-in of per-item drafts: one heading per file, named after the file, in item order.
pub(super) const FOLD_DRAFTS: &str = ". as $r | [range(0; $r.drafts | length) as $i | \"## \\($r.items[$i].path | split(\"/\") | last)\\n\\n\\($r.drafts[$i].body)\"] | join(\"\\n\\n\")";
