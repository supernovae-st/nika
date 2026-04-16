// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! YAML → [`RawWorkflow`] parser.
//!
//! Round 2d scope:
//!
//! - Top-level workflow scalars (`schema`, `name`, `description`,
//!   `goal`, `provider`, `model`).
//! - `tasks:` sequence — each task parsed for `name` + the action
//!   discriminator + the verb's minimum required field (`prompt` /
//!   `command` / `url` / `tool|resource` / `prompt`). Optional
//!   task-level fields (`depends_on`, `condition`, `for_each`,
//!   `record`, `decompose`, `budget`, `limits`, `completion`,
//!   `guardrails`, `max_retries`) and optional verb-specific
//!   fields land in Round 2e.
//!
//! Still deferred: `mcp`, `include`, `context`, `inputs`, `logging`,
//! `orchestrate`, `routing`, `schedule`, `max_duration_secs`.
//!
//! Fields that are known but not yet parsed are ignored silently so
//! that workflows under incremental development still load. Truly
//! structural failures (bad YAML, wrong root shape, type mismatches
//! on handled scalars, unsupported `schema:` version, missing task
//! `name`, missing/ambiguous action verb) produce a [`SchemaError`]
//! carrying a [`Span`] when one is available.

mod tasks;

use marked_yaml::types::MarkedScalarNode;
use marked_yaml::{Span as YamlSpan, parse_yaml};

use crate::error::SchemaError;
use crate::raw::RawWorkflow;
use crate::source::{ByteOffset, FileId, Span, Spanned};
use crate::types::SchemaVersion;

/// Parse a YAML string into a [`RawWorkflow`].
///
/// `file_id` labels the source in downstream diagnostics; it is
/// woven into every [`Span`] attached to the returned workflow.
///
/// # Errors
///
/// Returns a [`SchemaError`] when the YAML is malformed, when the
/// root node is not a mapping, when a handled field has the wrong
/// shape (for example `name: [foo]`), or when the `schema:` field
/// names an unsupported version.
pub fn parse(yaml: &str, file_id: FileId) -> Result<RawWorkflow, SchemaError> {
    let node = parse_yaml(file_id.0 as usize, yaml).map_err(|err| SchemaError::YamlSyntax {
        message: err.to_string(),
        span: None,
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

    let mut workflow = RawWorkflow::new();

    if let Some(scalar) = mapping.get_scalar("schema") {
        workflow.schema = Some(parse_schema_version(file_id, scalar, &char_to_byte)?);
    }
    if let Some(s) = extract_scalar(mapping, "name", file_id, &char_to_byte)? {
        workflow.name = Some(s);
    }
    if let Some(s) = extract_scalar(mapping, "description", file_id, &char_to_byte)? {
        workflow.description = Some(s);
    }
    if let Some(s) = extract_scalar(mapping, "goal", file_id, &char_to_byte)? {
        workflow.goal = Some(s);
    }
    if let Some(s) = extract_scalar(mapping, "provider", file_id, &char_to_byte)? {
        workflow.provider = Some(s);
    }
    if let Some(s) = extract_scalar(mapping, "model", file_id, &char_to_byte)? {
        workflow.model = Some(s);
    }

    workflow.tasks = tasks::parse_tasks(mapping, file_id, &char_to_byte)?;

    // TODO(round-2e): record which known-but-unhandled keys (`mcp`,
    // `context`, `include`, `inputs`, `logging`, `orchestrate`,
    // `routing`, `schedule`, `max_duration_secs`) were present so
    // the Round 3 analyzer can surface a precise "deferred field"
    // diagnostic instead of treating them as absent.

    Ok(workflow)
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

/// Parse a `schema:` scalar into [`SchemaVersion`].
fn parse_schema_version(
    file_id: FileId,
    scalar: &MarkedScalarNode,
    char_to_byte: &CharToByte,
) -> Result<Spanned<SchemaVersion>, SchemaError> {
    let raw = scalar.as_str();
    let span = yaml_span_to_span(file_id, scalar.span(), char_to_byte);
    let version = match raw {
        "v1" => SchemaVersion::V1,
        other => {
            return Err(SchemaError::UnsupportedVersion {
                version: other.to_owned(),
                span,
            });
        }
    };
    let span = span.unwrap_or_else(|| Span::point(file_id, ByteOffset::new(0)));
    Ok(Spanned::new(version, span))
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

    #[test]
    fn parse_minimal_name_only() {
        let yaml = "name: hello\n";
        let wf = parse(yaml, fid()).expect("parse");
        assert_eq!(wf.name.as_ref().map(|s| s.value.as_str()), Some("hello"));
        assert!(wf.description.is_none());
        assert!(wf.tasks.is_empty());
    }

    #[test]
    fn parse_all_top_level_scalars() {
        let yaml = "\
name: my-workflow
description: A demo
goal: Do the thing
provider: openai
model: gpt-4o
schema: v1
";
        let wf = parse(yaml, fid()).expect("parse");
        assert_eq!(wf.name.unwrap().value, "my-workflow");
        assert_eq!(wf.description.unwrap().value, "A demo");
        assert_eq!(wf.goal.unwrap().value, "Do the thing");
        assert_eq!(wf.provider.unwrap().value, "openai");
        assert_eq!(wf.model.unwrap().value, "gpt-4o");
        assert_eq!(wf.schema.unwrap().value, SchemaVersion::V1);
    }

    #[test]
    fn parse_preserves_span_for_name() {
        let yaml = "name: hello\n";
        let wf = parse(yaml, FileId::new(7)).expect("parse");
        let spanned = wf.name.expect("name present");
        assert_eq!(spanned.span.file, FileId::new(7));
        assert!(spanned.span.end >= spanned.span.start);
    }

    #[test]
    fn parse_empty_yaml_yields_empty_workflow() {
        // marked-yaml treats "" as the empty mapping — an empty
        // workflow is a legal (if useless) schema input, so the
        // parser returns an all-`None` `RawWorkflow` rather than an
        // error.
        let wf = parse("", fid()).expect("empty yaml is legal");
        assert!(wf.name.is_none());
        assert!(wf.tasks.is_empty());
    }

    #[test]
    fn parse_sequence_top_level_errors() {
        // marked-yaml itself rejects non-mapping roots, so the
        // failure surfaces as `YamlSyntax` rather than our own
        // `Validation` variant. That's fine — the error message
        // still points to the root location.
        let err = parse("- item\n", fid()).expect_err("sequence root must fail");
        assert!(matches!(err, SchemaError::YamlSyntax { .. }));
        if let SchemaError::YamlSyntax { message, .. } = err {
            assert!(message.to_lowercase().contains("mapping"));
        }
    }

    #[test]
    fn parse_name_as_sequence_errors() {
        let err = parse("name:\n  - foo\n", fid()).expect_err("sequence value");
        assert!(matches!(err, SchemaError::Validation { .. }));
        if let SchemaError::Validation { message, .. } = err {
            assert!(message.contains("`name`"));
        }
    }

    #[test]
    fn parse_description_as_mapping_errors() {
        let err = parse("description:\n  foo: bar\n", fid()).expect_err("mapping value");
        assert!(matches!(err, SchemaError::Validation { .. }));
    }

    #[test]
    fn parse_unsupported_schema_version_errors() {
        let err = parse("schema: v999\n", fid()).expect_err("bad version");
        assert!(matches!(err, SchemaError::UnsupportedVersion { .. }));
        if let SchemaError::UnsupportedVersion { version, span } = err {
            assert_eq!(version, "v999");
            // Span was preserved on the error path — P1 review fix.
            assert!(span.is_some(), "UnsupportedVersion must carry its span");
        }
    }

    #[test]
    fn parse_non_ascii_value_span_starts_at_correct_byte() {
        // Regression lock for the char-index → byte-offset fix.
        // Without the translation table, non-ASCII keys before the
        // scalar would push the byte start wrong. Put the multi-byte
        // character in a preceding skipped key so the scalar's
        // recorded character-index differs from its byte-index.
        //
        // YAML: "desc_é: skip\nname: hit\n"
        //        ↑ 6 bytes header (d e s c _ é = 7 bytes, é = 2)
        //        → "name" starts at char index 12, byte index 13
        //          (counts: d,e,s,c,_,é,:,space,s,k,i,p,\n,n)
        //          Without the table `ByteOffset` would land on the
        //          wrong `n`.
        let yaml = "desc_\u{00e9}: skip\nname: hit\n";
        let wf = parse(yaml, fid()).expect("parse");
        let spanned = wf.name.expect("name present");
        // Scalar value "hit" starts after "name: " on line 2.
        // Byte offset = bytes_before_line_2 + 6 ("name: ").
        let line1_bytes = "desc_\u{00e9}: skip\n".len();
        let expected_start = u32::try_from(line1_bytes + 6).unwrap();
        assert_eq!(
            spanned.span.start,
            ByteOffset::new(expected_start),
            "span start must be byte offset after char→byte translation",
        );
    }

    #[test]
    fn parse_error_span_carries_original_file_id() {
        // P1 review fix: the `.unwrap_or_default()` path used to
        // silently drop the caller's `FileId` and return
        // `FileId(0)`. Assert the error path now preserves it.
        let err = parse("name:\n  - foo\n", FileId::new(42)).expect_err("seq value");
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
    fn parse_duplicate_top_level_keys_takes_last() {
        // marked-yaml / yaml-rust2 treats duplicate mapping keys as
        // last-wins. Documenting the contract so a future upstream
        // change can't silently flip it.
        let wf = parse("name: first\nname: second\n", fid()).expect("parse");
        assert_eq!(wf.name.unwrap().value, "second");
    }

    #[test]
    fn parse_yaml_syntax_error_maps_to_schema_error() {
        let err = parse("name: [unclosed\n", fid()).expect_err("bad yaml");
        assert!(matches!(err, SchemaError::YamlSyntax { .. }));
    }

    #[test]
    fn parse_ignores_unhandled_fields_for_now() {
        // Round 2c handled the six top-level scalars; Round 2d
        // wires `tasks:`. Remaining future-round fields (`mcp`,
        // `context`, `include`, `inputs`, `logging`, `orchestrate`,
        // `routing`, `schedule`, `max_duration_secs`) must still not
        // error — lenient-by-default incremental scaffolding.
        let yaml = "\
name: partial
context:
  window: 2048
logging:
  level: info
include:
  - shared.yaml
";
        let wf = parse(yaml, fid()).expect("parse");
        assert_eq!(wf.name.unwrap().value, "partial");
        assert!(wf.context.is_none());
        assert!(wf.logging.is_none());
        assert!(wf.include.is_empty());
    }

    #[test]
    fn parse_ignores_unknown_top_level_keys() {
        // Unknown keys are silently ignored (lenient-by-default) —
        // strict-mode reporting lands in Round 3 analyzer.
        let yaml = "\
name: demo
unknown_field: whatever
";
        let wf = parse(yaml, fid()).expect("parse");
        assert_eq!(wf.name.unwrap().value, "demo");
    }

    #[test]
    fn parse_file_id_propagates_into_span() {
        let wf = parse("name: x\n", FileId::new(42)).expect("parse");
        assert_eq!(wf.name.unwrap().span.file, FileId::new(42));
    }
}
