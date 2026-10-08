// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a door that holds no project (Serve's `POST /v1/compile`) admits from its caller's own
//! engine so it can prepare as `nika compile` does: the observation of the files the request
//! states (`observed_world`, the host observer's own document: headers, keys, short categorical
//! values, kind counts, never a row) and the text of the files it read (`trial_inputs`), the
//! project a rehearsal room is built from. Admitted only in the shape the observer prints, about
//! paths the request states, within bounds; anything else is refused whole, never trimmed.
//! Admission proves a shape, never a truth: the facts stay data a seat reads.

pub mod bounds;
pub mod input;

use std::collections::BTreeSet;
use std::path::{Component, Path};

use serde_json::{Map, Value};

/// The largest admitted observation, serialized.
pub const OBSERVATION_BYTES: usize = 256 * 1024;
/// The most rows one observation may carry.
pub const OBSERVATION_ROWS: usize = 64;
/// The most bytes of trial inputs, in all: the observed room's copy bound.
pub const TRIAL_BYTES: u64 = 1024 * 1024;
/// The longest categorical value and the most values per column the observer keeps.
const VALUE_CHARS: usize = 32;
const VALUES_PER_COLUMN: usize = 8;
const ROW_KEYS: [&str; 10] = [
    "path",
    "state",
    "complete",
    "kind",
    "columns",
    "common_columns",
    "bytes",
    "peek_sha256",
    "delimiter",
    "values",
];
const STATES: [&str; 6] = [
    "observed",
    "absent",
    "unreadable",
    "outside_project",
    "empty",
    "unknown",
];
const FOLDER_EXTENSIONS: [&str; 5] = ["csv", "tsv", "json", "jsonl", "ndjson"];

/// Why a caller's observation or trial inputs were refused: one stable reason, never an echo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
#[error("{}", self.message())]
pub enum Refusal {
    /// Not JSON, or a literal that repeats an object key at any depth.
    Malformed,
    /// The observation exceeds [`OBSERVATION_BYTES`] or [`OBSERVATION_ROWS`].
    Oversize,
    /// The observation is not in the shape the observer prints.
    Shape,
    /// A row describes a path the request does not state.
    Unstated,
    /// Trial inputs past [`TRIAL_BYTES`].
    TrialOversize,
    /// Trial inputs that are not the text of the files the observation marks `observed`, or a
    /// door that tries no candidate.
    TrialShape,
}

impl Refusal {
    /// The stable machine code a door answers.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Malformed => "malformed_compile_request",
            Self::Oversize | Self::Shape | Self::Unstated => "compile_observation_refused",
            Self::TrialOversize | Self::TrialShape => "compile_trial_inputs_refused",
        }
    }

    /// What a door answers, naming the remedy; never an echo of the document.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Malformed => {
                "observed_world and trial_inputs are JSON objects without a repeated key"
            }
            Self::Oversize => {
                "observed_world exceeds its bound (256 KiB serialized, 64 rows): send the document `nika compile --observe-only` prints, nothing more"
            }
            Self::Shape => {
                "observed_world is not in the shape `nika compile --observe-only` prints (observed rows and their kinds only)"
            }
            Self::Unstated => {
                "observed_world describes a path the request does not state: observe the request you send"
            }
            Self::TrialOversize => {
                "trial_inputs exceed the trial room's copy bound (1 MiB of text in all)"
            }
            Self::TrialShape => {
                "trial_inputs are the text of the files observed_world marks observed ({\"files\": [{\"path\", \"text\"}]}), only beside it, only to a door that lists compileTrialInputs"
            }
        }
    }
}

/// An admitted observation and the trial inputs beside it, when the caller sent them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Observed {
    /// The observation (`observed_world`).
    pub world: Value,
    /// The text of the observed files (`trial_inputs`).
    pub trial: Option<Value>,
}

impl Observed {
    /// Admit the JSON texts `world` and `trial` as the caller's observation of what `text`
    /// states (a creation's intent; a revision's original request and its change).
    ///
    /// # Errors
    /// The first [`Refusal`] the documents meet.
    pub fn admit(text: &str, world: &str, trial: Option<&str>) -> Result<Self, Refusal> {
        let parse = |json: &str| {
            (!repeats_a_key(json))
                .then(|| serde_json::from_str::<Value>(json).ok())
                .flatten()
                .ok_or(Refusal::Malformed)
        };
        let world = parse(world)?;
        let trial = trial.map(parse).transpose()?;
        admit_observation(text, &world)?;
        if let Some(trial) = &trial {
            admit_trial(&world, trial)?;
        }
        Ok(Self { world, trial })
    }
}

/// Admit `world` as the observation of what `intent` states, or refuse it whole.
///
/// # Errors
/// The [`Refusal`] naming the first law the document breaks.
pub fn admit_observation(intent: &str, world: &Value) -> Result<(), Refusal> {
    if serde_json::to_vec(world).map_or(usize::MAX, |bytes| bytes.len()) > OBSERVATION_BYTES {
        return Err(Refusal::Oversize);
    }
    let object = world.as_object().ok_or(Refusal::Shape)?;
    let rows = (object.get("observed").and_then(Value::as_array))
        .filter(|rows| !rows.is_empty())
        .filter(|_| object.keys().all(|key| key == "observed" || key == "kinds"))
        .ok_or(Refusal::Shape)?;
    if rows.len() > OBSERVATION_ROWS {
        return Err(Refusal::Oversize);
    }
    let stated = stated(intent);
    let mut paths = BTreeSet::new();
    for row in rows {
        let path = row_path(row)?;
        if !covers(&stated, path) {
            return Err(Refusal::Unstated);
        }
        if !paths.insert(path) {
            return Err(Refusal::Shape);
        }
    }
    match object.get("kinds") {
        None => Ok(()),
        Some(Value::Object(kinds))
            if kinds
                .iter()
                .all(|(p, k)| paths.contains(p.as_str()) && k.is_object()) =>
        {
            Ok(())
        }
        Some(_) => Err(Refusal::Shape),
    }
}

/// Admit `trial` (`{"files": [{path, text}]}`) beside the admitted observation `world`.
///
/// # Errors
/// [`Refusal::TrialShape`] or [`Refusal::TrialOversize`].
pub fn admit_trial(world: &Value, trial: &Value) -> Result<(), Refusal> {
    let observed: BTreeSet<String> = observed_paths(world).into_iter().collect();
    let files = (trial.as_object().filter(|o| o.len() == 1))
        .and_then(|o| o.get("files")?.as_array())
        .filter(|files| !files.is_empty())
        .ok_or(Refusal::TrialShape)?;
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    for file in files {
        let (Some(path), Some(text)) = (
            file.get("path").and_then(Value::as_str),
            file.get("text").and_then(Value::as_str),
        ) else {
            return Err(Refusal::TrialShape);
        };
        let shaped = file.as_object().is_some_and(|o| o.len() == 2);
        if !shaped || !observed.contains(path) || relative(path).is_none() || !seen.insert(path) {
            return Err(Refusal::TrialShape);
        }
        total = total.saturating_add(text.len() as u64);
        if total > TRIAL_BYTES {
            return Err(Refusal::TrialOversize);
        }
    }
    Ok(())
}

/// The paths a request states, as the observer reads them: its sources, then its destinations.
#[must_use]
pub fn stated(intent: &str) -> Vec<String> {
    let mut stated = nika_compile::stated_sources(intent);
    for path in nika_compile::stated_destinations(intent) {
        if !stated.contains(&path) {
            stated.push(path);
        }
    }
    stated
}

/// The paths of the rows an observation marks `observed`.
#[must_use]
pub fn observed_paths(world: &Value) -> Vec<String> {
    (world["observed"].as_array().into_iter().flatten())
        .filter(|row| row["state"] == "observed")
        .filter_map(|row| row["path"].as_str().map(str::to_owned))
        .collect()
}

/// A stated path as a relative path that stays below its root: no root, prefix or `..`.
#[must_use]
pub fn relative(path: &str) -> Option<&Path> {
    let relative = Path::new(path.strip_prefix("./").unwrap_or(path));
    let normal = (relative.components()).all(|c| matches!(c, Component::Normal(_)));
    (normal && relative.components().next().is_some()).then_some(relative)
}

/// Whether a JSON literal repeats an object key at any depth (the parser's own recursion ceiling
/// bounds the walk), or is no JSON at all.
#[must_use]
pub fn repeats_a_key(literal: &str) -> bool {
    serde_json::from_str::<Unique>(literal).is_err()
}

/// A row's path, once every one of its fields is of the kind the observer writes.
fn row_path(row: &Value) -> Result<&str, Refusal> {
    let row = row.as_object().ok_or(Refusal::Shape)?;
    let (Some(path), Some(state)) = (
        row.get("path").and_then(Value::as_str),
        row.get("state").and_then(Value::as_str),
    ) else {
        return Err(Refusal::Shape);
    };
    let names = |v: &Value| (v.as_array()).is_some_and(|n| n.iter().all(Value::is_string));
    let sha = |v: &Value| {
        v.as_str()
            .is_some_and(|h| h.len() == 64 && h.bytes().all(hex))
    };
    let fields = row.keys().all(|key| ROW_KEYS.contains(&key.as_str()))
        && STATES.contains(&state)
        && row.get("complete").is_none_or(Value::is_boolean)
        && (row.get("kind")).is_none_or(|k| matches!(k.as_str(), Some("csv" | "json" | "jsonl")))
        && row.get("columns").is_none_or(names)
        && row.get("common_columns").is_none_or(names)
        && row.get("bytes").is_none_or(Value::is_u64)
        && row.get("peek_sha256").is_none_or(sha)
        && (row.get("delimiter"))
            .is_none_or(|d| d.as_str().is_some_and(|d| d.chars().count() == 1))
        && row.get("values").is_none_or(categorical);
    fields.then_some(path).ok_or(Refusal::Shape)
}

const fn hex(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
}

/// Short categorical values per column: a few short scalars, never a row.
fn categorical(value: &Value) -> bool {
    let short = |v: &Value| match v {
        Value::String(text) => text.chars().count() <= VALUE_CHARS,
        Value::Number(_) | Value::Bool(_) => true,
        _ => false,
    };
    value
        .as_object()
        .is_some_and(|columns: &Map<String, Value>| {
            columns.values().all(|values| {
                (values.as_array())
                    .is_some_and(|vs| vs.len() <= VALUES_PER_COLUMN && vs.iter().all(short))
            })
        })
}

/// Whether `path` is a stated path, or a tabular/JSON file directly inside a stated folder.
fn covers(stated: &[String], path: &str) -> bool {
    if relative(path).is_none() {
        return false;
    }
    if stated.iter().any(|s| s == path) {
        return true;
    }
    let bare = |p: &str| {
        p.strip_prefix("./")
            .unwrap_or(p)
            .trim_end_matches('/')
            .to_owned()
    };
    let Some((folder, name)) = bare(path)
        .rsplit_once('/')
        .map(|(f, n)| (f.to_owned(), n.to_owned()))
    else {
        return false;
    };
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    path.starts_with("./")
        && extension.is_some_and(|e| FOLDER_EXTENSIONS.contains(&e.as_str()))
        && stated.iter().any(|s| bare(s) == folder)
}

/// A JSON value whose objects never repeat a key.
struct Unique;

impl<'de> serde::Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UniqueVisitor)
    }
}

struct UniqueVisitor;

impl<'de> serde::de::Visitor<'de> for UniqueVisitor {
    type Value = Unique;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON literal without a repeated key")
    }

    fn visit_bool<E>(self, _: bool) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_i64<E>(self, _: i64) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_u64<E>(self, _: u64) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_f64<E>(self, _: f64) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_str<E>(self, _: &str) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_unit<E>(self) -> Result<Unique, E> {
        Ok(Unique)
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut items: A) -> Result<Unique, A::Error> {
        while items.next_element::<Unique>()?.is_some() {}
        Ok(Unique)
    }

    fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Unique, M::Error> {
        let mut seen = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key) {
                return Err(serde::de::Error::custom("repeated key"));
            }
            map.next_value::<Unique>()?;
        }
        Ok(Unique)
    }
}

#[cfg(test)]
mod tests;
