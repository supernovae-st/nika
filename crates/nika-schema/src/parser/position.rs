// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source positions: a `marked-yaml` marker to the byte span every
//! diagnostic carries.

use marked_yaml::{Marker, Span as YamlSpan};

use crate::error::SchemaError;
use crate::source::{ByteOffset, FileId, Span};

/// Precomputed marker → byte-offset tables for the source text.
///
/// `marked-yaml` 0.8 reports a character index, a 1-based line and a
/// 1-based column per marker; our [`ByteOffset`] and miette's
/// `SourceSpan` want byte offsets. ASCII inputs are the hot path: the
/// index IS the byte offset, so the constructor short-circuits. Any
/// other input is read by LINE and COLUMN, never by the index:
/// yaml-rust2 0.10 (`scan_block_scalar_content_line`) advances the index
/// of a block-scalar line it reads in bulk by that line's BYTE length,
/// so after one non-ASCII `|` or `>` line every later index overshoots,
/// while the line count and the column (reset at each break) stay exact.
pub(crate) struct CharToByte {
    /// `byte_at[char_idx]` is the UTF-8 byte offset. Contains
    /// `source.len()` as its final sentinel so end-of-file positions
    /// resolve without panicking.
    byte_at: Vec<u32>,
    /// Per line, split at yaml-rust2's own breaks (`\r\n` · `\r` · `\n`):
    /// the char index where it starts and where its content ends (its
    /// break, or the end of the source). Empty on the ASCII path.
    lines: Vec<(u32, u32)>,
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
                lines: Vec::new(),
            });
        }
        let mut byte_at: Vec<u32> = source
            .char_indices()
            .map(|(b, _)| u32::try_from(b).unwrap_or(u32::MAX))
            .collect();
        byte_at.push(u32::try_from(source.len()).unwrap_or(u32::MAX));
        let lines = line_table(source);
        Ok(Self { byte_at, lines })
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

    /// The byte offset of one marker: its index on the ASCII path, else
    /// its line and column (see the type). A column past its line's
    /// content clamps to the line's end — the bulk read overshoots the
    /// column on its own line too, and that content runs to the break. A
    /// line past the last one (the stream end yaml-rust2 forces onto a
    /// new line) is the end of the source.
    pub(super) fn marker(&self, marker: &Marker) -> u32 {
        if self.lines.is_empty() {
            return self.byte(marker.character());
        }
        let Some(&(start, end)) = self.lines.get(marker.line().saturating_sub(1)) else {
            return self.byte(usize::MAX);
        };
        let column = u32::try_from(marker.column().saturating_sub(1)).unwrap_or(u32::MAX);
        let char_idx = start.saturating_add(column).min(end);
        self.byte(usize::try_from(char_idx).unwrap_or(usize::MAX))
    }
}

/// Each line's `(start, content end)` char indices, split exactly where
/// yaml-rust2 counts a line: `\r\n` once, a lone `\r`, a `\n`.
fn line_table(source: &str) -> Vec<(u32, u32)> {
    let mut lines = Vec::new();
    let mut start = 0_u32;
    let mut chars = source.chars().zip(0_u32..).peekable();
    while let Some((ch, at)) = chars.next() {
        if ch == '\r' && chars.peek().is_some_and(|&(next, _)| next == '\n') {
            chars.next();
            lines.push((start, at));
            start = at.saturating_add(2);
        } else if ch == '\r' || ch == '\n' {
            lines.push((start, at));
            start = at.saturating_add(1);
        }
    }
    let total = u32::try_from(source.chars().count()).unwrap_or(u32::MAX);
    lines.push((start, total));
    lines
}

/// Convert a `marked_yaml::Span` into our span (attached to `file_id`).
///
/// marked-yaml reports the start marker (and sometimes no end). When
/// both markers are present we produce a `[start, end)` range; when
/// only `start` is available the span is zero-length (point). An
/// entirely blank yaml span yields `None`. Markers are translated to
/// byte offsets by [`CharToByte::marker`].
pub(crate) fn yaml_span_to_span(
    file_id: FileId,
    span: &YamlSpan,
    char_to_byte: &CharToByte,
) -> Option<Span> {
    let start = span.start()?;
    let start_off = char_to_byte.marker(start);
    let end_off = span.end().map_or(start_off, |m| char_to_byte.marker(m));
    Some(Span::new(
        file_id,
        ByteOffset::new(start_off),
        ByteOffset::new(end_off),
    ))
}

#[cfg(test)]
mod tests;
