// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `&str → Project`, and nothing else. This file NEVER opens a file:
//! the edge reads the bytes ([`crate::project::discover`]), this layer
//! reads the text — the house two-stage law.
//!
//! A CLOSED grammar walked by hand over the marked nodes (the
//! `nika_schema::parser` idiom): every key checked against its set,
//! every refusal named with its line, duplicate keys loud
//! (`error_on_duplicate_keys`), quoted scalars non-coercing
//! (`prevent_coercion` — `ceiling: "0.50"` is a string and refuses,
//! it does not silently become a number).

use marked_yaml::types::{MarkedMappingNode, MarkedScalarNode};
use marked_yaml::{LoadError, LoaderOptions, Node, Span as YamlSpan, parse_yaml_with_options};

use super::{
    ArmEntry, ArmLocus, MissPolicy, Project, ProjectError, ProjectErrorKind, ProvenanceFloor,
    RegistryPolicy, TOP_LEVEL_KEYS, TracesPolicy, is_kebab_id,
};
use std::time::Duration;

/// The closed `traces:` key set.
pub(crate) const TRACES_KEYS: &[&str] = &["keep"];
/// The closed `registry:` key set.
pub(crate) const REGISTRY_KEYS: &[&str] = &["floor"];
/// The closed `arm:` entry key set — the registry's THIRTEEN keys
/// (the cadence arc's `Beat` defines them; this gate accepts them all
/// and judges only the five that are its own).
pub(crate) const ARM_ENTRY_KEYS: &[&str] = &[
    "workflow",
    "cadence",
    "où",
    "plafond",
    "manqué",
    "chevauchement",
    "après_saut",
    "actif",
    "raison",
    "jusqu_au",
    "tolérance",
    "décalage",
    "par",
];

/// Parse the project file text (syntax + shape laws — no I/O, no
/// cross-reference). An EMPTY (or whitespace-only) text is an empty
/// file: pure defaults, the same as absent — the optionality law.
///
/// # Errors
/// A [`ProjectError`] naming the law and its line: malformed YAML ·
/// the frozen tag · an unknown key anywhere · a value outside its
/// shape law.
pub fn parse(text: &str) -> Result<Project, ProjectError> {
    if text.trim().is_empty() {
        return Ok(Project::default());
    }
    let options = LoaderOptions::default()
        .error_on_duplicate_keys(true)
        .prevent_coercion(true);
    let node = parse_yaml_with_options(0, text, options).map_err(|e| grammar(&e))?;
    let Node::Mapping(mapping) = &node else {
        // A document that is `~`/null declares nothing (a comments-only
        // file lands here) — the same defaults as an empty one.
        if is_null_document(&node) {
            return Ok(Project::default());
        }
        return Err(ProjectError::at(
            ProjectErrorKind::Grammar,
            "the top level must be a mapping (`nika: <name>` opens the file)",
            "open with `nika: <name>` — the project's kebab-case name, then the keys you govern",
            line_of(node.span()),
        ));
    };
    // A comments-only (or `{}`) file loads as an EMPTY mapping — it
    // declares nothing, so it governs nothing: the tag law binds
    // files that carry keys, not files that carry none.
    if mapping.is_empty() {
        return Ok(Project::default());
    }

    let mut project = Project::default();
    let mut arm = Vec::new();
    let mut seen_tag = false;
    for (key, value) in mapping.iter() {
        let name = key.as_str();
        match name {
            "nika" => {
                seen_tag = true;
                project.name = Some(parse_name(value)?);
            }
            "ceiling" => project.ceiling = Some(usd(value, "ceiling")?),
            "traces" => project.traces = Some(parse_traces(value)?),
            "registry" => project.registry = Some(parse_registry(value)?),
            "arm" => arm = parse_arm(value)?,
            _ => return Err(unknown_key(name, TOP_LEVEL_KEYS, key.span())),
        }
    }
    if !seen_tag {
        return Err(ProjectError::at(
            ProjectErrorKind::Identity,
            "`nika:` absent — a file that governs keys must name itself",
            "open with `nika: <name>` (kebab-case · the same grammar a workflow's `nika:` uses)",
            Some(1),
        ));
    }
    project.arm = arm;
    Ok(project)
}

/// The `nika:` value — the project's NAME, kebab-case.
///
/// It held the literal `v1` until this file learned the workflow
/// envelope's own lesson: a field with ONE legal value is not a version.
/// The slot now carries the project's most necessary field instead, in
/// the SAME grammar the workflow uses, so `nika:` means one thing across
/// both artifact classes. The version marker moved to the `$schema` URL,
/// where an editor already reads it and a fetch can resolve it.
fn parse_name(value: &Node) -> Result<String, ProjectError> {
    let name = scalar(value, "nika")?.as_str();
    // The reinterpretation guard, and it belongs HERE and not in the
    // workflow envelope. A pre-nuke WORKFLOW always carried a
    // `workflow:` block beside its `nika: v1`, so the dead form refuses
    // on that key and `v1` is free to be an ordinary workflow name. A
    // pre-nuke PROJECT file had no such companion: `nika: v1` +
    // `ceiling:` + `arm:` is well-formed under BOTH readings, so the
    // same bytes would silently stop meaning « schema v1 » and start
    // meaning « a project named v1 ». Nothing else can catch that, so a
    // retired marker refuses here — a reserved word must refuse, never
    // glide. `vault` · `v2ray` · `v1-migration` are ordinary names: the
    // rule is the WHOLE marker, never a prefix.
    if name
        .strip_prefix('v')
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(ProjectError::at(
            ProjectErrorKind::Identity,
            format!("`nika: {name}` — that is the retired schema tag, not a project name"),
            "the tag became the project's NAME (`nika: my-project`); the version moved to the \
             `$schema` line, where an editor reads it and a fetch resolves it",
            line_of(value.span()),
        ));
    }
    if !is_kebab_id(name) {
        return Err(ProjectError::at(
            ProjectErrorKind::Identity,
            format!("`nika: {name}` — a project name is kebab-case (`^[a-z][a-z0-9-]*$`)"),
            "name the project (`nika: my-project`) — the version lives on the `$schema` \
             line, never in the body",
            line_of(value.span()),
        ));
    }
    Ok(name.to_owned())
}

/// `traces:` — a mapping over the closed set; `keep` is REQUIRED
/// (a present block that governs nothing is a half-written edit —
/// said, never tolerated silently).
fn parse_traces(value: &Node) -> Result<TracesPolicy, ProjectError> {
    let mapping = mapping_of(value, "traces")?;
    let mut keep = None;
    for (key, value) in mapping.iter() {
        let name = key.as_str();
        match name {
            "keep" => keep = Some(parse_keep(value)?),
            _ => return Err(unknown_key(name, TRACES_KEYS, key.span())),
        }
    }
    let Some(keep) = keep else {
        return Err(ProjectError::at(
            ProjectErrorKind::BadValue,
            "`traces:` carries no `keep` — the one word the block speaks",
            "traces:\n  keep: 30d — the retention age cap, days-grained",
            line_of(value.span()),
        ));
    };
    Ok(TracesPolicy { keep })
}

/// `keep:` — `<N>d`, days-grained (the env var family speaks days;
/// the file speaks the same unit so the ladder never converts).
fn parse_keep(value: &Node) -> Result<Duration, ProjectError> {
    let raw = scalar(value, "keep")?.as_str();
    let bad = || {
        ProjectError::at(
            ProjectErrorKind::BadValue,
            format!("`keep: {raw}` — the form is `<N>d` (days)"),
            "keep: 30d — the same day-grain NIKA_TRACE_MAX_AGE_DAYS speaks",
            line_of(value.span()),
        )
    };
    let digits = raw.strip_suffix('d').ok_or_else(bad)?;
    let days: u64 = digits.parse().map_err(|_| bad())?;
    let secs = days.checked_mul(86_400).ok_or_else(bad)?;
    Ok(Duration::from_secs(secs))
}

/// `registry:` — a mapping over the closed set; `floor` is REQUIRED
/// (same half-written-edit law as `traces:`).
fn parse_registry(value: &Node) -> Result<RegistryPolicy, ProjectError> {
    let mapping = mapping_of(value, "registry")?;
    let mut floor = None;
    for (key, value) in mapping.iter() {
        let name = key.as_str();
        match name {
            "floor" => floor = Some(parse_floor(value)?),
            _ => return Err(unknown_key(name, REGISTRY_KEYS, key.span())),
        }
    }
    let Some(floor) = floor else {
        return Err(ProjectError::at(
            ProjectErrorKind::BadValue,
            "`registry:` carries no `floor` — the one word the block speaks",
            "registry:\n  floor: provenanced — refuse artifacts below the tier",
            line_of(value.span()),
        ));
    };
    Ok(RegistryPolicy { floor })
}

/// `floor:` — the closed tier ladder, lowercase.
fn parse_floor(value: &Node) -> Result<ProvenanceFloor, ProjectError> {
    let raw = scalar(value, "floor")?.as_str();
    ProvenanceFloor::parse(raw).ok_or_else(|| {
        ProjectError::at(
            ProjectErrorKind::BadValue,
            format!("`floor: {raw}` — unknown tier"),
            "the closed ladder: unprovenanced < provenanced < stage-clear < verified",
            line_of(value.span()),
        )
    })
}

/// `arm:` — a sequence of entries over the closed key set. Validated
/// for SHAPE only: the cadence expression law, the workflow's
/// existence, and the scheduling itself all belong to the consuming
/// arc — never here.
fn parse_arm(value: &Node) -> Result<Vec<ArmEntry>, ProjectError> {
    let Node::Sequence(seq) = value else {
        return Err(wrong_shape("arm", "a sequence of entries", value));
    };
    seq.iter().map(parse_arm_entry).collect()
}

/// One `arm:` entry. `plafond` and `manqué` are REQUIRED (the pay law
/// · the run-missed law — a default for either is a silent spend or a
/// silent loss); the parser refuses their absence by name. The cadence
/// arc's eight further keys are read WITHOUT their values judged
/// (verbatim — only `actif` is shape-judged, a bool); a value outside
/// the cadence law passes here and is refused THERE.
fn parse_arm_entry(node: &Node) -> Result<ArmEntry, ProjectError> {
    let Node::Mapping(mapping) = node else {
        return Err(wrong_shape("an arm entry", "a mapping", node));
    };
    let mut workflow = None;
    let mut cadence = None;
    let mut ou = None;
    let mut plafond = None;
    let mut manque = None;
    let mut chevauchement = None;
    let mut apres_saut = None;
    let mut actif = None;
    let mut raison = None;
    let mut jusqu_au = None;
    let mut tolerance = None;
    let mut decalage = None;
    let mut par = None;
    for (key, value) in mapping.iter() {
        let name = key.as_str();
        match name {
            "workflow" => workflow = Some(workflow_path(value)?),
            "cadence" => cadence = Some(cadence_text(value)?),
            "où" => ou = Some(locus(value)?),
            "plafond" => plafond = Some(usd(value, "plafond")?),
            "manqué" => manque = Some(miss_policy(value)?),
            "chevauchement" => chevauchement = Some(verbatim(value, name)?),
            "après_saut" => apres_saut = Some(verbatim(value, name)?),
            "actif" => actif = Some(switch(value)?),
            "raison" => raison = Some(verbatim(value, name)?),
            "jusqu_au" => jusqu_au = Some(verbatim(value, name)?),
            "tolérance" => tolerance = Some(verbatim(value, name)?),
            "décalage" => decalage = Some(verbatim(value, name)?),
            "par" => par = Some(verbatim(value, name)?),
            _ => return Err(unknown_key(name, ARM_ENTRY_KEYS, key.span())),
        }
    }
    let missing = |key: &str, why: &str, remedy: &str| {
        ProjectError::at(
            ProjectErrorKind::BadValue,
            format!("`{key}:` absent — {why}"),
            remedy.to_owned(),
            line_of(node.span()),
        )
    };
    Ok(ArmEntry {
        workflow: workflow.ok_or_else(|| {
            missing(
                "workflow",
                "REQUIRED — what fires",
                "workflow: workflows/mon-beat.nika.yaml",
            )
        })?,
        cadence: cadence.ok_or_else(|| {
            missing(
                "cadence",
                "REQUIRED — when it fires",
                "cadence: \"dimanche 18h07\" — the expression law is the cadence arc's",
            )
        })?,
        ou,
        plafond: plafond.ok_or_else(|| {
            missing(
                "plafond",
                "REQUIRED, no default — choosing for you is choosing who pays",
                "plafond: 2.00 — the per-tick ceiling · it refuses BEFORE spending",
            )
        })?,
        manque: manque.ok_or_else(|| {
            missing(
                "manqué",
                "REQUIRED, no default — « missed » must mean what the operator said",
                "manqué: rattraper · rattraper-une-fois · sauter",
            )
        })?,
        chevauchement,
        apres_saut,
        actif,
        raison,
        jusqu_au,
        tolerance,
        decalage,
        par,
    })
}

/// `workflow:` — a non-empty `*.nika.yaml` path (shape only; the
/// file's existence is the consuming edge's call — zero I/O here).
fn workflow_path(value: &Node) -> Result<String, ProjectError> {
    let raw = scalar(value, "workflow")?.as_str();
    if raw.is_empty() || !raw.ends_with(".nika.yaml") {
        return Err(ProjectError::at(
            ProjectErrorKind::BadValue,
            format!("`workflow: {raw}` — a `*.nika.yaml` path relative to the registry"),
            "workflow: workflows/mon-beat.nika.yaml",
            line_of(value.span()),
        ));
    }
    Ok(raw.to_owned())
}

/// `cadence:` — a non-empty string, verbatim (the expression LAW —
/// readable phrase · cron · `TZ=` — belongs to the cadence arc).
fn cadence_text(value: &Node) -> Result<String, ProjectError> {
    let raw = scalar(value, "cadence")?.as_str().trim();
    if raw.is_empty() {
        return Err(ProjectError::at(
            ProjectErrorKind::BadValue,
            "`cadence:` empty — when does the beat fire?",
            "cadence: \"dimanche 18h07\" — or the cron form with its TZ= prefix",
            line_of(value.span()),
        ));
    }
    Ok(raw.to_owned())
}

/// `où:` — `local | cloud`.
fn locus(value: &Node) -> Result<ArmLocus, ProjectError> {
    let raw = scalar(value, "où")?.as_str();
    ArmLocus::parse(raw).ok_or_else(|| {
        ProjectError::at(
            ProjectErrorKind::BadValue,
            format!("`où: {raw}` — unknown locus"),
            "où: local · cloud — moving is a one-word diff",
            line_of(value.span()),
        )
    })
}

/// `manqué:` — the closed missed-run set.
fn miss_policy(value: &Node) -> Result<MissPolicy, ProjectError> {
    let raw = scalar(value, "manqué")?.as_str();
    MissPolicy::parse(raw).ok_or_else(|| {
        ProjectError::at(
            ProjectErrorKind::BadValue,
            format!("`manqué: {raw}` — unknown missed-run policy"),
            "manqué: rattraper · rattraper-une-fois · sauter",
            line_of(value.span()),
        )
    })
}

/// One of the cadence arc's own keys — a scalar, stored VERBATIM. The
/// value's LAW (the closed enums · the ISO date · the `m/k` form) is
/// the cadence arc's: deux parseurs, jamais en désaccord — each judges
/// its own plane, and this plane is the SHAPE.
fn verbatim(value: &Node, key: &str) -> Result<String, ProjectError> {
    Ok(scalar(value, key)?.as_str().to_owned())
}

/// `actif:` — a bool, the one judged shape among the cadence arc's
/// eight. A quoted `"false"` refuses, the same non-coercion law as
/// `ceiling:` (a string that silently became a switch is the lie class).
fn switch(value: &Node) -> Result<bool, ProjectError> {
    scalar(value, "actif")?.as_bool().ok_or_else(|| {
        ProjectError::at(
            ProjectErrorKind::BadValue,
            "`actif:` must be a bool (`true` · `false`, unquoted)",
            "actif: false — the suspension itself is told by `raison:` + `jusqu_au:` (the cadence arc's law)",
            line_of(value.span()),
        )
    })
}

/// A USD amount (`ceiling:` · `plafond:`) — a real, positive, finite
/// number. A QUOTED number refuses (`prevent_coercion` — `"0.50"` is a
/// string, and a string that silently became money is the lie class).
fn usd(value: &Node, key: &str) -> Result<f64, ProjectError> {
    let node = scalar(value, key)?;
    let ok = |v: f64| v > 0.0 && v.is_finite();
    match node.as_f64().filter(|v| ok(*v)) {
        Some(v) => Ok(v),
        None => Err(ProjectError::at(
            ProjectErrorKind::BadValue,
            format!(
                "`{key}: {}` — a ceiling bounds at the positive real",
                node.as_str()
            ),
            format!("{key}: 0.50 — an unquoted, positive number of dollars"),
            line_of(value.span()),
        )),
    }
}

/// The node must be a scalar — named by its key.
fn scalar<'a>(value: &'a Node, key: &str) -> Result<&'a MarkedScalarNode, ProjectError> {
    value.as_scalar().ok_or_else(|| {
        ProjectError::at(
            ProjectErrorKind::BadValue,
            format!("`{key}:` must be a scalar"),
            "a single value — the file speaks scalars here, never blocks",
            line_of(value.span()),
        )
    })
}

/// The node must be a mapping — named by its key.
fn mapping_of<'a>(value: &'a Node, key: &str) -> Result<&'a MarkedMappingNode, ProjectError> {
    value.as_mapping().ok_or_else(|| {
        ProjectError::at(
            ProjectErrorKind::BadValue,
            format!("`{key}:` must be a mapping"),
            "a block of keys — the closed set, one level down",
            line_of(value.span()),
        )
    })
}

/// A node-kind refusal with the expected form named.
fn wrong_shape(what: &str, expected: &str, node: &Node) -> ProjectError {
    ProjectError::at(
        ProjectErrorKind::BadValue,
        format!("`{what}:` must be {expected}"),
        "the closed grammar: an entry is a mapping, the block a sequence",
        line_of(node.span()),
    )
}

/// The unknown-key refusal — NEVER a silent drop (a typo'd knob that
/// no-ops is invisible to the operator; the closed set names itself).
fn unknown_key(key: &str, closed: &[&str], span: &YamlSpan) -> ProjectError {
    ProjectError::at(
        ProjectErrorKind::UnknownKey,
        format!("unknown key `{key}`"),
        format!("the closed set is {}", closed.join(" · ")),
        line_of(span),
    )
}

/// A `~`/null document — the parser's shape for a comments-only file.
fn is_null_document(node: &Node) -> bool {
    node.as_scalar().is_some_and(|s| {
        let v = s.as_str().trim();
        v.is_empty() || v == "~" || v.eq_ignore_ascii_case("null")
    })
}

/// The 1-based line a span starts on, when it carries one.
fn line_of(span: &YamlSpan) -> Option<usize> {
    span.start().map(marked_yaml::Marker::line)
}

/// The loader's refusal → the named grammar error, line attached
/// (the loader's own text rides the detail — it speaks saphyr's
/// precise scanner vocabulary).
fn grammar(err: &LoadError) -> ProjectError {
    let line = match err {
        LoadError::TopLevelMustBeMapping(m)
        | LoadError::TopLevelMustBeSequence(m)
        | LoadError::UnexpectedAnchor(m)
        | LoadError::MappingKeyMustBeScalar(m)
        | LoadError::UnexpectedTag(m)
        | LoadError::ScanError(m, _) => Some(m.line()),
        LoadError::DuplicateKey(inner) => inner.key.span().start().map(marked_yaml::Marker::line),
    };
    ProjectError::at(
        ProjectErrorKind::Grammar,
        format!("the YAML does not parse — {err}"),
        "fix the YAML · the grammar is closed: nika · ceiling · arm · traces · registry",
        line,
    )
}
