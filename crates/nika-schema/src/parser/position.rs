// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source positions: a `marked-yaml` marker to the byte span every
//! diagnostic carries.

use marked_yaml::Span as YamlSpan;

use crate::error::SchemaError;
use crate::source::{ByteOffset, FileId, Span};

/// Precomputed char-index → byte-offset table for the source text.
///
/// `marked-yaml` 0.8 reports positions as character indices; our
/// [`ByteOffset`] and miette's `SourceSpan` want byte offsets. ASCII
/// inputs are the hot path, so the constructor short-circuits.
pub(crate) struct CharToByte {
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

/// Convert a `marked_yaml::Span` into our span (attached to `file_id`).
///
/// marked-yaml reports the start marker (and sometimes no end). When
/// both markers are present we produce a `[start, end)` range; when
/// only `start` is available the span is zero-length (point). An
/// entirely blank yaml span yields `None`. Character indices from
/// marked-yaml are translated to byte offsets via `char_to_byte`.
pub(crate) fn yaml_span_to_span(
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
