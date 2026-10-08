// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Profile r2 as its producer exports it (`PROFILE-R2.json`, embedded byte for byte and pinned
//! by its sha256): the fifteen kinds in their roles, their closed row schemas, the relations and
//! their attributes, the layout and the closed vocabularies. The door interprets these tables
//! rather than restating them. It refuses to run on an export whose grammar, bounds or layout are
//! not the ones it implements.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde_json::Value;

use super::{
    BOUNDS, DIRECTORIES, MANIFEST_PATH, NOTICE_PATH, PROFILE, RELATIONS_PATH, RELEASE_FORMAT,
};

/// The producer's export of profile r2, byte for byte.
pub const EXPORT: &str = include_str!("../PROFILE-R2.json");

/// The sha256 of [`EXPORT`]: the profile export this door implements.
pub const EXPORT_SHA256: &str = "7e1948e84a108824bcb34ff0fc8e8079d6b0f212e5ab6e2b985d79529132fbb1";

/// The grammars the door implements, as the export states them: the id name, a facet name, a
/// body slug, a knowledge version and a policy id.
const GRAMMARS: [(&str, &str); 5] = [
    ("/id_name", "[A-Za-z0-9][A-Za-z0-9._:/@+-]{0,199}"),
    ("/values/facet_name", "[a-z0-9][a-z0-9_-]{0,63}"),
    ("/layout/body_slug", "[a-z0-9][a-z0-9-]{0,99}"),
    (
        "/manifest/keys/knowledge_version",
        "[a-z0-9][a-z0-9.+-]{0,63}",
    ),
    ("/expected/policy/id", "[a-z0-9][a-z0-9-]{0,63}"),
];

/// The value bounds the door implements: code points in a line, bytes in a text, items in a
/// list, the largest count.
const VALUES: [(&str, u64); 4] = [
    ("line_code_points", 300),
    ("text_bytes", 65_536),
    ("list_items", 4_096),
    ("count_max", 9_007_199_254_740_991),
];

/// A field's type in a closed schema (the export's `t`, or a scalar's name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Ty {
    Line,
    Text,
    MaybeText,
    Hex64,
    SpecSha,
    Count,
    Number,
    Bool,
    Json,
    Nullable(Box<Ty>),
    Enum(Vec<String>),
    /// A body path of this kind.
    Body(String),
    /// An id of one of these kinds.
    Id(Vec<String>),
    Obj(Obj),
    List(Box<Ty>, usize, usize),
    Lines(usize, usize),
    Ids(Vec<String>, usize, usize),
    /// Facet names to values of this type, at most this many.
    Map(Box<Ty>, usize),
}

/// A closed object: its fields in order, and those it may leave out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Obj {
    pub(super) fields: Vec<(String, Ty)>,
    pub(super) optional: Vec<String>,
}

/// One kind of profile r2, in its role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kind {
    name: String,
    file: String,
    prefix: String,
    role: String,
    proof_levels: Vec<String>,
    verdicts: Vec<String>,
    body: Option<(String, String)>,
    executable: bool,
    row: Ty,
}

impl Kind {
    /// The kind's name (`block`, `counterexample`, `skill`).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its row file (`knowledge/blocks.jsonl`).
    #[must_use]
    pub fn file(&self) -> &str {
        &self.file
    }

    /// Its row file's stem (`blocks`): the name a reader keys the kind's rows by.
    #[must_use]
    pub fn stem(&self) -> &str {
        self.file
            .rsplit('/')
            .next()
            .unwrap_or(&self.file)
            .trim_end_matches(".jsonl")
    }

    /// Its id prefix (`block`, `src`, `facet`).
    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Its role: `component` (the one executable kind), `boundary` (a counterexample, never a
    /// component), `case`, `method`, `contract`, `structure`, `reference`, `repair`, `need`,
    /// `vocabulary` or `provenance`.
    #[must_use]
    pub fn role(&self) -> &str {
        &self.role
    }

    /// The directory of its body files, when its text is a body file.
    #[must_use]
    pub fn body_directory(&self) -> Option<&str> {
        self.body.as_ref().map(|(directory, _)| directory.as_str())
    }

    /// Whether its rows are executable components.
    #[must_use]
    pub const fn executable(&self) -> bool {
        self.executable
    }

    pub(super) fn proof_levels(&self) -> &[String] {
        &self.proof_levels
    }

    pub(super) fn verdicts(&self) -> &[String] {
        &self.verdicts
    }

    /// Its closed row schema.
    pub(super) const fn row(&self) -> &Ty {
        &self.row
    }

    /// Whether `path` is one of its body files: its directory, a slug, its extension.
    pub(super) fn holds_body(&self, path: &str) -> bool {
        self.body.as_ref().is_some_and(|(directory, extension)| {
            path.strip_prefix(directory.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
                .and_then(|rest| rest.strip_suffix(extension.as_str()))
                .is_some_and(body_slug)
        })
    }
}

/// Profile r2's tables, as the door reads them.
#[derive(Debug)]
pub struct Profile {
    kinds: Vec<Kind>,
    pub(super) relations: BTreeMap<String, (Vec<String>, Vec<String>)>,
    pub(super) attrs: Ty,
    pub(super) splits: Vec<String>,
    pub(super) exposures: Vec<String>,
    pub(super) exclusion_reasons: Vec<String>,
    pub(super) downgrade_reasons: Vec<String>,
    pub(super) licences: Vec<String>,
    pub(super) required: Vec<String>,
}

impl Profile {
    /// The fifteen kinds in profile order (the order of admission).
    #[must_use]
    pub fn kinds(&self) -> &[Kind] {
        &self.kinds
    }

    /// The kind whose id prefix is `prefix`.
    #[must_use]
    pub fn kind_of_prefix(&self, prefix: &str) -> Option<&Kind> {
        self.kinds.iter().find(|kind| kind.prefix == prefix)
    }

    /// The kind an id names by its prefix (`block:x` → `block`).
    #[must_use]
    pub fn kind_of_id(&self, id: &str) -> Option<&Kind> {
        id.split_once(':')
            .and_then(|(prefix, _)| self.kind_of_prefix(prefix))
    }

    /// The kind whose body file `path` is, by its directory, slug and extension.
    pub(super) fn body_kind(&self, path: &str) -> Option<&Kind> {
        self.kinds.iter().find(|kind| kind.holds_body(path))
    }

    /// A pinned path the layout admits: a required file, a body file, an admitted licence text.
    pub(super) fn in_layout(&self, path: &str) -> bool {
        self.required.iter().any(|required| required == path)
            || self.body_kind(path).is_some()
            || self.licence_of(path).is_some()
    }

    /// The admitted licence a `LICENSES/<licence>.txt` path names.
    pub(super) fn licence_of<'p>(&self, path: &'p str) -> Option<&'p str> {
        path.strip_prefix("LICENSES/")
            .and_then(|rest| rest.strip_suffix(".txt"))
            .filter(|licence| self.licences.iter().any(|known| known == licence))
    }
}

/// Profile r2, read once from [`EXPORT`]. `Err` names what of the export this door does not
/// implement: the door then refuses every r2 payload.
///
/// # Errors
/// The export is not the profile this door implements.
pub fn profile() -> Result<&'static Profile, &'static str> {
    static PROFILE_R2: OnceLock<Result<Profile, String>> = OnceLock::new();
    PROFILE_R2
        .get_or_init(|| read_export(EXPORT))
        .as_ref()
        .map_err(String::as_str)
}

/// An export read and judged against what the door implements.
pub(super) fn read_export(export: &str) -> Result<Profile, String> {
    let doc: Value = serde_json::from_str(export).map_err(|error| error.to_string())?;
    let expect = |pointer: &str, wanted: &Value| {
        if doc.pointer(pointer) == Some(wanted) {
            Ok(())
        } else {
            Err(format!(
                "the export's {pointer} is not the one this door implements"
            ))
        }
    };
    expect("/id", &Value::from(PROFILE))?;
    expect("/manifest/format", &Value::from(RELEASE_FORMAT))?;
    expect("/manifest/path", &Value::from(MANIFEST_PATH))?;
    expect("/relations/file", &Value::from(RELATIONS_PATH))?;
    expect(
        "/relations/keys",
        &serde_json::json!(["attrs", "from", "rel", "to"]),
    )?;
    expect("/layout/root_files", &serde_json::json!([NOTICE_PATH]))?;
    expect("/layout/directories", &serde_json::json!(DIRECTORIES))?;
    expect("/excluded_kinds", &serde_json::json!([]))?;
    for (pointer, grammar) in GRAMMARS {
        expect(pointer, &Value::from(grammar))?;
    }
    for (name, value) in VALUES {
        expect(&format!("/values/{name}"), &Value::from(value))?;
    }
    for (name, value) in BOUNDS {
        expect(&format!("/bounds/{name}"), &Value::from(value))?;
    }
    let kinds = doc["kinds"]
        .as_object()
        .ok_or("no kinds")?
        .iter()
        .map(|(name, kind)| read_kind(name, kind))
        .collect::<Result<Vec<_>, _>>()?;
    let relations = doc["relations"]["domain_range"]
        .as_object()
        .ok_or("no relations")?
        .iter()
        .map(|(rel, ends)| Ok((rel.clone(), (strings(&ends[0])?, strings(&ends[1])?))))
        .collect::<Result<_, String>>()?;
    let licences = doc["licences"]
        .as_object()
        .ok_or("no licences")?
        .keys()
        .cloned()
        .collect();
    Ok(Profile {
        kinds,
        relations,
        attrs: Ty::Obj(obj(&doc["relations"]["attrs"])?),
        splits: strings(&doc["splits"])?,
        exposures: strings(&doc["exposures"])?,
        exclusion_reasons: strings(&doc["exclusion_reasons"])?,
        downgrade_reasons: strings(&doc["downgrade_reasons"])?,
        licences,
        required: strings(&doc["layout"]["required"])?,
    })
}

fn read_kind(name: &str, kind: &Value) -> Result<Kind, String> {
    let body = match &kind["body"] {
        Value::Null => None,
        body => Some((text(&body["directory"])?, text(&body["extension"])?)),
    };
    Ok(Kind {
        name: name.to_owned(),
        file: text(&kind["file"])?,
        prefix: text(&kind["prefix"])?,
        role: text(&kind["role"])?,
        proof_levels: strings(&kind["proof_levels"])?,
        verdicts: strings(&kind["verdicts"])?,
        body,
        executable: kind["executable"].as_bool().ok_or("no executable flag")?,
        row: Ty::Obj(obj(&kind["row"])?),
    })
}

/// A type as the export writes it: a scalar's name, or an object naming its `t`.
fn ty(spec: &Value) -> Result<Ty, String> {
    if let Some(name) = spec.as_str() {
        return Ok(match name {
            "line" => Ty::Line,
            "text" => Ty::Text,
            "maybe-text" => Ty::MaybeText,
            "hex64" => Ty::Hex64,
            "spec" => Ty::SpecSha,
            "count" => Ty::Count,
            "number" => Ty::Number,
            "bool" => Ty::Bool,
            "json" => Ty::Json,
            other => return Err(format!("an unknown scalar {other}")),
        });
    }
    let size = |key: &str| {
        spec[key]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| format!("no {key}"))
    };
    Ok(match spec["t"].as_str() {
        Some("nullable") => Ty::Nullable(Box::new(ty(&spec["of"])?)),
        Some("enum") => Ty::Enum(strings(&spec["values"])?),
        Some("body") => Ty::Body(text(&spec["kind"])?),
        Some("id") => Ty::Id(strings(&spec["kinds"])?),
        Some("obj") => Ty::Obj(obj(spec)?),
        Some("list") => Ty::List(
            Box::new(ty(&spec["items"])?),
            spec.get("min").map_or(Ok(0), |_| size("min"))?,
            size("max")?,
        ),
        Some("lines") => Ty::Lines(size("min")?, size("max")?),
        Some("ids") => Ty::Ids(strings(&spec["kinds"])?, size("min")?, size("max")?),
        Some("map") if spec["key"] == GRAMMARS[1].1 => {
            Ty::Map(Box::new(ty(&spec["values"])?), size("max")?)
        }
        other => return Err(format!("an unknown type {other:?}")),
    })
}

fn obj(spec: &Value) -> Result<Obj, String> {
    if spec["t"] != "obj" {
        return Err("not an object type".to_owned());
    }
    let fields = spec["fields"]
        .as_array()
        .ok_or("no fields")?
        .iter()
        .map(|field| Ok((text(&field[0])?, ty(&field[1])?)))
        .collect::<Result<_, String>>()?;
    Ok(Obj {
        fields,
        optional: strings(&spec["optional"])?,
    })
}

fn text(value: &Value) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{value} is not a string"))
}

fn strings(value: &Value) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{value} is not a list"))?
        .iter()
        .map(text)
        .collect()
}

/// A body slug: `[a-z0-9][a-z0-9-]{0,99}`.
pub(super) fn body_slug(slug: &str) -> bool {
    slug.chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && slug.len() <= 100
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}
