// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Go-to-definition for task references (single-file).
//!
//! When the cursor sits on a task REFERENCE, jump to the task DEFINITION
//! (its `id`). Two reference shapes are resolved in v0.1 ·
//!
//! 1. an `after:` target (`after: { extract: success }` → the `extract`
//!    task's id), resolved from the parser's own item spans.
//! 2. a `tasks.<id>` chain inside a `${{ … }}` island (`${{
//!    tasks.extract.output.summary }}` → the `extract` task's id),
//!    resolved by a document-level byte scan of the islands.
//!
//! The definition target is always a task `id` span, mapped to an LSP
//! [`Location`] over the given URI. Single-file: every reference resolves
//! within the same document (v0.1 has no includes).

use std::collections::BTreeMap;

use lsp_types::{Location, Range, Uri};
use nika_schema::raw::RawWorkflow;
use nika_schema::{FileId, ParseMode, Span, parse};

use super::position::LineIndex;

/// Resolve the task reference under `offset` to the defining task's `id`
/// location. Returns `None` when the cursor is not on a resolvable task
/// reference, or the source does not parse.
#[must_use]
pub fn definition(uri: &Uri, text: &str, offset: usize) -> Option<Location> {
    let wf = parse(text, FileId::new(0), ParseMode::Lenient).ok()?;
    let index = LineIndex::new(text);
    // member refs (`${{ inputs.X }}` · `const.X` · `secrets.X`) jump to
    // their declaration — the parser's own name spans.
    if let Some((span, len)) = member_decl_target(&wf, text, offset) {
        return Some(Location::new(uri.clone(), token_range(&index, span, len)));
    }
    let id_spans = id_span_table(&wf);
    let target_id = after_target(&wf, offset).or_else(|| template_task_target(text, offset))?;
    let (span, id_len) = id_spans.get(target_id.as_str())?;
    Some(Location::new(
        uri.clone(),
        token_range(&index, *span, *id_len),
    ))
}

/// If `offset` sits on an `inputs.X` / `const.X` /
/// `secrets.X` member inside an island, return the DECLARATION's name
/// span (+ byte length). The same byte-scan discipline as
/// [`template_task_target`], generalized over the declaring roots.
fn member_decl_target(wf: &RawWorkflow, text: &str, offset: usize) -> Option<(Span, usize)> {
    let (root, name) = template_member_at(text, offset)?;
    match root {
        "inputs" => wf
            .inputs
            .iter()
            .find(|(n, _)| n.value == name)
            .map(|(n, _)| (n.span, n.value.len())),
        "const" => wf
            .consts
            .iter()
            .find(|(n, _)| n.value == name)
            .map(|(n, _)| (n.span, n.value.len())),
        _ => wf
            .secrets
            .iter()
            .find(|(n, _)| n.value == name)
            .map(|(n, _)| (n.span, n.value.len())),
    }
}

/// The `(root, member)` under `offset` when the cursor sits on a
/// `vars.<ident>` / `secrets.<ident>` / `env.<ident>` run inside an
/// island — cursor anywhere from the root keyword through the member.
pub(crate) fn template_member_at(text: &str, offset: usize) -> Option<(&'static str, String)> {
    let bytes = text.as_bytes();
    for (island_start, island_end) in islands(text) {
        if !(island_start..island_end).contains(&offset) {
            continue;
        }
        let inner = text.get(island_start..island_end)?;
        for root in ["inputs", "const", "secrets"] {
            let needle = format!("{root}.");
            let mut search_from = 0usize;
            while let Some(rel) = inner.get(search_from..)?.find(&needle) {
                let kw_at = island_start + search_from + rel;
                let mid_ident = kw_at > 0
                    && bytes
                        .get(kw_at - 1)
                        .is_some_and(|b| *b == b'_' || *b == b'.' || b.is_ascii_alphanumeric());
                if !mid_ident {
                    let m_start = kw_at + needle.len();
                    let m_end = ident_end(bytes, m_start);
                    if m_end > m_start && (kw_at..m_end).contains(&offset) {
                        return text.get(m_start..m_end).map(|m| (root, m.to_owned()));
                    }
                }
                search_from = (search_from + rel + needle.len()).min(inner.len());
            }
        }
    }
    None
}

/// The task referenced under `offset` (an `after:` target or a
/// `${{ tasks.X }}` ref) as `(id, verb)`. Shared with [`hover`](super::hover)
/// so it can show the target task's verb. `None` when the cursor is not on a
/// resolvable reference or the source does not parse.
pub(crate) fn referenced_task_at(text: &str, offset: usize) -> Option<(String, &'static str)> {
    let wf = parse(text, FileId::new(0), ParseMode::Lenient).ok()?;
    let id = after_target(&wf, offset).or_else(|| template_task_target(text, offset))?;
    let verb = wf
        .tasks
        .iter()
        .find(|t| t.value.id.value == id)
        .map(|t| t.value.action.verb())?;
    Some((id, verb))
}

/// Map every task id → (its `id` span, its byte length). The length widens
/// the parser's point span to the actual token for the jump target range.
fn id_span_table(wf: &RawWorkflow) -> BTreeMap<String, (Span, usize)> {
    wf.tasks
        .iter()
        .map(|t| {
            (
                t.value.id.value.clone(),
                (t.value.id.span, t.value.id.value.len()),
            )
        })
        .collect()
}

/// If `offset` falls inside an `after:` target, return the id it names.
///
/// The parser emits POINT spans for flow-list scalars (`start == end`,
/// anchored at the token start · see `check/mod.rs`'s own note). The span
/// is widened to the value's byte length so the cursor resolves anywhere
/// on the token, not only on its first byte.
fn after_target(wf: &RawWorkflow, offset: usize) -> Option<String> {
    let off = u32::try_from(offset).unwrap_or(u32::MAX);
    for task in &wf.tasks {
        for (target, _pred) in &task.value.after {
            if token_span_contains(target.span, target.value.len(), off) {
                return Some(target.value.clone());
            }
        }
    }
    None
}

/// If `offset` falls inside a `tasks.<id>` chain within a `${{ … }}`
/// island, return the `<id>`. A document-level byte scan: locate each
/// island, then within it find `tasks.<ident>` runs and test the cursor
/// against the `<ident>` byte range.
fn template_task_target(text: &str, offset: usize) -> Option<String> {
    let bytes = text.as_bytes();
    for (island_start, island_end) in islands(text) {
        let inner = text.get(island_start..island_end)?;
        // walk `tasks.<ident>` occurrences inside the island
        let mut search_from = 0usize;
        while let Some(rel) = inner.get(search_from..)?.find("tasks.") {
            let kw_at = island_start + search_from + rel;
            // Require a LEFT word boundary — a `tasks.` sitting mid-identifier
            // (`mytasks.foo`) is NOT a task reference.
            let mid_ident = kw_at > 0
                && bytes
                    .get(kw_at - 1)
                    .is_some_and(|b| *b == b'_' || b.is_ascii_alphanumeric());
            if !mid_ident {
                let id_start = kw_at + "tasks.".len();
                let id_end = ident_end(bytes, id_start);
                // resolve when the cursor is anywhere from the `tasks.`
                // keyword through the end of the id it names.
                if id_end > id_start && (kw_at..id_end).contains(&offset) {
                    return text.get(id_start..id_end).map(str::to_owned);
                }
            }
            search_from = (search_from + rel + "tasks.".len()).min(inner.len());
        }
    }
    None
}

/// The byte ranges `[start, end)` of every `${{ … }}` island in `text`
/// (`start` is just past `${{`, `end` is at the closing `}}`). Honors the
/// `\${{` literal escape and quote-aware close detection.
pub(super) fn islands(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if !bytes[i..].starts_with(b"${{") {
            i += 1;
            continue;
        }
        if backslash_run_is_odd(bytes, i) {
            // `\${{` (an ODD backslash run) is an escaped literal; `\\${{`
            // (an EVEN run) is an escaped backslash before a REAL island.
            i += 3;
            continue;
        }
        let body_start = i + 3;
        match find_close(bytes, body_start) {
            Some(close) => {
                out.push((body_start, close));
                i = close + 2;
            }
            None => break, // unterminated — nothing more to scan
        }
    }
    out
}

/// Whether the run of `\` immediately before `at` has ODD length. An odd
/// run escapes the following `${{` (a literal `\${{`); an even run is
/// escaped backslashes that leave the island live (`\\${{` is a real
/// island preceded by a literal backslash).
fn backslash_run_is_odd(bytes: &[u8], at: usize) -> bool {
    let mut count = 0usize;
    let mut j = at;
    while j > 0 && bytes.get(j - 1) == Some(&b'\\') {
        count += 1;
        j -= 1;
    }
    count % 2 == 1
}

/// Find the closing `}}` from `from`, skipping CEL string literals.
fn find_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => {
                if b == b'\'' || b == b'"' {
                    quote = Some(b);
                } else if b == b'}' && bytes.get(i + 1) == Some(&b'}') {
                    return Some(i);
                }
            }
        }
        i += 1;
    }
    None
}

/// The byte offset just past an identifier starting at `start` (CEL ids
/// are `[A-Za-z_][A-Za-z0-9_]*` · matches the `snake_case` task-id grammar).
pub(super) fn ident_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while let Some(&b) = bytes.get(i) {
        if b == b'_' || b.is_ascii_alphanumeric() {
            i += 1;
        } else {
            break;
        }
    }
    i
}

/// Whether `offset` falls within the token a `span` anchors, given the
/// token's byte length. A point span (`start == end`) is widened to
/// `[start, start + token_len)`; a real range uses its own end.
pub(super) fn token_span_contains(span: Span, token_len: usize, offset: u32) -> bool {
    let end = if span.end.0 > span.start.0 {
        span.end.0
    } else {
        span.start.0 + u32::try_from(token_len).unwrap_or(0)
    };
    offset >= span.start.0 && offset < end.max(span.start.0 + 1)
}

/// The LSP range over the token a point/real span anchors, widened to the
/// token byte length so the jump target highlights the whole id.
pub(super) fn token_range(index: &LineIndex, span: Span, token_len: usize) -> Range {
    let start = span.start.0 as usize;
    let end = if span.end.0 > span.start.0 {
        span.end.0 as usize
    } else {
        start + token_len
    };
    Range::new(index.position(start), index.position(end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn uri() -> Uri {
        Uri::from_str("file:///w.nika.yaml").expect("valid uri")
    }

    #[test]
    fn two_task_refs_in_one_island_both_resolve() {
        // A single island holds TWO `tasks.<id>` references. The inner search
        // loop must advance PAST the first (`search_from + rel + len`) and
        // find the second — and compute each `kw_at` from
        // `island_start + search_from + rel`. The FIRST resolves to `aaa`,
        // the SECOND to `bbb`; a broken advance/kw_at would mis-resolve or
        // miss the second.
        let yaml = "nika: w\ntasks:\n  aaa:\n    exec: { command: [\"x\"] }\n  bbb:\n    exec: { command: [\"${{ tasks.aaa == tasks.bbb }}\"] }\n";
        let index = LineIndex::new(yaml);
        let aaa_id = yaml.find("\n  aaa:").map(|p| p + 3).expect("aaa");
        let bbb_id = yaml.find("\n  bbb:").map(|p| p + 3).expect("bbb");
        let first = yaml.find("tasks.aaa").expect("ref1") + "tasks.".len();
        let second = yaml.find("tasks.bbb").expect("ref2") + "tasks.".len();
        assert_eq!(
            definition(&uri(), yaml, first).map(|l| l.range.start),
            Some(index.position(aaa_id)),
            "first ref → aaa"
        );
        assert_eq!(
            definition(&uri(), yaml, second).map(|l| l.range.start),
            Some(index.position(bbb_id)),
            "second ref in the SAME island → bbb (loop advanced correctly)"
        );
    }

    #[test]
    fn template_task_target_walks_three_refs_in_one_island() {
        // Test the resolver DIRECTLY (not through `definition`, which filters
        // unknown ids to None and would mask a wrong-but-nonempty result).
        // THREE refs in ONE island: the loop advance
        // `search_from + rel + len` must step correctly past EACH match to
        // reach the next. A `*` mutation on either `+` mis-advances on the
        // SECOND iteration (where `search_from` is non-zero), so the THIRD
        // ref becomes unreachable → None. Querying the THIRD ref forces two
        // correct advances.
        let text = "${{ tasks.aaa + tasks.bbb + tasks.ccc }}";
        let first = text.find("tasks.aaa").expect("r1") + "tasks.".len();
        let second = text.find("tasks.bbb").expect("r2") + "tasks.".len();
        let third = text.rfind("tasks.ccc").expect("r3") + "tasks.".len();
        assert_eq!(
            template_task_target(text, first),
            Some("aaa".to_owned()),
            "cursor in the first ref → aaa"
        );
        assert_eq!(
            template_task_target(text, second),
            Some("bbb".to_owned()),
            "cursor in the SECOND ref → bbb (one correct advance)"
        );
        assert_eq!(
            template_task_target(text, third),
            Some("ccc".to_owned()),
            "cursor in the THIRD ref → ccc (TWO correct advances; a broken \
             `+`→`*` advance overshoots and returns None here)"
        );
    }

    #[test]
    fn template_task_target_rejects_empty_id_returning_none_not_empty_string() {
        // `${{ tasks. }}` (dot then space → empty id). The `id_end > id_start`
        // guard must reject it and return None — NOT Some(""). A `>=`
        // mutation would return Some(""), which `definition` happens to
        // filter to None downstream; testing the resolver DIRECTLY catches
        // the divergence the integration path masks.
        let text = "${{ tasks. }}";
        let kw = text.find("tasks.").expect("kw");
        assert_eq!(
            template_task_target(text, kw),
            None,
            "an empty id resolves to None, not Some(\"\")"
        );
        // also through the public surface for good measure.
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { command: [\"${{ tasks. }}\"] }\n";
        assert!(definition(&uri(), yaml, yaml.find("tasks.").unwrap()).is_none());
    }

    #[test]
    fn lone_brace_in_island_body_does_not_close_it() {
        // A single `}` inside the island (e.g. CEL `x }`) is NOT a close —
        // only `}}` is. `find_close`'s `bytes.get(i + 1) == Some(&b'}')`
        // requires the NEXT byte; if it checked the current byte instead
        // (`i * 1`), a lone `}` would close the island early and `tasks.a`
        // (after the lone `}`) would be unreachable.
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { shell: \"${{ x } tasks.a }}\" }\n";
        let index = LineIndex::new(yaml);
        let a_id = yaml.find("\n  a:").map(|p| p + 3).expect("a id");
        let at = yaml.find("tasks.a }}").expect("ref") + "tasks.".len();
        assert_eq!(
            definition(&uri(), yaml, at).map(|l| l.range.start),
            Some(index.position(a_id)),
            "the lone `}}` before `tasks.a` does not close the island early"
        );
    }

    #[test]
    fn quoted_double_brace_does_not_close_the_island() {
        // A `}}` INSIDE a CEL string literal must NOT close the island. The
        // in-quote arm exits ONLY on the matching quote (`b == q`); flipping
        // to `b != q` would close the quote on the FIRST non-quote char and
        // then treat a `}}` mid-string as a real close. Use `'a}}b'` (the
        // `}}` is NOT adjacent to the quote) so the `!=` mutant closes at the
        // inner `}}` while the original runs on to the real one.
        let body = "${{ 'a}}b' + tasks.x }}";
        let isl = islands(body);
        assert_eq!(isl.len(), 1, "one island, closed at the OUTER `}}`");
        assert_eq!(
            &body[isl[0].0..isl[0].1],
            " 'a}}b' + tasks.x ",
            "the quoted `}}` is inside the body, not the close"
        );
        // and the ref AFTER the quoted `}}` resolves (only reachable if the
        // island didn't close early on the inner `}}`).
        assert_eq!(
            template_task_target(body, body.find("tasks.x").expect("ref") + "tasks.".len()),
            Some("x".to_owned()),
            "tasks.x after the quoted `}}` resolves"
        );
        // end-to-end through the public surface.
        let yaml = "nika: w\ntasks:\n  x:\n    exec: { shell: \"${{ 'a}}b' + tasks.x }}\" }\n";
        let index = LineIndex::new(yaml);
        let x_id = yaml.find("\n  x:").map(|p| p + 3).expect("x id");
        let at = yaml.rfind("tasks.x").expect("ref") + "tasks.".len();
        assert_eq!(
            definition(&uri(), yaml, at).map(|l| l.range.start),
            Some(index.position(x_id)),
            "the quoted `}}` is skipped; `tasks.x` after it still resolves"
        );
    }

    #[test]
    fn escaped_island_is_skipped_but_the_following_real_one_is_scanned() {
        // `\${{ … }}` is escaped (skipped via `i += 3`); the REAL island
        // AFTER it must still be found. The 4-char prefix puts the escaped
        // `${{` deep enough that a broken skip-advance (`i *= 3` instead of
        // `i += 3`) jumps PAST the real island and misses it entirely.
        let escaped_then_real = r"aaaa\${{ z }} ${{ tasks.a }}";
        let isl = islands(escaped_then_real);
        assert_eq!(isl.len(), 1, "only the real (un-escaped) island: {isl:?}");
        let real_open = escaped_then_real.find("${{ tasks").expect("real open");
        assert_eq!(isl[0].0, real_open + 3, "the real island body start");
        // the body slice is exactly the real island's inner text, proving the
        // skip-advance landed at the real `${{`, not somewhere past it.
        assert_eq!(
            &escaped_then_real[isl[0].0..isl[0].1],
            " tasks.a ",
            "the real island body, reached after skipping the escaped one"
        );
        // and the ref inside that real island is found by the offset scan.
        let kw = escaped_then_real.find("tasks.a").expect("ref");
        assert_eq!(
            template_task_target(escaped_then_real, kw + "tasks.".len()),
            Some("a".to_owned()),
            "the ref in the real island resolves after the escaped one"
        );
    }

    fn span_pt(at: u32) -> Span {
        Span::point(FileId::new(0), nika_schema::ByteOffset::new(at))
    }

    fn span_range(start: u32, end: u32) -> Span {
        Span::new(
            FileId::new(0),
            nika_schema::ByteOffset::new(start),
            nika_schema::ByteOffset::new(end),
        )
    }

    #[test]
    fn after_target_resolves_to_the_exact_full_id_range() {
        let yaml = "nika: w\ntasks:\n  extract:\n    exec: { command: [\"x\"] }\n  save:\n    after: { extract: success }\n    exec: { command: [\"y\"] }\n";
        // cursor on the `extract` target inside the after: map
        let dep_offset = yaml.rfind("extract").expect("after target") + 1;
        let loc = definition(&uri(), yaml, dep_offset).expect("resolves");
        // target is the FIRST `extract` (the id), widened to the WHOLE token:
        // the id starts at line 3 char 8 and is 7 bytes long → char 15.
        let index = LineIndex::new(yaml);
        let id_byte = yaml.find("extract").expect("id");
        assert_eq!(
            loc.range,
            Range::new(
                index.position(id_byte),
                index.position(id_byte + "extract".len())
            ),
            "the full id token range, not a zero-width point and not shifted"
        );
        assert_eq!(loc.uri, uri(), "the location carries the document URI");
    }

    #[test]
    fn after_target_resolves_anywhere_on_the_token_but_not_one_byte_outside() {
        // The target token is widened from the parser's POINT span. The
        // cursor resolves on the FIRST byte through the LAST byte of
        // `extract`, but NOT on the space one byte before nor the `:` one
        // byte after.
        let yaml = "nika: w\ntasks:\n  extract:\n    exec: { command: [\"x\"] }\n  save:\n    after: { extract: success }\n    exec: { command: [\"y\"] }\n";
        let tok = yaml.rfind("extract").expect("after target");
        // one byte BEFORE the token (the space after `{`): no resolution.
        assert!(
            definition(&uri(), yaml, tok - 1).is_none(),
            "the space before the target is outside"
        );
        // the FIRST byte of the token: resolves.
        assert!(
            definition(&uri(), yaml, tok).is_some(),
            "first byte resolves"
        );
        // the LAST byte of the token: resolves.
        assert!(
            definition(&uri(), yaml, tok + "extract".len() - 1).is_some(),
            "last byte resolves"
        );
        // one byte PAST the token (the `:`): no resolution.
        assert!(
            definition(&uri(), yaml, tok + "extract".len()).is_none(),
            "the `:` just past the token is outside"
        );
    }

    #[test]
    fn template_tasks_ref_resolves_to_the_exact_full_id_range() {
        let yaml = "nika: w\ntasks:\n  extract:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n  use_it:\n    with:\n      article: \"${{ tasks.extract.output }}\"\n    exec: { command: [\"echo\", \"${{ with.article }}\"] }\n";
        // cursor inside `tasks.extract` in the binding's template
        let ref_at = yaml.find("tasks.extract").expect("tpl ref") + "tasks.ex".len();
        let loc = definition(&uri(), yaml, ref_at).expect("resolves");
        let index = LineIndex::new(yaml);
        let id_byte = yaml.find("extract").expect("id");
        assert_eq!(
            loc.range,
            Range::new(
                index.position(id_byte),
                index.position(id_byte + "extract".len())
            ),
            "the template ref jumps to the full id token range"
        );
    }

    #[test]
    fn template_ref_resolves_in_the_second_island_too() {
        // Two `${{ … }}` islands · the cursor in the SECOND island's
        // `tasks.b` resolves to `b` (the islands loop must continue past the
        // first island, and `find_close` must close each correctly).
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n  b:\n    exec: { command: [\"x\"] }\n  c:\n    with:\n      first: \"${{ tasks.a.output }}\"\n      second: \"${{ tasks.b.output }}\"\n    exec: { command: [\"x\"] }\n";
        let second = yaml.rfind("tasks.b").expect("2nd island ref") + "tasks.".len();
        let loc = definition(&uri(), yaml, second).expect("resolves in island 2");
        let index = LineIndex::new(yaml);
        let b_id = yaml.find("\n  b:").map(|p| p + 3).expect("b id");
        assert_eq!(
            loc.range.start,
            index.position(b_id),
            "the second island's `tasks.b` jumps to the `b` id"
        );
    }

    #[test]
    fn template_ref_keyword_byte_resolves_through_id_end() {
        // The cursor on the `tasks.` keyword itself (not just the id) also
        // resolves — `template_task_target` accepts `kw_at..id_end`. The byte
        // one PAST the id end does NOT (it's `.output`, outside the id).
        let yaml = "nika: w\ntasks:\n  extract:\n    exec: { command: [\"x\"] }\n  u:\n    exec: { command: [\"${{ tasks.extract.output }}\"] }\n";
        let kw = yaml.find("tasks.extract").expect("ref");
        // cursor on the `t` of `tasks.` resolves.
        assert!(
            definition(&uri(), yaml, kw).is_some(),
            "the `tasks.` keyword resolves"
        );
        // cursor at the `.` after `extract` (one past the id) does NOT.
        let dot_after = kw + "tasks.extract".len();
        assert_eq!(
            yaml.as_bytes().get(dot_after).copied(),
            Some(b'.'),
            "the byte after the id is the `.output` dot"
        );
        assert!(
            definition(&uri(), yaml, dot_after).is_none(),
            "one byte past the id (the `.`) is outside the id ref"
        );
    }

    #[test]
    fn token_span_contains_point_span_widens_to_token_len() {
        // A POINT span (start == end) is widened to [start, start+len). The
        // cursor resolves at start .. start+len-1, NOT at start+len.
        let pt = span_pt(10);
        assert!(token_span_contains(pt, 3, 10), "first byte of the token");
        assert!(token_span_contains(pt, 3, 12), "last byte (10+3-1)");
        assert!(!token_span_contains(pt, 3, 13), "one past the 3-byte token");
        assert!(!token_span_contains(pt, 3, 9), "one byte before the token");
        // a ZERO-length token still owns its single start byte (the
        // `end.max(start+1)` floor), so the cursor on `start` resolves.
        assert!(
            token_span_contains(span_pt(5), 0, 5),
            "zero-len token owns its start byte"
        );
        assert!(
            !token_span_contains(span_pt(5), 0, 6),
            "but not the next byte"
        );
    }

    #[test]
    fn token_span_contains_real_range_uses_its_own_end() {
        // A REAL range span (end > start) uses its own end, NOT start+len.
        // Range [4, 9): resolves at 4..8, not at 9.
        let rng = span_range(4, 9);
        assert!(token_span_contains(rng, 99, 4), "range start");
        assert!(token_span_contains(rng, 99, 8), "last byte of the range");
        assert!(
            !token_span_contains(rng, 99, 9),
            "the range end is exclusive (uses span end, not start+len)"
        );
        assert!(!token_span_contains(rng, 99, 3), "before the range");
    }

    #[test]
    fn token_range_widens_point_and_keeps_real_range() {
        let index = LineIndex::new("abcdefghij");
        // POINT span at byte 2, token len 4 → widened to [2, 6).
        assert_eq!(
            token_range(&index, span_pt(2), 4),
            Range::new(index.position(2), index.position(6)),
            "point span widened to [start, start+len)"
        );
        // REAL range [2, 5): uses its own end, ignoring token_len.
        assert_eq!(
            token_range(&index, span_range(2, 5), 99),
            Range::new(index.position(2), index.position(5)),
            "real range keeps its own end, not start+len"
        );
    }

    #[test]
    fn cursor_not_on_a_reference_returns_none() {
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n";
        // cursor on the `command` keyword — not a task reference
        let cmd_at = yaml.find("command").expect("kw");
        assert!(definition(&uri(), yaml, cmd_at).is_none());
    }

    #[test]
    fn ref_to_undefined_task_returns_none() {
        // an `after:` entry naming a ghost — no id span to jump to
        let yaml = "nika: w\ntasks:\n  a:\n    after: { ghost: success }\n    exec: { command: [\"x\"] }\n";
        let ghost_at = yaml.find("ghost").expect("ref") + 1;
        assert!(definition(&uri(), yaml, ghost_at).is_none());
    }

    #[test]
    fn unparseable_returns_none() {
        let yaml = "nika: v1\nworkflow: [bad\n";
        assert!(definition(&uri(), yaml, 0).is_none());
    }

    #[test]
    fn islands_handles_escape_and_quotes() {
        // \${{ is escaped (no island); a }} inside quotes does not close.
        let text = r"a \${{ not }} b ${{ x == '}}' }} c";
        let found = islands(text);
        assert_eq!(found.len(), 1, "one real island: {found:?}");
        // EXACT bounds: the live island body is just past `${{` to the real
        // closing `}}` (the quoted `}}` does NOT close it). `${{` opens at
        // byte 16 → body starts at 19; the closing `}}` is at byte 30.
        let open = text.find("${{ x").expect("real island open");
        assert_eq!(found[0].0, open + 3, "body starts just past `${{`");
        let real_close = text.rfind("}}").expect("the real close");
        assert_eq!(found[0].1, real_close, "ends at the unquoted `}}`");
    }

    #[test]
    fn islands_finds_exact_bounds_of_a_simple_island() {
        let text = "x ${{ tasks.a }} y";
        let found = islands(text);
        assert_eq!(found.len(), 1, "one island");
        let open = text.find("${{").expect("open");
        assert_eq!(found[0].0, open + 3, "body starts just past `${{`");
        assert_eq!(found[0].1, text.find("}}").expect("close"), "ends at `}}`");
        // the body slice is exactly the inner text.
        assert_eq!(
            &text[found[0].0..found[0].1],
            " tasks.a ",
            "the island body"
        );
    }

    #[test]
    fn islands_finds_both_of_two_islands_in_order() {
        // The scan loop must advance PAST the first island's close and find
        // the second (the `i = close + 2` step + the loop `i < len`).
        let text = "${{ a }}${{ bb }}";
        let found = islands(text);
        assert_eq!(found.len(), 2, "two adjacent islands: {found:?}");
        assert_eq!(found[0], (3, 6), "first island body [3,6)");
        assert_eq!(found[1], (11, 15), "second island body [11,15)");
    }

    #[test]
    fn islands_unterminated_open_yields_nothing() {
        // An `${{` with no closing `}}` is not an island (find_close → None).
        let text = "echo ${{ tasks.a";
        assert!(islands(text).is_empty(), "unterminated open → no island");
    }

    #[test]
    fn double_backslash_before_island_is_a_real_island() {
        // `\\${{` = an escaped backslash THEN a real island (even run).
        let text = r"echo \\${{ tasks.foo }}";
        let found = islands(text);
        assert_eq!(
            found.len(),
            1,
            "even backslash run → real island: {found:?}"
        );
        // and it resolves the ref inside it (proves the even-run path keeps
        // the island live, exercising backslash_run_is_odd's `% 2` parity).
        let yaml =
            "nika: w\ntasks:\n  foo:\n    exec: { command: [\"x\", \"${{ tasks.foo }}\"] }\n";
        let at = yaml.rfind("tasks.foo").expect("ref") + "tasks.".len();
        assert!(
            definition(&uri(), yaml, at).is_some(),
            "ref inside a real island resolves"
        );
    }

    #[test]
    fn odd_backslash_run_escapes_the_island() {
        // A single `\${{` (ODD run) is an escaped literal — NOT an island.
        let single = r"a \${{ tasks.x }} b";
        assert!(islands(single).is_empty(), "odd run escapes: no island");
        // THREE backslashes (still odd) also escape.
        let triple = r"a \\\${{ tasks.x }} b";
        assert!(
            islands(triple).is_empty(),
            "triple (odd) backslash run escapes too"
        );
    }

    #[test]
    fn tasks_substring_mid_identifier_is_not_a_reference() {
        // `mytasks.foo` contains `tasks.` but is not a task reference.
        let yaml =
            "nika: w\ntasks:\n  foo:\n    exec: { command: [\"echo\", \"${{ mytasks.foo }}\"] }\n";
        let at = yaml.find("mytasks.foo").expect("substr") + "mytasks.fo".len();
        assert!(
            definition(&uri(), yaml, at).is_none(),
            "mid-ident match rejected"
        );
    }

    /// `${{ inputs.X }}` jumps to the `X:` declaration under `inputs:` —
    /// and the same lane serves `secrets.` / `env.`. An undeclared
    /// member resolves nowhere (no invented target).
    #[test]
    fn member_ref_jumps_to_its_declaration() {
        let text = "nika: w\nconst:\n  city: \"paris\"\ninputs:\n  REGION: { type: string, required: false, default: \"eu\" }\nsecrets:\n  api_key:\n    source: env\n    key: K\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ const.city }}\", \"${{ inputs.REGION }}\", \"${{ secrets.api_key }}\"] }\n";
        let uri: Uri = "file:///w.nika.yaml".parse().expect("uri");
        let decl_line = |needle: &str| {
            let at = text.find(needle).expect(needle);
            u32::try_from(text[..at].matches('\n').count()).expect("line fits")
        };
        for (ref_needle, decl_needle) in [
            ("const.city", "  city:"),
            ("inputs.REGION", "  REGION:"),
            ("secrets.api_key", "  api_key:"),
        ] {
            let at = text.find(ref_needle).expect(ref_needle) + ref_needle.len() - 2;
            let loc = definition(&uri, text, at).expect(ref_needle);
            assert_eq!(
                loc.range.start.line,
                decl_line(decl_needle),
                "{ref_needle} lands on its declaration line"
            );
        }
        // undeclared member → nothing
        let at = text.find("const.city").expect("ref");
        let miss = text.replace("const.city", "const.ghost");
        assert!(definition(&uri, &miss, at + 7).is_none());
    }
}
