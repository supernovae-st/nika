// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `cel-subset/0.1` lexer — source bytes → a token stream.
//!
//! Tokenizes exactly the grammar's terminals (03-dag §formal-grammar):
//! identifiers, the int/float/string/bool/null literals, the operator
//! set, and the punctuation. `true`/`false`/`null`/`in` are RESERVED
//! words (never identifiers · grammar note). String escapes are the
//! closed set `\\ \' \" \n \t`. Every token carries its byte span.

use crate::error::CelError;

/// One lexical token + its byte span into the source.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub kind: Tok,
    pub span: (usize, usize),
}

/// The token kinds (grammar terminals).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    True,
    False,
    Null,
    // operators / punctuation
    AndAnd,   // &&
    OrOr,     // ||
    Bang,     // !
    EqEq,     // ==
    NotEq,    // !=
    Lt,       // <
    Le,       // <=
    Gt,       // >
    Ge,       // >=
    In,       // in (keyword)
    Question, // ?
    Colon,    // :
    Dot,      // .
    Comma,    // ,
    LParen,   // (
    RParen,   // )
    LBrack,   // [
    RBrack,   // ]
}

/// Tokenize `src` (the inside of `${{ }}`, already trimmed) into the
/// token stream. NIKA-VAR-005 on any byte that is not part of a valid
/// token.
pub(crate) fn lex(src: &str) -> Result<Vec<Token>, CelError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let kind = match b {
            b'&' | b'|' => lex_pair(bytes, &mut i, b)?,
            b'=' => lex_eq(bytes, &mut i)?,
            b'!' => lex_bang(bytes, &mut i),
            b'<' => lex_angle(bytes, &mut i, Tok::Lt, Tok::Le),
            b'>' => lex_angle(bytes, &mut i, Tok::Gt, Tok::Ge),
            b'?' => one(&mut i, Tok::Question),
            b':' => one(&mut i, Tok::Colon),
            // `.` is always the field-access dot. A float keeps its dot
            // INSIDE `lex_number` (`1.5`); the grammar requires a leading
            // digit, so a bare `.5` is a static error (the fallthrough).
            b'.' => one(&mut i, Tok::Dot),
            b',' => one(&mut i, Tok::Comma),
            b'(' => one(&mut i, Tok::LParen),
            b')' => one(&mut i, Tok::RParen),
            b'[' => one(&mut i, Tok::LBrack),
            b']' => one(&mut i, Tok::RBrack),
            b'\'' | b'"' => lex_string(src, &mut i)?,
            b'-' | b'0'..=b'9' => lex_number(bytes, &mut i)?,
            b if b.is_ascii_alphabetic() || b == b'_' => lex_ident_or_keyword(bytes, &mut i),
            // Binary arithmetic is outside the cel-subset/0.1 boolean-guard
            // grammar — teach WHERE it belongs (a `nika:jq` task) with the
            // SAME voice as the check-side lexer (`nika-tmpl` · one-voice
            // parity: a workflow run without `check` first must learn the
            // identical lesson). `-` opens a numeric literal above; its
            // binary form lands in `lex_number`'s no-digit path.
            b'+' | b'*' | b'/' | b'%' => {
                return Err(CelError::static_err(
                    format!(
                        "arithmetic operator `{}` — `${{{{ … }}}}` is a boolean guard, not a \
                         calculator (cel-subset/0.1): compute the value in a `nika:jq` task \
                         and gate on `tasks.<id>.output`",
                        b as char
                    ),
                    (start, start + 1),
                ));
            }
            other => {
                return Err(CelError::static_err(
                    format!("unexpected character `{}`", other as char),
                    (start, start + 1),
                ));
            }
        };
        tokens.push(Token {
            kind,
            span: (start, i),
        });
    }
    Ok(tokens)
}

fn one(i: &mut usize, tok: Tok) -> Tok {
    *i += 1;
    tok
}

/// `&&` / `||` — the doubled boolean operators (a single `&`/`|` is an
/// error · no bitwise in the subset).
fn lex_pair(bytes: &[u8], i: &mut usize, b: u8) -> Result<Tok, CelError> {
    if bytes.get(*i + 1) == Some(&b) {
        *i += 2;
        Ok(if b == b'&' { Tok::AndAnd } else { Tok::OrOr })
    } else {
        Err(CelError::static_err(
            format!(
                "`{0}` is not an operator — did you mean `{0}{0}`?",
                b as char
            ),
            (*i, *i + 1),
        ))
    }
}

fn lex_eq(bytes: &[u8], i: &mut usize) -> Result<Tok, CelError> {
    if bytes.get(*i + 1) == Some(&b'=') {
        *i += 2;
        Ok(Tok::EqEq)
    } else {
        Err(CelError::static_err(
            "`=` is not an operator — did you mean `==`?",
            (*i, *i + 1),
        ))
    }
}

fn lex_bang(bytes: &[u8], i: &mut usize) -> Tok {
    if bytes.get(*i + 1) == Some(&b'=') {
        *i += 2;
        Tok::NotEq
    } else {
        *i += 1;
        Tok::Bang
    }
}

fn lex_angle(bytes: &[u8], i: &mut usize, bare: Tok, with_eq: Tok) -> Tok {
    if bytes.get(*i + 1) == Some(&b'=') {
        *i += 2;
        with_eq
    } else {
        *i += 1;
        bare
    }
}

/// Identifier or a reserved keyword (`true`/`false`/`null`/`in`).
fn lex_ident_or_keyword(bytes: &[u8], i: &mut usize) -> Tok {
    let start = *i;
    while *i < bytes.len() {
        let c = bytes[*i];
        if c.is_ascii_alphanumeric() || c == b'_' {
            *i += 1;
        } else {
            break;
        }
    }
    // Identifier bytes are ASCII by construction (the loop only advances
    // over ascii alnum/underscore).
    let word = std::str::from_utf8(&bytes[start..*i]).unwrap_or("");
    match word {
        "true" => Tok::True,
        "false" => Tok::False,
        "null" => Tok::Null,
        "in" => Tok::In,
        _ => Tok::Ident(word.to_owned()),
    }
}

/// An INT or FLOAT literal (`-?[0-9]+` · `-?[0-9]+\.[0-9]+`).
fn lex_number(bytes: &[u8], i: &mut usize) -> Result<Tok, CelError> {
    let start = *i;
    if bytes.get(*i) == Some(&b'-') {
        *i += 1;
    }
    let digits_start = *i;
    while bytes.get(*i).is_some_and(u8::is_ascii_digit) {
        *i += 1;
    }
    let mut is_float = false;
    if bytes.get(*i) == Some(&b'.') && bytes.get(*i + 1).is_some_and(u8::is_ascii_digit) {
        is_float = true;
        *i += 1; // the dot
        while bytes.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
    }
    if *i == digits_start {
        // A `-` that never opened a numeric literal is a binary-minus
        // attempt — the same arithmetic teaching as the dispatch arm.
        return Err(CelError::static_err(
            "arithmetic operator `-` — `${{ … }}` is a boolean guard, not a \
             calculator (cel-subset/0.1): compute the value in a `nika:jq` task \
             and gate on `tasks.<id>.output`",
            (start, (*i).max(start + 1)),
        ));
    }
    let text = std::str::from_utf8(&bytes[start..*i]).unwrap_or("");
    if is_float {
        text.parse::<f64>()
            .map(Tok::Float)
            .map_err(|_| CelError::static_err("invalid float literal", (start, *i)))
    } else {
        text.parse::<i64>()
            .map(Tok::Int)
            .map_err(|_| CelError::static_err("integer literal out of range", (start, *i)))
    }
}

/// A single- or double-quoted string with the closed escape set
/// (`\\ \' \" \n \t`). An unknown escape or an unterminated string is
/// NIKA-VAR-005.
fn lex_string(src: &str, i: &mut usize) -> Result<Tok, CelError> {
    let bytes = src.as_bytes();
    let start = *i;
    let quote = bytes[*i];
    *i += 1;
    let mut out = String::new();
    while *i < bytes.len() {
        let c = bytes[*i];
        if c == quote {
            *i += 1;
            return Ok(Tok::Str(out));
        }
        if c == b'\\' {
            let esc = bytes.get(*i + 1).copied();
            let ch = match esc {
                Some(b'\\') => '\\',
                Some(b'\'') => '\'',
                Some(b'"') => '"',
                Some(b'n') => '\n',
                Some(b't') => '\t',
                Some(other) => {
                    return Err(CelError::static_err(
                        format!("unknown escape `\\{}`", other as char),
                        (*i, *i + 2),
                    ));
                }
                None => {
                    return Err(CelError::static_err(
                        "trailing backslash in string",
                        (*i, *i + 1),
                    ));
                }
            };
            out.push(ch);
            *i += 2;
            continue;
        }
        // Copy one WHOLE UTF-8 char. ASCII quote/backslash were handled
        // above, so a byte ≥ 0x80 here is a multibyte lead — decode the
        // full char (the source is a valid-UTF-8 `&str`, and `*i` is on a
        // char boundary) instead of casting each byte (`c as char` is a
        // Latin-1 decode that mangles every multibyte codepoint).
        let ch = src
            .get(*i..)
            .and_then(|rest| rest.chars().next())
            .unwrap_or(char::REPLACEMENT_CHARACTER);
        out.push(ch);
        *i += ch.len_utf8();
    }
    Err(CelError::static_err(
        "unterminated string literal",
        (start, *i),
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_identifiers_and_reserved_words() {
        assert_eq!(
            kinds("vars.topic in tasks"),
            vec![
                Tok::Ident("vars".into()),
                Tok::Dot,
                Tok::Ident("topic".into()),
                Tok::In,
                Tok::Ident("tasks".into()),
            ]
        );
        // The four reserved words are NOT identifiers.
        assert_eq!(
            kinds("true false null"),
            vec![Tok::True, Tok::False, Tok::Null]
        );
    }

    #[test]
    fn lexes_the_operator_set() {
        assert_eq!(
            kinds("== != <= >= < > && || ! ? :"),
            vec![
                Tok::EqEq,
                Tok::NotEq,
                Tok::Le,
                Tok::Ge,
                Tok::Lt,
                Tok::Gt,
                Tok::AndAnd,
                Tok::OrOr,
                Tok::Bang,
                Tok::Question,
                Tok::Colon,
            ]
        );
    }

    #[test]
    fn lexes_numbers_int_and_float_and_negative() {
        assert_eq!(kinds("42"), vec![Tok::Int(42)]);
        assert_eq!(kinds("-7"), vec![Tok::Int(-7)]);
        assert_eq!(kinds("1.5"), vec![Tok::Float(1.5)]);
        // A bare `.` is the field dot, never a float start.
        assert_eq!(
            kinds("a.b"),
            vec![Tok::Ident("a".into()), Tok::Dot, Tok::Ident("b".into())]
        );
    }

    #[test]
    fn lexes_strings_with_the_closed_escape_set() {
        assert_eq!(kinds(r"'hello'"), vec![Tok::Str("hello".into())]);
        assert_eq!(kinds(r#""a\nb""#), vec![Tok::Str("a\nb".into())]);
        assert_eq!(kinds(r"'it\'s'"), vec![Tok::Str("it's".into())]);
        assert_eq!(kinds(r"'a\\b'"), vec![Tok::Str("a\\b".into())]);
    }

    #[test]
    fn single_amp_is_a_static_error_not_a_token() {
        let err = lex("a & b").expect_err("single & must fail");
        assert_eq!(err.spec_code(), "NIKA-VAR-005");
    }

    #[test]
    fn bare_eq_suggests_double() {
        let err = lex("a = b").expect_err("single = must fail");
        assert_eq!(err.spec_code(), "NIKA-VAR-005");
        assert!(err.message().contains("=="));
    }

    #[test]
    fn unknown_escape_and_unterminated_string_are_static() {
        assert_eq!(
            lex(r"'a\xb'").expect_err("bad escape").spec_code(),
            "NIKA-VAR-005"
        );
        assert_eq!(
            lex("'unclosed").expect_err("unterminated").spec_code(),
            "NIKA-VAR-005"
        );
    }

    #[test]
    fn unexpected_character_is_static() {
        assert_eq!(lex("a @ b").expect_err("@").spec_code(), "NIKA-VAR-005");
    }

    /// `kinds()` discards spans; assert them directly so every token's
    /// `(start, end)` byte offset is pinned — the lexer's advance
    /// arithmetic (`*i += N`, the `start + 1` in error spans) is otherwise
    /// invisible to a kind-only assertion.
    #[test]
    fn tokens_carry_their_exact_byte_spans() {
        let spans = |src: &str| -> Vec<(usize, usize)> {
            lex(src).unwrap().into_iter().map(|t| t.span).collect()
        };
        // ident · `==` (two-byte op) · int, each separated by one space.
        assert_eq!(spans("ab == 12"), vec![(0, 2), (3, 5), (6, 8)]);
        // single-byte op `!` then two-byte `!=` then `<=` then bare `<`.
        assert_eq!(spans("! != <= <"), vec![(0, 1), (2, 4), (5, 7), (8, 9)]);
        // a negative int and a float keep their dot/sign inside one span.
        assert_eq!(spans("-7 1.5"), vec![(0, 2), (3, 6)]);
        // a string spans its quotes; the `\n` escape is two source bytes.
        assert_eq!(spans(r"'a\nb'"), vec![(0, 6)]);
    }

    #[test]
    fn the_unexpected_char_error_span_is_the_single_byte() {
        // `(start, start + 1)` at lexer.rs — a one-byte cursor on the `@`.
        let err = lex("a @ b").expect_err("@ is not a token");
        assert_eq!(err.span(), (2, 3));
    }

    #[test]
    fn operator_and_escape_error_spans_are_exact() {
        // A single `&` / `=` → a one-byte span on the stray operator
        // (lex_pair / lex_eq `(*i, *i + 1)`).
        assert_eq!(lex("a & b").expect_err("&").span(), (2, 3));
        assert_eq!(lex("a = b").expect_err("=").span(), (2, 3));
        // An unknown escape spans the two bytes `\x` (`(*i, *i + 2)`).
        assert_eq!(lex(r"'a\xb'").expect_err("bad escape").span(), (2, 4));
        // A trailing backslash is the single `\` byte (`(*i, *i + 1)`).
        assert_eq!(lex(r"'x\").expect_err("trailing bslash").span(), (2, 3));
    }

    #[test]
    fn string_literals_preserve_multibyte_utf8() {
        // Decode WHOLE UTF-8 chars, never Latin-1-mangle each byte
        // (`'héllo ☃'` must NOT become `'hÃ©llo â☃'`).
        assert_eq!(kinds("'héllo ☃'"), vec![Tok::Str("héllo ☃".into())]);
        assert_eq!(
            kinds(r#""café · 日本""#),
            vec![Tok::Str("café · 日本".into())]
        );
        // Escapes still compose with multibyte content.
        assert_eq!(kinds(r"'é\n☃'"), vec![Tok::Str("é\n☃".into())]);
    }

    #[test]
    fn binary_arithmetic_teaches_jq() {
        // One-voice parity with the check-side lexer (`nika-tmpl`): every
        // arithmetic operator surfaces the boolean-guard teaching (routes
        // to `nika:jq`), never a raw `unexpected character` — a run
        // launched without `check` first learns the identical lesson.
        for expr in ["a + b", "a * b", "a / b", "a % b", "a - b"] {
            let err = lex(expr).expect_err(expr);
            let msg = err.to_string();
            assert!(
                msg.contains("arithmetic operator") && msg.contains("nika:jq"),
                "`{expr}` → {msg}"
            );
        }
        // A negative numeric literal is NOT arithmetic — it still lexes.
        assert!(lex("-5 < 0").is_ok(), "negative literal must lex");
    }
}
