// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Incremental SSE (Server-Sent Events) parser.
//!
//! Both wire formats this crate speaks (Anthropic Messages, OpenAI-compat
//! Chat Completions) stream over SSE. This parser is deliberately minimal:
//! it buffers raw bytes, splits on blank-line event boundaries, and yields
//! the concatenated `data:` payload of each complete event. `event:` names,
//! `id:` fields and comments are ignored — both dialects carry the
//! discriminator inside the JSON payload.
//!
//! Hand-rolled on purpose (CRAFT · zero new deps): the grammar we need is
//! ~30 lines.

/// Incremental SSE event splitter. Feed chunks, drain complete events.
///
/// Memory bound: this parser does not cap `buf` itself — the bound is the
/// http effect's response-size cap (`HttpError::TooLarge` mid-stream ·
/// 64 MiB default in `nika-http`). That invariant is load-bearing at the
/// seam: any alternative `HttpPostDyn` impl wired into the registry must
/// cap its streaming bodies the same way.
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    buf: Vec<u8>,
    /// Resume offset for boundary scanning — bytes before this index were
    /// already scanned and contain no boundary (keeps a multi-chunk event
    /// linear instead of re-scanning the whole buffer per `feed`).
    scan_from: usize,
}

impl SseParser {
    /// Create an empty parser.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk and return the `data:` payloads of every event that
    /// became complete with this chunk, in order.
    pub(crate) fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut events = Vec::new();
        // An SSE event ends at a blank line: "\n\n" (we normalize \r away).
        // Resume the scan where the last one stopped (minus the longest
        // boundary prefix that may straddle the chunk seam: "\n\r").
        loop {
            let start = self.scan_from.saturating_sub(2);
            let Some(pos) = find_boundary(&self.buf[start..]) else {
                self.scan_from = self.buf.len();
                break;
            };
            let end = start + pos.end;
            let content = start + pos.start;
            let raw: Vec<u8> = self.buf.drain(..end).collect();
            self.scan_from = 0;
            let block = String::from_utf8_lossy(&raw[..content]);
            let data = extract_data(&block);
            if !data.is_empty() {
                events.push(data);
            }
        }
        events
    }
}

/// Byte range of the first event boundary: `start` = end of block content,
/// `end` = first byte after the blank-line separator.
struct Boundary {
    start: usize,
    end: usize,
}

/// Find the first `\n\n` / `\r\n\r\n` / `\n\r\n` boundary.
fn find_boundary(buf: &[u8]) -> Option<Boundary> {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' {
            // "\n\n"
            if buf[i + 1] == b'\n' {
                return Some(Boundary {
                    start: i,
                    end: i + 2,
                });
            }
            // "\n\r\n"
            if i + 2 < buf.len() && buf[i + 1] == b'\r' && buf[i + 2] == b'\n' {
                return Some(Boundary {
                    start: i,
                    end: i + 3,
                });
            }
        }
        i += 1;
    }
    None
}

/// Concatenate the `data:` lines of one event block (SSE multi-line data
/// joins with `\n` per spec).
fn extract_data(block: &str) -> String {
    let mut out = String::new();
    for line in block.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.strip_prefix("data:") {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(rest.strip_prefix(' ').unwrap_or(rest));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn single_event_single_chunk() {
        let mut p = SseParser::new();
        let events = p.feed(b"data: {\"a\":1}\n\n");
        assert_eq!(events, vec!["{\"a\":1}".to_owned()]);
    }

    #[test]
    fn event_split_across_chunks() {
        let mut p = SseParser::new();
        assert!(p.feed(b"data: {\"a\"").is_empty());
        assert!(p.feed(b":1}").is_empty());
        let events = p.feed(b"\n\n");
        assert_eq!(events, vec!["{\"a\":1}".to_owned()]);
    }

    #[test]
    fn multiple_events_one_chunk() {
        let mut p = SseParser::new();
        let events = p.feed(b"data: 1\n\ndata: 2\n\ndata: 3\n\n");
        assert_eq!(events, vec!["1", "2", "3"]);
    }

    #[test]
    fn event_name_lines_ignored_data_kept() {
        let mut p = SseParser::new();
        let events = p.feed(b"event: content_block_delta\ndata: {\"x\":2}\n\n");
        assert_eq!(events, vec!["{\"x\":2}".to_owned()]);
    }

    #[test]
    fn comments_and_ids_ignored() {
        let mut p = SseParser::new();
        let events = p.feed(b": keepalive\nid: 7\nretry: 100\n\n");
        assert!(events.is_empty());
    }

    #[test]
    fn crlf_boundaries() {
        let mut p = SseParser::new();
        let events = p.feed(b"data: a\r\n\r\ndata: b\r\n\r\n");
        assert_eq!(events, vec!["a", "b"]);
    }

    #[test]
    fn multi_line_data_joined_with_newline() {
        let mut p = SseParser::new();
        let events = p.feed(b"data: line1\ndata: line2\n\n");
        assert_eq!(events, vec!["line1\nline2".to_owned()]);
    }

    #[test]
    fn no_space_after_colon_accepted() {
        let mut p = SseParser::new();
        let events = p.feed(b"data:tight\n\n");
        assert_eq!(events, vec!["tight"]);
    }

    #[test]
    fn incomplete_tail_stays_buffered() {
        let mut p = SseParser::new();
        assert!(p.feed(b"data: pending\n").is_empty());
        let events = p.feed(b"\n");
        assert_eq!(events, vec!["pending"]);
    }

    #[test]
    fn long_event_across_many_chunks_stays_linear_and_correct() {
        // 2k chunks of boundary-less data then the terminator — exercises
        // the scan cursor (quadratic rescan would still pass but this
        // guards the cursor arithmetic at the seams).
        let mut p = SseParser::new();
        let chunk = b"data: 0123456789abcdef";
        for _ in 0..2000 {
            assert!(p.feed(chunk).is_empty());
        }
        let events = p.feed(b"\n\n");
        assert_eq!(events.len(), 1);
        assert!(events[0].starts_with("0123456789abcdef"));
        // One logical `data:` line (no newlines between chunks): the first
        // chunk contributes its 16 payload bytes, each subsequent chunk its
        // full 22 raw bytes (the embedded `data: ` is content, not a field).
        assert_eq!(events[0].len(), 16 + 1999 * 22);
    }

    proptest! {
        /// Chunking must never change the parsed result: any split of the
        /// same byte stream yields the same event sequence.
        #[test]
        fn chunking_invariance(splits in proptest::collection::vec(0usize..60, 0..6)) {
            let stream = b"data: {\"a\":1}\n\nevent: x\ndata: two\n\ndata: l1\ndata: l2\r\n\r\n: c\n\ndata: last\n\n";
            let mut whole = SseParser::new();
            let expected = whole.feed(stream);

            let mut cuts: Vec<usize> = splits.into_iter().map(|s| s % stream.len()).collect();
            cuts.sort_unstable();
            let mut chunked = SseParser::new();
            let mut got = Vec::new();
            let mut prev = 0;
            for c in cuts {
                got.extend(chunked.feed(&stream[prev..c]));
                prev = c;
            }
            got.extend(chunked.feed(&stream[prev..]));
            prop_assert_eq!(got, expected);
        }
    }
}
