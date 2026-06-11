// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tokenizer for the CEL v0.1 subset (spec `03-dag.md` EBNF terminals).
//!
//! `IDENT = [A-Za-z_][A-Za-z0-9_]*` · `INT = -?[0-9]+` ·
//! `FLOAT = -?[0-9]+\.[0-9]+` ·
//! `STRING = '…' | "…"` with escapes `\\ \' \" \n \t`.
//! Reserved words · `true` · `false` · `null` · `in`.

use super::error::ExprError;

/// A lexed token with its start offset (relative to expression start).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) offset: usize,
}

/// Token kinds of the v0.1 subset.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum TokenKind {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    True,
    False,
    Null,
    In,
    OrOr,
    AndAnd,
    Bang,
    EqEq,
    NotEq,
    Le,
    Ge,
    Lt,
    Gt,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Question,
    Colon,
    Eof,
}

impl TokenKind {
    /// Human description for error messages.
    pub(super) fn describe(&self) -> String {
        match self {
            Self::Ident(name) => format!("identifier `{name}`"),
            Self::Int(v) => format!("integer `{v}`"),
            Self::Float(v) => format!("float `{v}`"),
            Self::Str(_) => "string literal".to_owned(),
            Self::True => "`true`".to_owned(),
            Self::False => "`false`".to_owned(),
            Self::Null => "`null`".to_owned(),
            Self::In => "`in`".to_owned(),
            Self::OrOr => "`||`".to_owned(),
            Self::AndAnd => "`&&`".to_owned(),
            Self::Bang => "`!`".to_owned(),
            Self::EqEq => "`==`".to_owned(),
            Self::NotEq => "`!=`".to_owned(),
            Self::Le => "`<=`".to_owned(),
            Self::Ge => "`>=`".to_owned(),
            Self::Lt => "`<`".to_owned(),
            Self::Gt => "`>`".to_owned(),
            Self::LParen => "`(`".to_owned(),
            Self::RParen => "`)`".to_owned(),
            Self::LBracket => "`[`".to_owned(),
            Self::RBracket => "`]`".to_owned(),
            Self::Comma => "`,`".to_owned(),
            Self::Dot => "`.`".to_owned(),
            Self::Question => "`?`".to_owned(),
            Self::Colon => "`:`".to_owned(),
            Self::Eof => "end of expression".to_owned(),
        }
    }
}

/// Tokenize a full expression string.
pub(super) fn tokenize(src: &str) -> Result<Vec<Token>, ExprError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let b = bytes[i];
        match b {
            b' ' | b'\t' | b'\n' | b'\r' => {
                i += 1;
            }
            b'(' => push_simple(&mut tokens, TokenKind::LParen, start, &mut i),
            b')' => push_simple(&mut tokens, TokenKind::RParen, start, &mut i),
            b'[' => push_simple(&mut tokens, TokenKind::LBracket, start, &mut i),
            b']' => push_simple(&mut tokens, TokenKind::RBracket, start, &mut i),
            b',' => push_simple(&mut tokens, TokenKind::Comma, start, &mut i),
            b'.' => push_simple(&mut tokens, TokenKind::Dot, start, &mut i),
            b'?' => push_simple(&mut tokens, TokenKind::Question, start, &mut i),
            b':' => push_simple(&mut tokens, TokenKind::Colon, start, &mut i),
            b'|' | b'&' | b'=' | b'!' | b'<' | b'>' => {
                let (kind, len) = lex_operator(bytes, i)?;
                tokens.push(Token {
                    kind,
                    offset: start,
                });
                i += len;
            }
            b'\'' | b'"' => {
                let (value, len) = lex_string(src, i)?;
                tokens.push(Token {
                    kind: TokenKind::Str(value),
                    offset: start,
                });
                i += len;
            }
            b'-' | b'0'..=b'9' => {
                let (kind, len) = lex_number(src, i)?;
                tokens.push(Token {
                    kind,
                    offset: start,
                });
                i += len;
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                let (kind, len) = lex_ident(src, i);
                tokens.push(Token {
                    kind,
                    offset: start,
                });
                i += len;
            }
            _ => {
                // Resolve the full char for the error message (the byte
                // may start a multi-byte sequence).
                let ch = src[i..].chars().next().unwrap_or('\u{FFFD}');
                return Err(ExprError::UnexpectedChar { ch, offset: i });
            }
        }
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        offset: src.len(),
    });
    Ok(tokens)
}

fn push_simple(tokens: &mut Vec<Token>, kind: TokenKind, offset: usize, i: &mut usize) {
    tokens.push(Token { kind, offset });
    *i += 1;
}

/// Lex a (possibly 2-char) operator starting at `i`.
fn lex_operator(bytes: &[u8], i: usize) -> Result<(TokenKind, usize), ExprError> {
    let next = bytes.get(i + 1).copied();
    match (bytes[i], next) {
        (b'|', Some(b'|')) => Ok((TokenKind::OrOr, 2)),
        (b'&', Some(b'&')) => Ok((TokenKind::AndAnd, 2)),
        (b'=', Some(b'=')) => Ok((TokenKind::EqEq, 2)),
        (b'!', Some(b'=')) => Ok((TokenKind::NotEq, 2)),
        (b'<', Some(b'=')) => Ok((TokenKind::Le, 2)),
        (b'>', Some(b'=')) => Ok((TokenKind::Ge, 2)),
        (b'!', _) => Ok((TokenKind::Bang, 1)),
        (b'<', _) => Ok((TokenKind::Lt, 1)),
        (b'>', _) => Ok((TokenKind::Gt, 1)),
        (other, _) => Err(ExprError::UnexpectedChar {
            ch: other as char,
            offset: i,
        }),
    }
}

/// Lex a quoted string with the grammar's escape set (`\\ \' \" \n \t`).
fn lex_string(src: &str, start: usize) -> Result<(String, usize), ExprError> {
    let bytes = src.as_bytes();
    let quote = bytes[start];
    let mut value = String::new();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                let escaped = bytes.get(i + 1).copied();
                match escaped {
                    Some(b'\\') => value.push('\\'),
                    Some(b'\'') => value.push('\''),
                    Some(b'"') => value.push('"'),
                    Some(b'n') => value.push('\n'),
                    Some(b't') => value.push('\t'),
                    _ => {
                        return Err(ExprError::UnexpectedChar {
                            ch: '\\',
                            offset: i,
                        });
                    }
                }
                i += 2;
            }
            b if b == quote => {
                return Ok((value, i + 1 - start));
            }
            _ => {
                // Append the full char (handles multi-byte UTF-8).
                let ch = src[i..].chars().next().unwrap_or('\u{FFFD}');
                value.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    Err(ExprError::UnterminatedString { offset: start })
}

/// Lex an integer or float (`-?[0-9]+` / `-?[0-9]+\.[0-9]+`).
fn lex_number(src: &str, start: usize) -> Result<(TokenKind, usize), ExprError> {
    let bytes = src.as_bytes();
    let mut i = start;
    if bytes[i] == b'-' {
        i += 1;
        if !bytes.get(i).copied().is_some_and(|b| b.is_ascii_digit()) {
            return Err(ExprError::UnexpectedChar {
                ch: '-',
                offset: start,
            });
        }
    }
    while bytes.get(i).copied().is_some_and(|b| b.is_ascii_digit()) {
        i += 1;
    }
    // A fraction requires a digit AFTER the dot — otherwise the dot is
    // a member-access token (`tasks.x` after an int never occurs, but
    // `[0].field` indexing does).
    let is_float = bytes.get(i).copied() == Some(b'.')
        && bytes
            .get(i + 1)
            .copied()
            .is_some_and(|b| b.is_ascii_digit());
    if is_float {
        i += 1;
        while bytes.get(i).copied().is_some_and(|b| b.is_ascii_digit()) {
            i += 1;
        }
        let text = &src[start..i];
        let value = text.parse::<f64>().map_err(|_| ExprError::UnexpectedChar {
            ch: '.',
            offset: start,
        })?;
        Ok((TokenKind::Float(value), i - start))
    } else {
        let text = &src[start..i];
        let value = text.parse::<i64>().map_err(|_| ExprError::UnexpectedChar {
            ch: '-',
            offset: start,
        })?;
        Ok((TokenKind::Int(value), i - start))
    }
}

/// Lex an identifier or reserved word.
fn lex_ident(src: &str, start: usize) -> (TokenKind, usize) {
    let bytes = src.as_bytes();
    let mut i = start;
    while bytes
        .get(i)
        .copied()
        .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        i += 1;
    }
    let text = &src[start..i];
    let kind = match text {
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "null" => TokenKind::Null,
        "in" => TokenKind::In,
        _ => TokenKind::Ident(text.to_owned()),
    };
    (kind, i - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        tokenize(src)
            .expect("tokenize")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn lex_member_chain() {
        assert_eq!(
            kinds("tasks.build.status"),
            vec![
                TokenKind::Ident("tasks".into()),
                TokenKind::Dot,
                TokenKind::Ident("build".into()),
                TokenKind::Dot,
                TokenKind::Ident("status".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_operators() {
        assert_eq!(
            kinds("a == b != c <= d >= e < f > g && h || !i"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::EqEq,
                TokenKind::Ident("b".into()),
                TokenKind::NotEq,
                TokenKind::Ident("c".into()),
                TokenKind::Le,
                TokenKind::Ident("d".into()),
                TokenKind::Ge,
                TokenKind::Ident("e".into()),
                TokenKind::Lt,
                TokenKind::Ident("f".into()),
                TokenKind::Gt,
                TokenKind::Ident("g".into()),
                TokenKind::AndAnd,
                TokenKind::Ident("h".into()),
                TokenKind::OrOr,
                TokenKind::Bang,
                TokenKind::Ident("i".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_reserved_words() {
        assert_eq!(
            kinds("true false null in"),
            vec![
                TokenKind::True,
                TokenKind::False,
                TokenKind::Null,
                TokenKind::In,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_numbers() {
        assert_eq!(
            kinds("42 -7 2.5 -0.25"),
            vec![
                TokenKind::Int(42),
                TokenKind::Int(-7),
                TokenKind::Float(2.5),
                TokenKind::Float(-0.25),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_int_then_member_dot() {
        // `[0].field` — the dot after an int with no trailing digit is
        // member access, not a float.
        assert_eq!(
            kinds("x[0].field"),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::LBracket,
                TokenKind::Int(0),
                TokenKind::RBracket,
                TokenKind::Dot,
                TokenKind::Ident("field".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_strings_both_quotes_with_escapes() {
        assert_eq!(
            kinds(r#"'success' "skip" 'it\'s' "a\"b" 'tab\there'"#),
            vec![
                TokenKind::Str("success".into()),
                TokenKind::Str("skip".into()),
                TokenKind::Str("it's".into()),
                TokenKind::Str("a\"b".into()),
                TokenKind::Str("tab\there".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_unterminated_string_errors() {
        let err = tokenize("'open").expect_err("unterminated");
        assert_eq!(err, ExprError::UnterminatedString { offset: 0 });
    }

    #[test]
    fn lex_unexpected_char_errors() {
        let err = tokenize("a @ b").expect_err("at-sign");
        assert!(matches!(
            err,
            ExprError::UnexpectedChar { ch: '@', offset: 2 }
        ));
    }

    #[test]
    fn lex_bare_minus_errors() {
        // No arithmetic in the subset — `-` only opens a numeric literal.
        let err = tokenize("a - b").expect_err("bare minus");
        assert!(matches!(err, ExprError::UnexpectedChar { ch: '-', .. }));
    }

    #[test]
    fn lex_single_pipe_and_amp_error() {
        assert!(tokenize("a | b").is_err());
        assert!(tokenize("a & b").is_err());
    }
}
