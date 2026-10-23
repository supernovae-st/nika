// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The env file a scheduled run loads (D7 · NIK-198): provider keys live in
//! the person's file, never in the unit. `nika arm fire --env-file <FILE>`
//! reads it itself. A launchd unit used to source it through `/bin/sh`,
//! and the plain `KEY=value` lines of a dotenv file were never exported, so
//! the fired run saw no key.
//!
//! The grammar is a dotenv file's, as a POSIX shell would assign it: one
//! `KEY=value` per line, an optional `export ` before it, blank lines and
//! `#` comments, a value bare, `'single-quoted'` (literal) or
//! `"double-quoted"` (`\"`, `\\`, `` \` `` and `\$` unescape; any other
//! backslash stays). A bare value loses the blanks around it and ends where
//! a `#` follows a blank. No `$VAR` is expanded. The last assignment of a
//! key wins. A refusal names the file and the line, never a value.

use std::path::{Path, PathBuf};

/// The most bytes an env file may hold: one person's keys, not a payload.
const MAX_ENV_FILE_BYTES: u64 = 1024 * 1024;

/// Why an env file was refused: the file, the line and the law, never a value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EnvFileError {
    path: PathBuf,
    line: Option<usize>,
    why: String,
}

impl EnvFileError {
    /// The file refused.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The 1-based line refused, `None` when the file itself was.
    #[must_use]
    pub fn line(&self) -> Option<usize> {
        self.line
    }
}

impl std::fmt::Display for EnvFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path = self.path.display();
        match self.line {
            Some(line) => write!(formatter, "env file {path}, line {line}: {}", self.why),
            None => write!(formatter, "env file {path}: {}", self.why),
        }
    }
}

impl std::error::Error for EnvFileError {}

/// Read and parse the env file at `path`.
///
/// # Errors
/// The file cannot be read (absent, not a regular file, over 1 MiB, not
/// UTF-8), or a line is not `KEY=value`.
pub fn read(path: &Path) -> Result<Vec<(String, String)>, EnvFileError> {
    let refuse = |why: String| EnvFileError {
        path: path.to_path_buf(),
        line: None,
        why,
    };
    let meta = std::fs::metadata(path).map_err(|e| refuse(format!("cannot be read ({e})")))?;
    if !meta.is_file() {
        return Err(refuse("is not a regular file".to_owned()));
    }
    if meta.len() > MAX_ENV_FILE_BYTES {
        return Err(refuse(format!(
            "holds {} bytes, more than an env file's {MAX_ENV_FILE_BYTES}",
            meta.len()
        )));
    }
    let bytes = std::fs::read(path).map_err(|e| refuse(format!("cannot be read ({e})")))?;
    let text = String::from_utf8(bytes).map_err(|_| refuse("is not UTF-8 text".to_owned()))?;
    parse(&text).map_err(|(line, why)| EnvFileError {
        path: path.to_path_buf(),
        line: Some(line),
        why: why.to_owned(),
    })
}

/// Parse an env file's text into its assignments, in file order, the last
/// one of a key alone kept.
///
/// # Errors
/// The 1-based line that is not `KEY=value`, and why.
pub fn parse(text: &str) -> Result<Vec<(String, String)>, (usize, &'static str)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut vars: Vec<(String, String)> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let Some((key, value)) = assignment(raw).map_err(|why| (index + 1, why))? else {
            continue;
        };
        vars.retain(|(known, _)| known != key);
        vars.push((key.to_owned(), value));
    }
    Ok(vars)
}

/// One line: `None` for a blank line or a comment, else its key and value.
fn assignment(raw: &str) -> Result<Option<(&str, String)>, &'static str> {
    let line = raw.trim_matches(|c: char| c.is_ascii_whitespace());
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let line = line
        .strip_prefix("export")
        .filter(|rest| rest.starts_with([' ', '\t']))
        .map_or(line, str::trim_start);
    let (key, rest) = line
        .split_once('=')
        .ok_or("not a `KEY=value` line (no `=`)")?;
    let mut chars = key.chars();
    let starts = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    if !starts || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("not a `KEY=value` line (a key is letters, digits and `_`)");
    }
    let value = value(rest)?;
    if value.contains('\0') {
        return Err("a value holds a NUL byte, which no environment can carry");
    }
    Ok(Some((key, value)))
}

/// A value as written after `=`: bare, single- or double-quoted.
fn value(rest: &str) -> Result<String, &'static str> {
    let unquoted = rest.trim_start_matches([' ', '\t']);
    if let Some(quoted) = unquoted.strip_prefix('\'') {
        let (inner, after) = quoted
            .split_once('\'')
            .ok_or("a single-quoted value is never closed")?;
        return closed(after).map(|()| inner.to_owned());
    }
    if let Some(quoted) = unquoted.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = quoted.char_indices();
        while let Some((at, c)) = chars.next() {
            match c {
                '"' => return closed(&quoted[at + 1..]).map(|()| out),
                '\\' => match chars.next() {
                    Some((_, next @ ('"' | '\\' | '$' | '`'))) => out.push(next),
                    Some((_, next)) => {
                        out.push('\\');
                        out.push(next);
                    }
                    None => out.push('\\'),
                },
                other => out.push(other),
            }
        }
        return Err("a double-quoted value is never closed");
    }
    // A bare value ends where a `#` follows a blank: `KEY=a#b` keeps its `#`.
    let comment = (rest.char_indices())
        .find(|&(at, c)| c == '#' && rest[..at].ends_with([' ', '\t']))
        .map_or(rest.len(), |(at, _)| at);
    Ok(rest[..comment].trim_matches([' ', '\t']).to_owned())
}

/// What may follow a closing quote: blanks, then an optional comment.
fn closed(after: &str) -> Result<(), &'static str> {
    let after = after.trim_start_matches([' ', '\t']);
    if after.is_empty() || after.starts_with('#') {
        Ok(())
    } else {
        Err("text follows a closing quote")
    }
}

#[cfg(test)]
#[path = "env_file_tests.rs"]
mod tests;
