// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Exchange canonical string constants in private probe programs. The execution lexer's
//! complete strings (including escapes) and parenthesized constant-string additions are
//! recognized. No arbitrary expression is evaluated and the emitted program stays intact.

use jaq_core::load::lex::{Lexer, StrPart, Tok, Token};
use std::ops::Range;

/// Checked source edits, rendered lazily: all constants together, then each occurrence alone.
/// Independent literal self-inspection must not mask a comparison's spelling dependence.
pub(super) struct Exchanges<'a> {
    program: &'a str,
    edits: Vec<(Range<usize>, String)>,
}

pub(super) fn exchange<'a>(program: &'a str, spellings: [&str; 2]) -> Option<Exchanges<'a>> {
    let tokens = Lexer::new(program).lex().ok()?;
    let mut edits = Vec::new();
    for token in &tokens {
        collect(token, program, spellings, &mut edits)?;
    }
    edits.sort_by_key(|(range, _)| range.start);
    Some(Exchanges { program, edits })
}

impl Exchanges<'_> {
    pub(super) fn programs(&self) -> impl Iterator<Item = String> + '_ {
        let count = self.edits.len();
        let single = (count > 1).then_some(0..count).into_iter().flatten();
        (!self.edits.is_empty())
            .then_some(self.edits.as_slice())
            .into_iter()
            .chain(single.map(|i| &self.edits[i..=i]))
            .filter_map(|edits| self.render(edits))
    }

    fn render(&self, edits: &[(Range<usize>, String)]) -> Option<String> {
        let mut result = String::new();
        let mut cursor = 0;
        for (range, replacement) in edits {
            result.push_str(self.program.get(cursor..range.start)?);
            result.push_str(replacement);
            cursor = range.end;
        }
        result.push_str(self.program.get(cursor..)?);
        Some(result)
    }
}

fn constant(token: &Token<&str>) -> Option<String> {
    match &token.1 {
        Tok::Str(parts) => parts.iter().try_fold(String::new(), |mut value, part| {
            match part {
                StrPart::Str(part) => value.push_str(part),
                StrPart::Char(c) => value.push(*c),
                StrPart::Term(_) => return None,
            }
            Some(value)
        }),
        Tok::Block(tokens) if token.0.starts_with('(') => {
            // The lexer includes the closing delimiter as the block's last symbol.
            let (close, tokens) = tokens.split_last()?;
            if close.0 != ")" || !matches!(close.1, Tok::Sym) {
                return None;
            }
            let mut value = constant(tokens.first()?)?;
            let mut tail = tokens.get(1..)?.chunks_exact(2);
            for pair in &mut tail {
                if !matches!(pair[0].1, Tok::Sym) || pair[0].0 != "+" {
                    return None;
                }
                value.push_str(&constant(&pair[1])?);
            }
            tail.remainder().is_empty().then_some(value)
        }
        _ => None,
    }
}

fn collect(
    token: &Token<&str>,
    program: &str,
    spellings: [&str; 2],
    edits: &mut Vec<(Range<usize>, String)>,
) -> Option<()> {
    if let Some(value) = constant(token)
        && let Some(index) = spellings.iter().position(|spelling| *spelling == value)
    {
        // Lexer tokens borrow exact source slices. Check the range; no pointer dereference
        // or guessed substring search. A whole parenthesized constant is one disjoint edit.
        let start = token
            .0
            .as_ptr()
            .addr()
            .checked_sub(program.as_ptr().addr())?;
        let end = start.checked_add(token.0.len())?;
        if program.get(start..end) != Some(token.0) {
            return None;
        }
        let replacement = serde_json::to_string(spellings[1 - index]).ok()?;
        // Parentheses can be a function's argument list, not just grouping. Preserve them.
        let replacement = if matches!(token.1, Tok::Block(_)) {
            format!("({replacement})")
        } else {
            replacement
        };
        edits.push((start..end, replacement));
        return Some(());
    }
    match &token.1 {
        Tok::Str(parts) => {
            for part in parts {
                if let StrPart::Term(term) = part {
                    collect(term, program, spellings, edits)?;
                }
            }
        }
        Tok::Block(tokens) => {
            for token in tokens {
                collect(token, program, spellings, edits)?;
            }
        }
        _ => {}
    }
    Some(())
}
