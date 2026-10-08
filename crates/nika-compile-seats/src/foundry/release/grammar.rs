// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The text and path grammar every admitted value of a knowledge release follows, shared by
//! profiles r1 and r2 and by their producer (the r1 contract §5, §6, §9; unchanged in r2): the
//! closed table of forbidden code points, a line, a text, a file's text, a safe relative path,
//! the hex digests, the tokens and the id grammar. A profile chooses the byte bounds of its
//! texts and files; the predicates are the same.

use serde_json::Value;

/// The code points no admitted text carries (§9.3): a closed table, not a Unicode category. TAB
/// and LF are judged apart ([`printable`]), and so is every noncharacter `U+xFFFE`/`U+xFFFF`.
/// The presentation selectors U+FE0E and U+FE0F are admitted.
pub const FORBIDDEN: [(char, char); 31] = [
    ('\u{0}', '\u{8}'),
    ('\u{B}', '\u{1F}'),
    ('\u{7F}', '\u{9F}'),
    ('\u{AD}', '\u{AD}'),
    ('\u{34F}', '\u{34F}'),
    ('\u{600}', '\u{605}'),
    ('\u{61C}', '\u{61C}'),
    ('\u{6DD}', '\u{6DD}'),
    ('\u{70F}', '\u{70F}'),
    ('\u{890}', '\u{891}'),
    ('\u{8E2}', '\u{8E2}'),
    ('\u{115F}', '\u{1160}'),
    ('\u{17B4}', '\u{17B5}'),
    ('\u{180B}', '\u{180F}'),
    ('\u{200B}', '\u{200F}'),
    ('\u{2028}', '\u{202E}'),
    ('\u{2060}', '\u{206F}'),
    ('\u{3164}', '\u{3164}'),
    ('\u{E000}', '\u{F8FF}'),
    ('\u{FDD0}', '\u{FDEF}'),
    ('\u{FE00}', '\u{FE0D}'),
    ('\u{FEFF}', '\u{FEFF}'),
    ('\u{FFA0}', '\u{FFA0}'),
    ('\u{FFF9}', '\u{FFFF}'),
    ('\u{110BD}', '\u{110BD}'),
    ('\u{110CD}', '\u{110CD}'),
    ('\u{13430}', '\u{1343F}'),
    ('\u{1BCA0}', '\u{1BCA3}'),
    ('\u{1D173}', '\u{1D17A}'),
    ('\u{E0000}', '\u{E0FFF}'),
    ('\u{F0000}', '\u{10FFFF}'),
];

/// A code point a presented text may carry: TAB and LF only where `multiline`, never a
/// noncharacter, never one of [`FORBIDDEN`]. (Surrogates cannot occur in a `str`.)
#[must_use]
pub fn printable(c: char, multiline: bool) -> bool {
    if c == '\t' || c == '\n' {
        return multiline;
    }
    let point = u32::from(c);
    point & 0xFFFE != 0xFFFE
        && !FORBIDDEN
            .iter()
            .any(|(low, high)| (*low..=*high).contains(&c))
}

/// Every code point is `White_Space` (Rust's `char::is_whitespace`, §9.2): the empty text included.
#[must_use]
pub fn blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// One line: 1 to 300 admitted code points, no TAB or LF, not blank.
#[must_use]
pub fn line(text: &str) -> bool {
    !blank(text) && text.chars().count() <= 300 && text.chars().all(|c| printable(c, false))
}

/// Text of at most `max` bytes of admitted code points (TAB and LF too), blank only where
/// `maybe` allows.
#[must_use]
pub fn text(text: &str, max: usize, maybe: bool) -> bool {
    (maybe || !blank(text)) && text.len() <= max && text.chars().all(|c| printable(c, true))
}

/// A file a release presents (a body, the notices): UTF-8, 1 to `max` bytes of admitted code
/// points (TAB and LF too), not blank.
#[must_use]
pub fn file_text(bytes: &[u8], max: usize) -> bool {
    (1..=max).contains(&bytes.len())
        && std::str::from_utf8(bytes)
            .is_ok_and(|text| !blank(text) && text.chars().all(|c| printable(c, true)))
}

/// A relative POSIX path with no empty, `.` or `..` segment, no backslash, NUL, absolute or
/// drive form.
#[must_use]
pub fn safe_relative(path: &str) -> bool {
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains('\0')
        && !path.starts_with('/')
        && !drive
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

/// A lowercase sha256.
#[must_use]
pub fn hex64(value: &Value) -> bool {
    value.as_str().is_some_and(|s| lower_hex(s, 64))
}

/// A specification commit: 40 or 64 lowercase hex.
#[must_use]
pub fn spec_sha(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| lower_hex(s, 40) || lower_hex(s, 64))
}

/// Exactly `len` lowercase hex digits.
#[must_use]
pub fn lower_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// A policy id: `[a-z0-9][a-z0-9-]{0,63}`.
#[must_use]
pub fn policy_token(text: &str) -> bool {
    token(text, 64, |c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
    }) && text
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// A token: a first character that is an ASCII letter or digit, at most `max` characters, each
/// admitted by `rest`.
#[must_use]
pub fn token(text: &str, max: usize, rest: impl Fn(char) -> bool) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && text.chars().count() <= max
        && text.chars().all(rest)
}

/// A row id's name after its prefix: `[A-Za-z0-9][A-Za-z0-9._:/@+-]{0,199}`.
#[must_use]
pub fn id_name(name: &str) -> bool {
    token(name, 200, |c| {
        c.is_ascii_alphanumeric() || "._:/@+-".contains(c)
    })
}

/// `knowledge/families.jsonl` → `families`.
#[must_use]
pub fn stem(path: &str) -> String {
    base_name(path).trim_end_matches(".jsonl").to_owned()
}

/// `knowledge/families.jsonl` → `families.jsonl`.
#[must_use]
pub fn base_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}
