// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! YAML → [`RawWorkflow`] parser — the canonical v1 language.
//!
//! Parses the spec envelope (`nika-spec/spec/01-envelope.md`) ·
//! `nika:` · `workflow:` · `description:` · `model:` · `vars:` · `env:`
//! · `secrets:` · `tasks:` · `outputs:` — and the closed task shape of
//! `03-dag.md` with the 4 verbs of `02-verbs.md`.
//!
//! The parser is **shape-only** · it validates field forms (scalar vs
//! mapping · closed enums · the Go-duration grammar · exactly-one-verb)
//! and rejects unknown fields in [`ParseMode::Strict`]. Cross-reference
//! semantics (cycles · `depends_on` resolution · `${{ }}` namespace
//! resolution · the `when:` boolean-shape rule) are the analyzer's job.
//!
//! Presence of `nika:` / `workflow:` / non-empty `tasks:` is ALSO the
//! analyzer's job (collected errors) — the parser only validates the
//! fields it sees.

mod envelope;
mod tasks;
mod value;
mod verbs;

use marked_yaml::types::MarkedScalarNode;
use marked_yaml::{LoadError, LoaderOptions, Span as YamlSpan, parse_yaml_with_options};

use crate::error::SchemaError;
use crate::raw::RawWorkflow;
use crate::source::{ByteOffset, FileId, Span, Spanned};
use crate::types::SchemaVersion;

/// Forward-compat mode (spec `02-verbs.md` §forward-compat) ·
///
/// - [`Strict`](Self::Strict) — REJECT any unknown field anywhere
///   (top-level · task-level · verb-level). The conformance-test default.
/// - [`Lenient`](Self::Lenient) — ignore unknown fields (production
///   default per the spec · « Warn + ignore » · warnings TODO).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ParseMode {
    /// Reject unknown fields with a clear error (test default).
    #[default]
    Strict,
    /// Ignore unknown fields (production default).
    Lenient,
}

/// The canonical top-level envelope keys (spec `01-envelope.md`).
const TOP_LEVEL_KEYS: &[&str] = &[
    "nika",
    "workflow",
    "description",
    "model",
    "vars",
    "env",
    "secrets",
    "permits",
    "tasks",
    "outputs",
];

/// Parse a YAML string into a [`RawWorkflow`].
///
/// `file_id` labels the source in downstream diagnostics; it is woven
/// into every [`Span`] attached to the returned workflow. `mode` picks
/// the unknown-field policy.
///
/// # Errors
///
/// Returns a [`SchemaError`] when the YAML is malformed, carries
/// duplicate mapping keys, violates a field's shape (closed enum ·
/// Go-duration · exactly-one-verb · id formats), or — in
/// [`ParseMode::Strict`] — carries an unknown field.
pub fn parse(yaml: &str, file_id: FileId, mode: ParseMode) -> Result<RawWorkflow, SchemaError> {
    // Pre-parse resource guards (the untrusted-input bounds — see the
    // security note on `CharToByte::new`). marked-yaml allocates the
    // whole node tree up-front and its block parser recurses per
    // indentation level, so BOTH bounds must precede it.
    check_source_bounds(yaml)?;

    // YAML 1.2 forbids duplicate mapping keys from silently last-winning
    // — `error_on_duplicate_keys` turns them into loud errors (covers
    // vars/env/secrets/outputs/with/output duplicate-key detection).
    // `prevent_coercion` makes QUOTED scalars non-coercing (only plain
    // scalars type-coerce) — the YAML 1.2 contract `"42"` is a string.
    let options = LoaderOptions::default()
        .error_on_duplicate_keys(true)
        .prevent_coercion(true);
    let node =
        parse_yaml_with_options(file_id.0 as usize, yaml, options).map_err(|err| match err {
            LoadError::DuplicateKey(inner) => SchemaError::DuplicateKey {
                message: format!(
                    "\"{}\" appears twice in the same mapping",
                    inner.key.as_str()
                ),
                span: None,
            },
            other => SchemaError::YamlSyntax {
                message: other.to_string(),
                span: None,
            },
        })?;

    // Char-to-byte translation table: marked-yaml reports character
    // (code-point) indices, but `ByteOffset` — and every miette
    // `SourceSpan` downstream — wants byte offsets. Precompute once
    // per parse so each span lookup is O(1).
    let char_to_byte = CharToByte::new(yaml)?;

    let mapping = node.as_mapping().ok_or_else(|| SchemaError::Validation {
        message: "workflow root must be a YAML mapping".to_owned(),
        span: yaml_span_to_span(file_id, node.span(), &char_to_byte),
    })?;

    let cx = Cx {
        file_id,
        char_to_byte: &char_to_byte,
        mode,
    };

    cx.check_unknown_keys(mapping, TOP_LEVEL_KEYS, "the workflow envelope")?;

    let mut workflow = RawWorkflow::new();

    if let Some(scalar) = mapping.get_scalar("nika") {
        workflow.nika = Some(parse_nika_version(&cx, scalar)?);
    }
    if let Some(s) = extract_scalar(mapping, "workflow", file_id, &char_to_byte)? {
        validate_workflow_id(&s)?;
        workflow.workflow = Some(s);
    }
    if let Some(s) = extract_scalar(mapping, "description", file_id, &char_to_byte)? {
        workflow.description = Some(s);
    }
    if let Some(s) = extract_scalar(mapping, "model", file_id, &char_to_byte)? {
        workflow.model = Some(s);
    }

    workflow.vars = envelope::parse_vars(&cx, mapping)?;
    workflow.env = envelope::parse_env(&cx, mapping)?;
    workflow.secrets = envelope::parse_secrets(&cx, mapping)?;
    workflow.permits = envelope::parse_permits(&cx, mapping)?;
    workflow.outputs = envelope::parse_outputs(&cx, mapping)?;
    workflow.tasks = tasks::parse_tasks(&cx, mapping)?;

    Ok(workflow)
}

// ── Shared parse context ────────────────────────────────────────────

/// A spanned key → spanned value entry list, preserving YAML order
/// (`env:` · `secrets:` · `with:` · `output:` blocks).
pub(super) type SpannedEntries<T> = Vec<(Spanned<String>, Spanned<T>)>;

/// Maximum workflow source size — 4 MiB. marked-yaml allocates the
/// whole node tree up-front, so this bounds parse-time memory. A
/// hand-written workflow (even with inline JSON Schemas and long
/// prompts) is kilobytes; 4 MiB is a memory-safety bound, not a policy
/// limit — far above any real file, loud below the `u32::MAX` span
/// ceiling (which stays a separate span-correctness guard).
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// Maximum leading-space indentation accepted per line — ~512 block
/// nesting levels at 2 spaces/level, a 4–6× margin under the empirical
/// marked-yaml stack-overflow point (~3000 levels / 8 MB stack) and
/// generous for real files (block-scalar CONTENT with >1 KB of leading
/// spaces is not a workflow).
const MAX_INDENT_BYTES: usize = 1024;

/// The pre-parse resource guards — byte size + indentation depth, both
/// LOUD and both before marked-yaml allocates/recurses. One O(n) pass
/// over lines (leading spaces only — YAML forbids tabs in indentation).
fn check_source_bounds(source: &str) -> Result<(), SchemaError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(SchemaError::YamlSyntax {
            message: format!(
                "workflow source is {} bytes (max {MAX_SOURCE_BYTES}) — \
                 rejected (memory-safety bound)",
                source.len()
            ),
            span: None,
        });
    }
    for (line_no, line) in source.lines().enumerate() {
        let indent = line.bytes().take_while(|&b| b == b' ').count();
        if indent > MAX_INDENT_BYTES {
            return Err(SchemaError::YamlSyntax {
                message: format!(
                    "line {} is indented {indent} spaces (max {MAX_INDENT_BYTES}) — \
                     nesting this deep is rejected (stack-safety bound)",
                    line_no + 1
                ),
                span: None,
            });
        }
    }
    Ok(())
}

/// Per-parse context threaded through the submodules.
pub(super) struct Cx<'a> {
    pub(super) file_id: FileId,
    pub(super) char_to_byte: &'a CharToByte,
    pub(super) mode: ParseMode,
}

impl Cx<'_> {
    /// Translate a marked-yaml span.
    pub(super) fn span(&self, span: &YamlSpan) -> Option<Span> {
        yaml_span_to_span(self.file_id, span, self.char_to_byte)
    }

    /// Translate a marked-yaml span, falling back to a zero-point span.
    pub(super) fn span_or_zero(&self, span: &YamlSpan) -> Span {
        self.span(span)
            .unwrap_or_else(|| Span::point(self.file_id, ByteOffset::new(0)))
    }

    /// In [`ParseMode::Strict`] · reject any mapping key outside `known`
    /// (spec `02-verbs.md` §forward-compat · strict = test default).
    pub(super) fn check_unknown_keys(
        &self,
        mapping: &marked_yaml::types::MarkedMappingNode,
        known: &[&str],
        location: &str,
    ) -> Result<(), SchemaError> {
        if self.mode == ParseMode::Lenient {
            return Ok(());
        }
        for (key, _) in mapping.iter() {
            if !known.contains(&key.as_str()) {
                return Err(SchemaError::UnknownField {
                    field: key.as_str().to_owned(),
                    location: location.to_owned(),
                    span: self.span(key.span()),
                });
            }
        }
        Ok(())
    }

    /// Reject any mapping key outside `known` in BOTH parse modes.
    ///
    /// Reserved for security-bearing blocks (`permits:`) where a typo'd
    /// capability key silently changing the boundary would be a security
    /// bug — lenient mode does not apply there.
    pub(super) fn check_unknown_keys_always(
        &self,
        mapping: &marked_yaml::types::MarkedMappingNode,
        known: &[&str],
        location: &str,
    ) -> Result<(), SchemaError> {
        for (key, _) in mapping.iter() {
            if !known.contains(&key.as_str()) {
                return Err(SchemaError::UnknownField {
                    field: key.as_str().to_owned(),
                    location: location.to_owned(),
                    span: self.span(key.span()),
                });
            }
        }
        Ok(())
    }

    /// Extract an optional string scalar by key.
    pub(super) fn opt_scalar(
        &self,
        mapping: &marked_yaml::types::MarkedMappingNode,
        key: &str,
    ) -> Result<Option<Spanned<String>>, SchemaError> {
        extract_scalar(mapping, key, self.file_id, self.char_to_byte)
    }

    /// Extract a required string scalar by key.
    pub(super) fn require_scalar(
        &self,
        mapping: &marked_yaml::types::MarkedMappingNode,
        key: &str,
        location: &str,
    ) -> Result<Spanned<String>, SchemaError> {
        self.opt_scalar(mapping, key)?
            .ok_or_else(|| SchemaError::MissingField {
                field: format!("{location}.{key}"),
                span: self.span(mapping.span()),
            })
    }
}

// ── Helpers ─────────────────────────────────────────────────────

/// Precomputed char-index → byte-offset table for the source text.
///
/// `marked-yaml` 0.8 reports positions as character indices; our
/// [`ByteOffset`] and miette's `SourceSpan` want byte offsets. ASCII
/// inputs are the hot path, so the constructor short-circuits.
pub(super) struct CharToByte {
    /// `byte_at[char_idx]` is the UTF-8 byte offset. Contains
    /// `source.len()` as its final sentinel so end-of-file positions
    /// resolve without panicking.
    byte_at: Vec<u32>,
}

impl CharToByte {
    pub(super) fn new(source: &str) -> Result<Self, SchemaError> {
        // Guard against pathological inputs that would overflow u32.
        // 4 GB of YAML is not a workflow, it's a denial-of-service
        // attempt — fail loud rather than silently clamp.
        //
        // SECURITY NOTE (untrusted-input resource bounds · crate-spec §11):
        // this u32::MAX limit is a SPAN-CORRECTNESS bound, distinct from the
        // DoS bounds below. The untrusted-input guard SET is now complete —
        // all checked BEFORE marked-yaml allocates/recurses:
        //   • source byte cap        `MAX_SOURCE_BYTES` (4 MiB · memory)
        //   • indentation depth cap  `MAX_INDENT_BYTES` (block stack-safety)
        //   • YAML value nesting cap  `value::MAX_VALUE_DEPTH` (walker + Drop)
        //   • task-count cap         `tasks::MAX_TASKS` (analyzer DAG passes)
        // (empirical anchor: unbounded block nesting overflowed the stack at
        // ~3000 levels.) marked-yaml 0.8 does not expand anchors/aliases, so
        // the billion-laughs vector is closed and its flow parser self-limits
        // recursion (~150). What REMAINS pre-`nika serve` is policy, not
        // safety: per-tenant quotas + a wall-clock parse timeout.
        if source.len() > u32::MAX as usize {
            return Err(SchemaError::YamlSyntax {
                message: format!(
                    "workflow source exceeds {} bytes — spans would overflow",
                    u32::MAX
                ),
                span: None,
            });
        }
        // Fast path: if every byte is ASCII, char index = byte index.
        if source.is_ascii() {
            return Ok(Self {
                byte_at: Vec::new(),
            });
        }
        let mut byte_at: Vec<u32> = source
            .char_indices()
            .map(|(b, _)| u32::try_from(b).unwrap_or(u32::MAX))
            .collect();
        byte_at.push(u32::try_from(source.len()).unwrap_or(u32::MAX));
        Ok(Self { byte_at })
    }

    /// Translate a character index into a byte offset, clamping to
    /// end-of-file when the char index is out of range (which
    /// marked-yaml may report for synthetic end markers).
    pub(super) fn byte(&self, char_idx: usize) -> u32 {
        if self.byte_at.is_empty() {
            // ASCII fast path — char index IS the byte offset.
            return u32::try_from(char_idx).unwrap_or(u32::MAX);
        }
        let clamped = char_idx.min(self.byte_at.len().saturating_sub(1));
        self.byte_at[clamped]
    }
}

/// Extract an optional top-level string scalar by key.
///
/// Returns `Ok(None)` if the key is absent. Returns a
/// [`SchemaError::Validation`] if the key exists but the value is a
/// mapping or a sequence.
pub(super) fn extract_scalar(
    mapping: &marked_yaml::types::MarkedMappingNode,
    key: &str,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<Option<Spanned<String>>, SchemaError> {
    let Some(node) = mapping.get_node(key) else {
        return Ok(None);
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: format!("`{key}` must be a scalar string"),
            span: yaml_span_to_span(file_id, node.span(), char_to_byte),
        });
    };
    let span = yaml_span_to_span(file_id, scalar.span(), char_to_byte)
        .unwrap_or_else(|| Span::point(file_id, ByteOffset::new(0)));
    Ok(Some(Spanned::new(scalar.as_str().to_owned(), span)))
}

/// Parse the `nika:` scalar — exactly `v1` (spec `01-envelope.md` ·
/// « **Anti-pattern** · do not write `nika: v1.0` · `nika: "1"` · or
/// `nika: 1.0`. The value is exactly `v1`. »).
fn parse_nika_version(
    cx: &Cx<'_>,
    scalar: &MarkedScalarNode,
) -> Result<Spanned<SchemaVersion>, SchemaError> {
    let raw = scalar.as_str();
    let span = cx.span(scalar.span());
    if raw != "v1" {
        return Err(SchemaError::BadNikaVersion {
            version: raw.to_owned(),
            span,
        });
    }
    Ok(Spanned::new(
        SchemaVersion::V1,
        span.unwrap_or_else(|| Span::point(cx.file_id, ByteOffset::new(0))),
    ))
}

/// Validate `workflow:` against `^[a-z][a-z0-9-]*$` (spec
/// `01-envelope.md` · kebab-case · « it is a resource name, never
/// referenced inside an expression »).
fn validate_workflow_id(id: &Spanned<String>) -> Result<(), SchemaError> {
    let s = &id.value;
    let valid = s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid {
        Ok(())
    } else {
        Err(SchemaError::BadWorkflowId {
            id: s.clone(),
            span: Some(id.span),
        })
    }
}

/// Validate a task `id:` against `^[a-z][a-z0-9_]*$` (spec `03-dag.md` ·
/// `snake_case` · CEL-safe · a hyphen would parse as subtraction).
pub(super) fn validate_task_id(id: &Spanned<String>) -> Result<(), SchemaError> {
    let s = &id.value;
    let valid = s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(SchemaError::BadTaskId {
            id: s.clone(),
            span: Some(id.span),
        })
    }
}

/// Convert a `marked_yaml::Span` into our span (attached to `file_id`).
///
/// marked-yaml reports the start marker (and sometimes no end). When
/// both markers are present we produce a `[start, end)` range; when
/// only `start` is available the span is zero-length (point). An
/// entirely blank yaml span yields `None`. Character indices from
/// marked-yaml are translated to byte offsets via `char_to_byte`.
pub(super) fn yaml_span_to_span(
    file_id: FileId,
    span: &YamlSpan,
    char_to_byte: &CharToByte,
) -> Option<Span> {
    let start = span.start()?;
    let start_off = char_to_byte.byte(start.character());
    let end_off = span
        .end()
        .map_or(start_off, |m| char_to_byte.byte(m.character()));
    Some(Span::new(
        file_id,
        ByteOffset::new(start_off),
        ByteOffset::new(end_off),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fid() -> FileId {
        FileId::new(0)
    }

    fn parse_strict(yaml: &str) -> Result<RawWorkflow, SchemaError> {
        parse(yaml, fid(), ParseMode::Strict)
    }

    const MINIMAL: &str = "\
nika: v1
workflow: hello
tasks:
  - id: greet
    infer:
      prompt: \"Say hi\"
";

    #[test]
    fn oversized_source_is_rejected_loud() {
        // a 4 MiB+ document rejects BEFORE marked-yaml allocates the tree
        let big = format!(
            "nika: v1\nworkflow: w\ndescription: \"{}\"\ntasks: []\n",
            "x".repeat(MAX_SOURCE_BYTES)
        );
        let err = parse_strict(&big).expect_err("rejected");
        assert!(err.to_string().contains("memory-safety bound"), "{err}");
    }

    #[test]
    fn too_many_tasks_is_rejected_loud() {
        use std::fmt::Write as _;
        let mut yaml = String::from("nika: v1\nworkflow: w\ntasks:\n");
        for i in 0..=tasks::MAX_TASKS {
            let _ = write!(yaml, "  - id: t{i}\n    exec: {{ command: \"true\" }}\n");
        }
        let err = parse_strict(&yaml).expect_err("rejected");
        assert!(err.to_string().contains("resource bound"), "{err}");
        assert!(err.to_string().contains("compose"), "actionable: {err}");
    }

    #[test]
    fn pathological_block_nesting_is_rejected_not_crashed() {
        // The empirical stack-overflow class (marked-yaml block parse
        // recursed past the 8 MB stack at ~3000 levels · CRASH before
        // this guard): both layers reject LOUD now.
        // Layer 1 · the indent guard (pre-parse · protects marked-yaml).
        let mut deep_block = String::new();
        for i in 0..600 {
            deep_block.push_str(&" ".repeat(2 * i));
            deep_block.push_str("k:\n");
        }
        let yaml = format!("nika: v1\nworkflow: w\nx:\n{deep_block}");
        let err = parse_strict(&yaml).expect_err("rejected");
        assert!(
            err.to_string().contains("stack-safety bound"),
            "loud: {err}"
        );
        // Layer 2 · the value-depth cap (conversion · protects every
        // downstream Value walker) — flow style slips past layer 1.
        let depth = 140; // > MAX_VALUE_DEPTH(128) · < marked-yaml flow limit
        let inner = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
        let yaml = format!(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke: {{ tool: \"nika:log\", args: {{ v: {inner} }} }}\n"
        );
        let err = parse_strict(&yaml).expect_err("rejected");
        assert!(
            err.to_string().contains("nesting exceeds 128"),
            "loud: {err}"
        );
    }

    #[test]
    fn parse_minimal_canonical_envelope() {
        let wf = parse_strict(MINIMAL).expect("parse");
        assert_eq!(wf.nika.as_ref().map(|s| s.value), Some(SchemaVersion::V1));
        assert_eq!(
            wf.workflow.as_ref().map(|s| s.value.as_str()),
            Some("hello")
        );
        assert_eq!(wf.tasks.len(), 1);
    }

    #[test]
    fn parse_all_top_level_scalars() {
        let yaml = "\
nika: v1
workflow: my-workflow
description: A demo
model: anthropic/claude-sonnet-4-6
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.workflow.expect("workflow").value, "my-workflow");
        assert_eq!(wf.description.expect("description").value, "A demo");
        assert_eq!(
            wf.model.expect("model").value,
            "anthropic/claude-sonnet-4-6"
        );
        assert_eq!(wf.nika.expect("nika").value, SchemaVersion::V1);
    }

    #[test]
    fn parse_bad_nika_version_errors() {
        // Spec 01 · « do not write `nika: v1.0` · `nika: "1"` · `1.0` ».
        for bad in ["nika: v1.0\n", "nika: \"1\"\n", "nika: 1.0\n", "nika: v2\n"] {
            let err = parse_strict(bad).expect_err("bad nika version");
            assert!(
                matches!(err, SchemaError::BadNikaVersion { .. }),
                "{bad:?} → {err:?}"
            );
        }
    }

    #[test]
    fn parse_bad_nika_version_carries_span() {
        let err = parse_strict("nika: v999\n").expect_err("bad version");
        let SchemaError::BadNikaVersion { version, span } = err else {
            panic!("expected BadNikaVersion");
        };
        assert_eq!(version, "v999");
        assert!(span.is_some(), "BadNikaVersion must carry its span");
    }

    #[test]
    fn parse_bad_workflow_id_errors() {
        // Spec 01 · « Must match · ^[a-z][a-z0-9-]*$ ».
        for bad in [
            "workflow: Bad_Id\n",
            "workflow: 9lives\n",
            "workflow: my_flow\n",
            "workflow: \"\"\n",
        ] {
            let err = parse_strict(bad).expect_err("bad workflow id");
            assert!(
                matches!(err, SchemaError::BadWorkflowId { .. }),
                "{bad:?} → {err:?}"
            );
        }
    }

    #[test]
    fn parse_good_workflow_ids() {
        for good in ["hello", "scrape-and-summarize", "a", "a1-b2"] {
            let yaml = format!("workflow: {good}\n");
            let wf = parse_strict(&yaml).expect("parse");
            assert_eq!(wf.workflow.expect("workflow").value, good);
        }
    }

    #[test]
    fn strict_rejects_unknown_top_level_key() {
        // Conformance fixture envelope/005-unknown-top-level-key.
        let yaml = "\
nika: v1
workflow: hello
foo: bar
tasks:
  - id: greet
    infer:
      prompt: \"hi\"
";
        let err = parse_strict(yaml).expect_err("unknown key");
        let SchemaError::UnknownField { field, .. } = err else {
            panic!("expected UnknownField, got {err:?}");
        };
        assert_eq!(field, "foo");
    }

    #[test]
    fn lenient_ignores_unknown_top_level_key() {
        let yaml = "\
nika: v1
workflow: hello
foo: bar
tasks:
  - id: greet
    infer:
      prompt: \"hi\"
";
        let wf = parse(yaml, fid(), ParseMode::Lenient).expect("lenient parse");
        assert_eq!(wf.tasks.len(), 1);
    }

    #[test]
    fn duplicate_top_level_keys_error() {
        // YAML 1.2 · duplicate keys never silently last-win.
        let err = parse_strict("workflow: first\nworkflow: second\n").expect_err("dup");
        assert!(matches!(err, SchemaError::DuplicateKey { .. }), "{err:?}");
    }

    #[test]
    fn parse_empty_yaml_yields_empty_workflow() {
        // The PARSER accepts an empty mapping — missing nika/workflow/
        // tasks are the ANALYZER's collected errors.
        let wf = parse_strict("").expect("empty yaml is shape-legal");
        assert!(wf.nika.is_none());
        assert!(wf.tasks.is_empty());
    }

    #[test]
    fn parse_sequence_top_level_errors() {
        let err = parse_strict("- item\n").expect_err("sequence root must fail");
        assert!(matches!(err, SchemaError::YamlSyntax { .. }));
    }

    #[test]
    fn parse_workflow_as_sequence_errors() {
        let err = parse_strict("workflow:\n  - foo\n").expect_err("sequence value");
        assert!(matches!(err, SchemaError::Validation { .. }));
    }

    #[test]
    fn parse_yaml_syntax_error_maps_to_schema_error() {
        let err = parse_strict("workflow: [unclosed\n").expect_err("bad yaml");
        assert!(matches!(err, SchemaError::YamlSyntax { .. }));
    }

    #[test]
    fn parse_file_id_propagates_into_span() {
        let wf = parse("workflow: x\n", FileId::new(42), ParseMode::Strict).expect("parse");
        assert_eq!(wf.workflow.expect("workflow").span.file, FileId::new(42));
    }

    #[test]
    fn parse_error_span_carries_original_file_id() {
        let err = parse("workflow:\n  - foo\n", FileId::new(42), ParseMode::Strict)
            .expect_err("seq value");
        assert!(
            matches!(
                &err,
                SchemaError::Validation {
                    span: Some(span),
                    ..
                } if span.file == FileId::new(42)
            ),
            "expected Validation with span carrying FileId(42), got {err:?}",
        );
    }

    #[test]
    fn parse_non_ascii_value_span_starts_at_correct_byte() {
        // Regression lock for the char-index → byte-offset fix.
        let yaml = "desc_\u{00e9}: skip\nworkflow: hit\n";
        let wf = parse(yaml, fid(), ParseMode::Lenient).expect("parse");
        let spanned = wf.workflow.expect("workflow present");
        let line1_bytes = "desc_\u{00e9}: skip\n".len();
        let expected_start = u32::try_from(line1_bytes + 10).expect("fits"); // "workflow: "
        assert_eq!(
            spanned.span.start,
            ByteOffset::new(expected_start),
            "span start must be byte offset after char→byte translation",
        );
    }
}
