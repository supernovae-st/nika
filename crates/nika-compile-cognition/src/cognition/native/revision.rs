// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A revision in words may replace one path while retaining every other obligation.
//! The proof compares complete documents: only that literal path and the workflow name
//! may differ. Paths recalled by the change but already present in the base are not new
//! destinations. An explicit addition never proves a replacement. A proven substitution
//! is journaled without making the model invent a gap; other omissions remain pending.

use serde_json::Value;

use crate::CompileRequest;
use crate::fidelity::Diagnostic;
use crate::types::{EditChange, Input};

/// Words that state a change as a replacement (FR · EN, folded, whole words).
const REPLACING: &[&str] = &[
    "finalement",
    "plutot",
    "a la place",
    "au lieu",
    "remplace",
    "remplacer",
    "utilise",
    "utiliser",
    "change",
    "changer",
    "modifie",
    "modifier",
    "desormais",
    "dorenavant",
    "instead",
    "rather",
    "replace",
    "use",
    "switch",
    "actually",
    "from now on",
];

/// Words that state a change as an addition (FR · EN, folded, whole words): whatever else the
/// change says, the old path is then kept, never replaced.
const ADDING: &[&str] = &[
    "aussi",
    "egalement",
    "en plus",
    "de plus",
    "en outre",
    "ajoute",
    "ajouter",
    "rajoute",
    "also",
    "too",
    "as well",
    "additionally",
    "in addition",
    "add",
    "append",
];

/// How many gaps an accepted candidate's record keeps: a path is waived only through a gap the
/// record keeps, so the human always sees what the candidate leaves behind.
pub(super) const KEPT_GAPS: usize = 8;

/// The base and the change words of a revision in words; none for a creation or a structured
/// edit.
pub(super) fn of(request: &CompileRequest) -> Option<(String, String)> {
    match &request.input {
        Input::Edit {
            source,
            change: EditChange::Text(words),
        } => Some((source.clone(), words.clone())),
        _ => None,
    }
}

/// The paths the request states that a revised candidate may leave unopened while the human
/// has not disposed of them. A recalled path needs a complete structural substitution
/// before it can become a pending gap; it is never automatically superseded. Empty for
/// a creation. A gap never authorizes application or execution by itself.
pub(super) fn waivable(
    intent: &str,
    revision: Option<&(String, String)>,
    gaps: &[String],
    candidate: &str,
) -> Vec<String> {
    let Some((base, words)) = revision else {
        return Vec::new();
    };
    let kept: Vec<&str> = gaps
        .iter()
        .map(|gap| gap.trim())
        .filter(|gap| !gap.is_empty())
        .take(KEPT_GAPS)
        .collect();
    stated(intent)
        .into_iter()
        .filter(|path| {
            kept.iter().any(|gap| names(gap, path))
                && (!names(words, path) || substitution(base, words, candidate, path).is_some())
        })
        .collect()
}

/// A gap the change supersedes on proof: the stated path the candidate no longer opens, the
/// path the change puts in its place, and the recorded explanation (model or compiler).
pub(super) struct Superseded {
    pub(super) path: String,
    pub(super) by: String,
    pub(super) gap: String,
}

/// An accepted candidate's gaps: those still pending, each disposed of by the human before the
/// candidate is READY, and those a revision supersedes on proof. A gap is superseded only when
/// the one stated path it leaves behind is proven replaced; anything else it names keeps it
/// pending.
pub(super) fn settle(
    intent: &str,
    request: &CompileRequest,
    candidate: &str,
    gaps: &[&str],
) -> (Vec<String>, Vec<Superseded>) {
    let revision = of(request);
    let mut pending = Vec::new();
    let mut superseded = Vec::new();
    for gap in gaps {
        let left = waivable(intent, revision.as_ref(), &[(*gap).to_owned()], candidate);
        let proven = match (&revision, left.as_slice()) {
            (Some((base, words)), [path]) => {
                replacement(base, words, candidate, path).map(|by| Superseded {
                    path: path.clone(),
                    by,
                    gap: (*gap).to_owned(),
                })
            }
            _ => None,
        };
        match proven {
            Some(proof) => superseded.push(proof),
            None => pending.push((*gap).to_owned()),
        }
    }
    (pending, superseded)
}

/// The path that replaces `path`, when the change proves it: the change words state a
/// replacement and no addition, omit the old path and name exactly one new path
/// (retained base paths may be recalled). The candidate must be the base with every
/// `path` value replaced by it and nothing else changed (the workflow's name aside).
fn replacement(base: &str, words: &str, candidate: &str, path: &str) -> Option<String> {
    if names(words, path) {
        return None;
    }
    substitution(base, words, candidate, path)
}

/// Structural substitution only; a named old path still needs a human gap decision.
fn substitution(base: &str, words: &str, candidate: &str, path: &str) -> Option<String> {
    if !says(words, REPLACING) || says(words, ADDING) {
        return None;
    }
    let mut expected = crate::edit::literal_projection(base)?;
    let new_paths: Vec<String> = stated(words)
        .into_iter()
        .filter(|named| !contains_path(&expected, named))
        .collect();
    let [by]: [String; 1] = new_paths.try_into().ok()?;
    if same(&by, path) {
        return None;
    }
    if !substitute(&mut expected, path, &by) {
        return None;
    }
    let mut revised = crate::edit::literal_projection(candidate)?;
    for doc in [&mut expected, &mut revised] {
        doc.as_object_mut()?.remove("nika");
        unrooted(doc);
    }
    (expected == revised).then_some(by)
}

/// Journal compiler-observed substitutions even when the model supplied no gap.
/// An old path named by the change stays pending; literal equality cannot establish
/// whether those words asked to retain it. This never rewrites the candidate.
pub(super) fn record_path_changes(
    intent: &str,
    revision: Option<&(String, String)>,
    candidate: &str,
    gaps: &mut Vec<String>,
) {
    let Some((base, words)) = revision else {
        return;
    };
    for path in stated(intent) {
        if gaps.len() >= KEPT_GAPS {
            break;
        }
        if gaps.iter().any(|gap| names(gap, &path)) {
            continue;
        }
        if let Some(by) = substitution(base, words, candidate, &path) {
            let qualifier = if names(words, &path) {
                "compiler-observed substitution; this recalled path requires your decision"
            } else {
                "compiler-proven substitution under the revision rule"
            };
            gaps.push(format!("`{path}` becomes `{by}` ({qualifier})."));
        }
    }
}

/// A literal path already present in the base, including a retained source or
/// secondary destination mentioned again in the change.
fn contains_path(value: &Value, path: &str) -> bool {
    match value {
        Value::String(text) => same(text, path),
        Value::Array(items) => items.iter().any(|item| contains_path(item, path)),
        Value::Object(map) => map.values().any(|item| contains_path(item, path)),
        _ => false,
    }
}

/// A replacement that duplicates a known write to the new destination is a repair,
/// not a faithful substitution. This bounded check compares literal paths and the
/// same content producer; it makes no claim about arbitrary equivalent programs.
pub(super) fn duplicate_write(
    revision: Option<&(String, String)>,
    candidate: &str,
) -> Option<Diagnostic> {
    let (base, words) = revision?;
    if !says(words, REPLACING) || says(words, ADDING) {
        return None;
    }
    let base = crate::edit::literal_projection(base)?;
    let new: Vec<_> = stated(words)
        .into_iter()
        .filter(|path| !contains_path(&base, path))
        .collect();
    let [new]: [String; 1] = new.try_into().ok()?;
    let candidate = crate::edit::literal_projection(candidate)?;
    let writes = writes(&candidate);
    let prior = writes_of_base(&base);
    for (_, content) in writes.iter().filter(|(path, _)| same(path, &new)) {
        if let Some((old, _)) = writes.iter().find(|(path, other)| {
            !same(path, &new) && prior.iter().any(|p| same(p, path)) && other == content
        }) {
            return Some(Diagnostic {
                kind: "revision",
                message: format!(
                    "REVISION DUPLICATES DESTINATION: the change states a replacement, but the same content is written to both `{old}` and `{new}`. Revise the existing destination and its permit; preserve the other computations and outputs. If both destinations are really needed, ask for that business decision instead of silently adding a write."
                ),
            });
        }
    }
    None
}

fn writes_of_base(doc: &Value) -> Vec<String> {
    writes(doc).into_iter().map(|(path, _)| path).collect()
}

/// Resolve only bare constants and task-local bindings, with a fixed depth bound.
/// Task-output expressions stay as producer identities; no expression is evaluated.
fn bound<'a>(doc: &'a Value, task: &'a Value, value: &'a Value) -> &'a Value {
    let mut value = value;
    for _ in 0..4 {
        let Some(inner) = value
            .as_str()
            .and_then(|s| s.trim().strip_prefix("${{"))
            .and_then(|s| s.strip_suffix("}}"))
        else {
            break;
        };
        let inner = inner.trim();
        let next = inner
            .strip_prefix("const.")
            .and_then(|key| doc.get("const")?.get(key))
            .or_else(|| {
                inner
                    .strip_prefix("with.")
                    .and_then(|key| task.get("with")?.get(key))
            });
        let Some(next) = next else { break };
        value = next;
    }
    value
}

fn writes(doc: &Value) -> Vec<(String, Value)> {
    doc.get("tasks")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|tasks| tasks.values())
        .filter_map(|task| {
            if task.pointer("/invoke/tool")?.as_str()? != "nika:write" {
                return None;
            }
            let path = bound(doc, task, task.pointer("/invoke/args/path")?).as_str()?;
            let content = bound(doc, task, task.pointer("/invoke/args/content")?).clone();
            Some((path.to_owned(), content))
        })
        .collect()
}

/// The paths a text states, as the path law reads them: its sources, then its destinations.
fn stated(text: &str) -> Vec<String> {
    let mut paths = crate::hot::stated_sources(text);
    for path in crate::hot::stated_destinations(text) {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

/// Whether `text` names `path` whole: no letter, digit or path character glued to either end
/// (« data.txt » and « out/a.txt » never name `a.txt`; « `a.txt` », « ./a.txt » and « a.txt. »
/// do).
fn names(text: &str, path: &str) -> bool {
    let path = path.trim_start_matches("./");
    if path.is_empty() {
        return false;
    }
    let glued = |c: Option<char>| {
        c.is_some_and(|c| c.is_alphanumeric() || matches!(c, '/' | '_' | '-' | '~' | '.'))
    };
    text.match_indices(path).any(|(at, _)| {
        let head = text.get(..at).unwrap_or_default();
        let head = head.strip_suffix("./").unwrap_or(head);
        let tail = text.get(at + path.len()..).unwrap_or_default();
        let tail = tail.trim_start_matches(['.', ',', ';', ':', '!', '?']);
        !glued(head.chars().next_back()) && !glued(tail.chars().next())
    })
}

/// One relative path however it is spelled (`./b.txt` is `b.txt`).
fn same(a: &str, b: &str) -> bool {
    a.trim_start_matches("./") == b.trim_start_matches("./")
}

/// Whether the words say one of `cues`, whole words compared folded.
fn says(words: &str, cues: &[&str]) -> bool {
    let folded = crate::hot::fold(words);
    let padded = format!(
        " {} ",
        folded
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    );
    cues.iter().any(|cue| padded.contains(&format!(" {cue} ")))
}

/// Every string value equal to `from` becomes `to`; whether one did.
fn substitute(value: &mut Value, from: &str, to: &str) -> bool {
    match value {
        Value::String(text) if same(text, from) => {
            to.clone_into(text);
            true
        }
        Value::Array(items) => {
            let mut any = false;
            for item in items {
                any |= substitute(item, from, to);
            }
            any
        }
        Value::Object(map) => {
            let mut any = false;
            for item in map.values_mut() {
                any |= substitute(item, from, to);
            }
            any
        }
        _ => false,
    }
}

/// Every string value without its leading `./`: one relative path, one spelling.
fn unrooted(value: &mut Value) {
    match value {
        Value::String(text) => {
            if text.starts_with("./") {
                text.replace_range(..2, "");
            }
        }
        Value::Array(items) => items.iter_mut().for_each(unrooted),
        Value::Object(map) => map.values_mut().for_each(unrooted),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{ADDING, REPLACING, names, says};

    #[test]
    fn a_path_is_named_whole_and_never_as_a_piece_of_another() {
        for (text, path) in [
            ("`a.txt` is superseded by the change", "a.txt"),
            ("écrit dans ./a.txt.", "a.txt"),
            ("a.txt, puis b.txt", "./a.txt"),
            ("(a.txt)", "a.txt"),
        ] {
            assert!(names(text, path), "{text} names {path}");
        }
        for (text, path) in [
            ("data.txt is missing", "a.txt"),
            ("out/a.txt is written", "a.txt"),
            ("a.txt.bak stays", "a.txt"),
            ("a.txt_old", "a.txt"),
            ("", "a.txt"),
            ("a.txt", ""),
        ] {
            assert!(!names(text, path), "{text} does not name {path:?}");
        }
    }

    #[test]
    fn a_replacement_is_said_as_one_and_an_addition_never_is() {
        for words in [
            "Finalement, utilise b.txt.",
            "Écris plutôt dans ./out/c.md",
            "Use ./out/second.txt instead.",
            "Mets le résultat dans b.txt à la place",
        ] {
            assert!(says(words, REPLACING) && !says(words, ADDING), "{words}");
        }
        for words in [
            "Écris aussi dans b.txt.",
            "Also use b.txt",
            "Ajoute b.txt en plus",
        ] {
            assert!(says(words, ADDING), "{words}");
        }
        for words in ["Et dans b.txt.", "Ajoute une ligne vide", "b.txt"] {
            assert!(!says(words, REPLACING), "{words}");
        }
        assert!(!says("because the tool adds", REPLACING) && !says("tool address", ADDING));
    }
}
