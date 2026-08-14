// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-tmpl` — the `${{ … }}` template-island lexer, as pure data.
//!
//! The ONE scanner the static checker (`nika-schema`) and the runtime resolver
//! (`nika-runtime`) share. Before this crate the island scanner was
//! hand-duplicated in both, kept in sync by comments — and it drifted once in
//! production (a `\${{` literal-escape the checker skipped but the runtime
//! resolved · check-passed / run-broke, 2026-06-18). Recent IFC work
//! (arXiv:2606.26479) makes the reason structural: for a deterministic
//! out-of-band security gate the **trusted computing base is provenance / label
//! assignment**, and identical `${{ }}` semantics between the checker and the
//! resolver IS that TCB. One lexer ⇒ parity by construction, not by discipline.
//!
//! **The scanner stays AST-free; the grammar lives one floor up.** The
//! root of this crate finds island *boundaries* and returns byte-spans +
//! the raw body slice — it does NOT parse the body. Since 2026-07-10 the
//! body's grammar ALSO lives here, as the [`expression`] module (lexer ·
//! AST · parser · reference walker · template renderer), descended from
//! `nika-schema` when that crate hit the 15k crate-size wall (the
//! trace→dap precedent): the scanner and the language it scans are one
//! home, and `nika_schema::expression` re-exports the module verbatim.
//! Consumers of the bare scanner (the runtime's `nika-cel`) keep
//! depending on exactly what they did — the root API is unchanged.
//!
//! Layer **L0** — pure, zero I/O, zero async, zero dependencies.

#![forbid(unsafe_code)]
// The descended expression tests keep the schema crate's test-only
// allowances (fixtures expect/panic on impossible states — same block,
// same reasons).
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
    )
)]

/// A located `${{ … }}` island: byte offsets into the source string plus the
/// raw (untrimmed) body slice between `${{` and `}}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct IslandSpan<'a> {
    /// Byte offset of the leading `$` of `${{`.
    pub start: usize,
    /// The slice BETWEEN `${{` and `}}` — untrimmed (callers trim as needed).
    pub body: &'a str,
    /// Byte offset of the first body byte (`start + 3`).
    pub body_start: usize,
    /// Byte offset ONE PAST the closing `}}`.
    pub end: usize,
}

impl<'a> IslandSpan<'a> {
    /// Construct from the four resolved offsets + body slice (forward-compat
    /// invariant #19 — a `new` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(start: usize, body: &'a str, body_start: usize, end: usize) -> Self {
        Self {
            start,
            body,
            body_start,
            end,
        }
    }
}

/// The one failure mode: an opener with no quote-aware closer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScanError {
    /// A `${{` at `offset` was never closed by a `}}` (respecting string
    /// literals in the body). Consumers map this to their own domain error.
    Unterminated {
        /// Byte offset of the unterminated opener.
        offset: usize,
    },
}

impl ScanError {
    /// The byte offset in the source where the scan failed. Total over all
    /// variants — every lexer error has a position (a future variant that
    /// forgets one fails to compile HERE, in the defining crate, not silently
    /// at a downstream `_` arm).
    #[must_use]
    pub fn offset(&self) -> usize {
        match self {
            ScanError::Unterminated { offset } => *offset,
        }
    }
}

// Hand-written Display + Error (FCI-019 · errors are `std::error::Error`) —
// NOT via `thiserror`, which would break this crate's zero-dependency contract.
// Consumers translate to their own domain error (ExprError · RuntimeError); this
// exists so a bare `ScanError` still prints + composes like any Rust error.
impl core::fmt::Display for ScanError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ScanError::Unterminated { offset } => {
                write!(
                    f,
                    "unterminated template opener at byte {offset}: no closing delimiter"
                )
            }
        }
    }
}

impl core::error::Error for ScanError {}

// The grammar module's own //! docs carry the full story (a /// here
// would re-anchor its intra-doc links at the crate root and break them).
pub mod expression;

/// Scan every REAL (non-`\`-escaped) `${{ … }}` island, left to right.
///
/// - **Quote-aware close**: a `}}` inside a `'…'` / `"…"` body string literal
///   does NOT close the island (`${{ inputs.x == "}}" }}` is one island).
/// - **Escape**: a `\${{` is a literal, not an island (the opener is skipped;
///   the preceding backslash is the author's escape).
///
/// # Errors
/// [`ScanError::Unterminated`] on the first opener that has no closer.
pub fn scan_islands(s: &str) -> Result<Vec<IslandSpan<'_>>, ScanError> {
    let bytes = s.as_bytes();
    let mut islands = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // Byte-level probe — `i` may sit inside a multi-byte char; `${{` is
        // pure ASCII so `starts_with` is exact.
        if !bytes[i..].starts_with(b"${{") {
            i += 1;
            continue;
        }
        // `\${{` — escaped literal, not an island. Skip the opener.
        if i > 0 && bytes[i - 1] == b'\\' {
            i += 3;
            continue;
        }
        let start = i;
        let body_start = i + 3;
        let body_end =
            find_island_close(s, body_start).ok_or(ScanError::Unterminated { offset: start })?;
        let end = body_end + 2;
        islands.push(IslandSpan::new(
            start,
            &s[body_start..body_end],
            body_start,
            end,
        ));
        i = end;
    }
    Ok(islands)
}

/// Offset of the `}}` that closes an island body starting at `from`, skipping
/// string literals (`'…'` / `"…"`, honoring `\` escapes inside them). A `}}`
/// inside a literal does not close. Returns `None` if unterminated.
#[must_use]
pub fn find_island_close(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == b'\\' {
                    i += 2;
                    continue;
                }
                if b == q {
                    quote = None;
                }
                i += 1;
            }
            None => {
                if b == b'\'' || b == b'"' {
                    quote = Some(b);
                    i += 1;
                } else if b == b'}' && bytes.get(i + 1) == Some(&b'}') {
                    return Some(i);
                } else {
                    i += 1;
                }
            }
        }
    }
    None
}

/// If `s` is EXACTLY one island (only surrounding whitespace around it),
/// return its trimmed body — the type-preserving single-island case
/// (`"${{ ref }}"` resolves to the referenced VALUE, not its string form).
///
/// Conservative on quoted braces: a body containing `}}` (even inside a string
/// literal) yields `None` and the caller renders textually. This preserves the
/// runtime's historical behavior verbatim; a quote-aware widening (via
/// [`scan_islands`]) is a deliberate future change, not part of this extraction.
///
/// One vocabulary, one verdict (#511): what the quote-aware scanner
/// REFUSES, this fast path refuses too — the textual strip alone
/// accepted `${{'{{{{}}` (an unterminated quote swallowing the closer)
/// while [`scan_islands`] answered `Unterminated`; `Some` here now
/// implies the scanner finds exactly one island (property-pinned).
#[must_use]
pub fn single_island(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    let body = trimmed.strip_prefix("${{")?.strip_suffix("}}")?;
    // A second island inside would mean `}}…${{` — reject (textual).
    if body.contains("${{") || body.contains("}}") {
        return None;
    }
    match scan_islands(trimmed) {
        Ok(islands) if islands.len() == 1 => Some(body.trim()),
        _ => None,
    }
}
