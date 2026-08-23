// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unfilled `# SLOT:` comments — YAML comments survive only in the
//! source, never the AST, so this scan reads the file bytes.
//!
//! A `# SLOT:` line inside a block scalar is prompt CONTENT (the
//! 2026-07-29 pack lesson). The scanner tracks `|` / `>` so those
//! lines are not counted.

use crate::ByteSpan;

/// One unfilled SLOT comment in the source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct SlotMarker {
    /// 1-based source line of the comment.
    pub line: u32,
    /// Comment body after `#`, trimmed (`SLOT: kebab-case workflow id`).
    pub body: String,
    /// Byte range of the `SLOT:` marker through the rest of the comment.
    pub span: ByteSpan,
}

impl SlotMarker {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(line: u32, body: String, span: ByteSpan) -> Self {
        Self { line, body, span }
    }

    /// The teaching after `SLOT:`, trimmed.
    #[must_use]
    pub fn label(&self) -> &str {
        self.body
            .strip_prefix("SLOT:")
            .map_or(self.body.as_str(), str::trim)
    }
}

/// Human / JSON finding row: how many markers, that the file is still a
/// scaffold, and the repair (fill every SLOT, then delete the comments).
#[must_use]
pub fn slot_refusal_message(slots: &[SlotMarker]) -> String {
    let n = slots.len();
    let word = if n == 1 { "marker" } else { "markers" };
    let named = slots
        .iter()
        .map(|s| format!("line {}: {}", s.line, s.label()))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "{n} unfilled SLOT {word} ({named}) — this file is still a scaffold, \
         not a workflow. Fill every SLOT then delete the comments."
    )
}

/// YAML comments whose body starts with `SLOT:` (`# SLOT:` · inline
/// `  # SLOT:`). Quoted `#` and block-scalar content are not comments.
#[must_use]
pub fn scan_unfilled_slots(source: &str) -> Vec<SlotMarker> {
    let mut out = Vec::new();
    let mut byte = 0usize;
    let mut block: Option<Block> = None;
    for (idx, raw) in source.split_inclusive('\n').enumerate() {
        let line = raw
            .strip_suffix('\n')
            .unwrap_or(raw)
            .strip_suffix('\r')
            .unwrap_or(raw);
        let skip = block.as_mut().is_some_and(|b| b.skip(line));
        if skip {
            byte = byte.saturating_add(raw.len());
            continue;
        }
        block = None;
        if let Some(hit) = comment_slot(line, idx, byte) {
            out.push(hit);
        }
        if is_block_scalar_value(code_before_comment(line)) {
            block = Some(Block {
                parent_indent: leading_indent(line),
                content_indent: None,
            });
        }
        byte = byte.saturating_add(raw.len());
    }
    out
}

struct Block {
    parent_indent: usize,
    content_indent: Option<usize>,
}

impl Block {
    /// True while `line` is still inside the scalar (content, not a key).
    fn skip(&mut self, line: &str) -> bool {
        if line.trim().is_empty() {
            return true;
        }
        let indent = leading_indent(line);
        match self.content_indent {
            None if indent > self.parent_indent => {
                self.content_indent = Some(indent);
                true
            }
            None => false,
            Some(ci) => indent >= ci,
        }
    }
}

fn leading_indent(line: &str) -> usize {
    line.bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count()
}

fn comment_slot(line: &str, idx: usize, line_start: usize) -> Option<SlotMarker> {
    let (hash_at, comment) = split_comment(line)?;
    let pad = comment.len() - comment.trim_start().len();
    let body = comment.trim_start();
    if !body.starts_with("SLOT:") {
        return None;
    }
    let body = body.trim_end().to_owned();
    let start = line_start.saturating_add(hash_at.saturating_add(1).saturating_add(pad));
    let end = start.saturating_add(body.len());
    Some(SlotMarker::new(
        u32::try_from(idx.saturating_add(1)).unwrap_or(u32::MAX),
        body,
        ByteSpan::new(
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(end).unwrap_or(u32::MAX),
        ),
    ))
}

/// Index of `#` that opens a YAML comment, and the bytes after it.
fn split_comment(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('#') {
        let hash_at = line.len() - trimmed.len();
        return Some((hash_at, rest));
    }
    inline_hash(line).map(|hash_at| (hash_at, &line[hash_at.saturating_add(1)..]))
}

fn code_before_comment(line: &str) -> &str {
    split_comment(line).map_or(line, |(hash_at, _)| &line[..hash_at])
}

fn inline_hash(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut i = 0;
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    while i < b.len() {
        let c = b[i];
        if single {
            if c == b'\'' {
                if b.get(i.saturating_add(1)) == Some(&b'\'') {
                    i = i.saturating_add(2);
                    continue;
                }
                single = false;
            }
        } else if double {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                double = false;
            }
        } else {
            match c {
                b'\'' => single = true,
                b'"' => double = true,
                b'#' if i > 0 && b[i - 1].is_ascii_whitespace() => return Some(i),
                _ => {}
            }
        }
        i = i.saturating_add(1);
    }
    None
}

fn is_block_scalar_value(code: &str) -> bool {
    let Some((_, after)) = code.trim_end().rsplit_once(':') else {
        return false;
    };
    block_header_suffix(after.trim())
}

/// YAML block-header suffix: `|` / `>` plus optional chomp / indent.
fn block_header_suffix(s: &str) -> bool {
    let b = s.as_bytes();
    let Some((first, rest)) = b.split_first() else {
        return false;
    };
    if *first != b'|' && *first != b'>' {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    match rest {
        [b'+' | b'-', digits @ ..] => digits.iter().all(u8::is_ascii_digit),
        [b'0'..=b'9', ..] => {
            let n = rest.iter().take_while(|c| c.is_ascii_digit()).count();
            matches!(&rest[n..], [] | [b'+' | b'-'])
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{scan_unfilled_slots, slot_refusal_message};

    #[test]
    fn two_comment_markers_are_named() {
        let src = "nika: w       # SLOT: kebab-case\n# SLOT: the one model job\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n";
        let hits = scan_unfilled_slots(src);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert_eq!(hits[0].line, 1);
        assert_eq!(hits[0].label(), "kebab-case");
        assert_eq!(hits[1].line, 2);
        assert_eq!(hits[1].label(), "the one model job");
        let msg = slot_refusal_message(&hits);
        assert!(msg.contains("2 unfilled SLOT markers"), "{msg}");
        assert!(msg.contains("scaffold"), "{msg}");
        assert!(msg.contains("delete the comments"), "{msg}");
    }

    #[test]
    fn block_scalar_content_is_not_a_marker() {
        let src = "nika: w\ntasks:\n  t:\n    infer:\n      prompt: |\n        Summarize.\n        # SLOT: not a marker\n";
        assert!(scan_unfilled_slots(src).is_empty());
    }

    #[test]
    fn a_comment_that_mentions_the_marker_is_not_one() {
        let src = "nika: w\ntasks:\n  t:\n    infer:\n      # `# SLOT:` line is sent to the model verbatim\n      prompt: hi\n";
        assert!(scan_unfilled_slots(src).is_empty());
    }

    #[test]
    fn a_quoted_hash_is_not_a_comment() {
        let src = "nika: w\nconst:\n  note: \"say # SLOT: hi\"\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n";
        assert!(scan_unfilled_slots(src).is_empty());
    }

    #[test]
    fn a_comment_above_the_block_still_counts() {
        let src = "nika: w\ntasks:\n  t:\n    infer:\n      # SLOT: the one model job\n      prompt: |\n        keep slot markers OUT\n";
        let hits = scan_unfilled_slots(src);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].label(), "the one model job");
    }
}
