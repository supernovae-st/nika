// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Path-shaped literals: the deterministic guard between prose and a local path.
//!
//! A plan carries verbatim phrases ("a short Markdown note … to ./out/summary.md",
//! "./notes avec plein de fichiers .md"). Only a single token that looks like a
//! path may become a constant, a permit entry or a tool argument; prose never
//! does. The shape decides the structure: one file is one read or write, several
//! files are a fan-out, a directory needs a glob, a placeholder needs a human.

/// How one literal token reads as a local path.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathShape {
    /// One exact file: a final segment with an extension, no placeholder, no glob.
    File(String),
    /// A directory: a trailing slash or no extension on the final segment.
    Directory(String),
    /// A glob the engine can expand (`*`, `?`, `[`).
    Glob(String),
    /// A placeholder a human must resolve (`<slug>`, `{name}`, `$x`).
    Placeholder(String),
}

/// Every path-shaped token of a phrase, in order, without duplicates.
#[must_use]
pub fn literals(text: &str) -> Vec<PathShape> {
    let mut found: Vec<PathShape> = Vec::new();
    for word in text.split_whitespace() {
        if let Some(shape) = token(word)
            && !found.contains(&shape)
        {
            found.push(shape);
        }
    }
    found
}

/// The one exact file a phrase names, when it names exactly one path-shaped token
/// and that token is a file.
#[must_use]
pub fn single_file(text: &str) -> Option<String> {
    match literals(text).as_slice() {
        [PathShape::File(path)] => Some(path.clone()),
        _ => None,
    }
}

/// One whitespace-free word read as a path, or nothing when it is prose.
pub fn token(word: &str) -> Option<PathShape> {
    // Quotes and brackets wrap a token, punctuation follows it, and prose nests them
    // (`(./a.md, ./b.md),`): trim until nothing changes.
    let mut word = word;
    loop {
        let next = word
            .trim_matches(|c: char| {
                matches!(c, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '«' | '»')
            })
            .trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if next == word {
            break;
        }
        word = next;
    }
    if word.len() < 2 || word.contains("://") || word.chars().any(char::is_whitespace) {
        return None;
    }
    let rooted = word.starts_with("./")
        || word.starts_with("../")
        || word.starts_with('/')
        || word.starts_with("~/");
    if !rooted && !bare_filename(word) {
        return None;
    }
    let literal = word.to_owned();
    if word.contains(['<', '>', '{', '}', '$']) {
        return Some(PathShape::Placeholder(literal));
    }
    if word.contains(['*', '?', '[']) {
        return Some(PathShape::Glob(literal));
    }
    if word.ends_with('/') || extension(word).is_none() {
        return Some(PathShape::Directory(literal));
    }
    Some(PathShape::File(literal))
}

/// The material a read consumes when a request names a path: a file or a glob as
/// stated; a directory as every file directly under it (`./notes` → `./notes/*`), a
/// derivation of the stated literal that guesses neither an extension nor a depth.
/// `nika:glob` returns files only, so a subdirectory is left out, never read.
pub(crate) fn material(path: &str) -> String {
    match token(path) {
        Some(PathShape::Directory(dir)) => format!("{}/*", dir.trim_end_matches('/')),
        _ => path.to_owned(),
    }
}

/// `name.ext` with a real stem and an alphabetic-led extension; never a bare number,
/// an abbreviation (`e.g`) or a version (`v1.2`).
fn bare_filename(word: &str) -> bool {
    if word.contains('/') {
        return false;
    }
    let Some((stem, ext)) = word.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && stem
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && stem.chars().any(char::is_alphanumeric)
        && ext.len() >= 2
        && ext.len() <= 8
        && ext.chars().next().is_some_and(char::is_alphabetic)
        && ext.chars().all(char::is_alphanumeric)
}

/// The lowercase extension of the final segment, when it has one.
pub fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty()
        || ext.is_empty()
        || ext.len() > 8
        || !ext.chars().next().is_some_and(char::is_alphabetic)
        || !ext.chars().all(char::is_alphanumeric)
    {
        return None;
    }
    Some(ext.to_ascii_lowercase())
}

/// The `snake_case` stem of a file path, for task ids and constant names.
#[must_use]
pub fn stem(path: &str) -> String {
    let name = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let mut out = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
        }
    }
    let out = out.trim_end_matches('_').to_owned();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        format!("file_{out}")
    } else {
        out
    }
}

/// The directory a glob or a directory literal lives under, for a permit entry.
#[must_use]
pub fn directory_of(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    let cut = trimmed
        .find(['*', '?', '['])
        .map_or(trimmed.len(), |i| trimmed[..i].rfind('/').unwrap_or(0));
    let dir = trimmed[..cut].trim_end_matches('/');
    if dir.is_empty() {
        ".".to_owned()
    } else {
        dir.to_owned()
    }
}

/// Structured text formats the assembler can parse before a code rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Structured {
    Json,
    Csv,
    Yaml,
    Toml,
}

impl Structured {
    #[must_use]
    pub fn of(path: &str) -> Option<Self> {
        match extension(path)?.as_str() {
            "json" => Some(Self::Json),
            "csv" => Some(Self::Csv),
            "yaml" | "yml" => Some(Self::Yaml),
            "toml" => Some(Self::Toml),
            _ => None,
        }
    }
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_token_sheds_nested_brackets_and_punctuation_until_nothing_changes() {
        assert_eq!(
            token("./docs/fournisseur-c.md),"),
            Some(PathShape::File("./docs/fournisseur-c.md".to_owned()))
        );
        assert_eq!(
            token("(./docs/fournisseur-a.md,"),
            Some(PathShape::File("./docs/fournisseur-a.md".to_owned()))
        );
        assert_eq!(
            token("«./out/rapport.md»."),
            Some(PathShape::File("./out/rapport.md".to_owned()))
        );
    }

    #[test]
    fn a_path_token_is_one_word_that_looks_like_a_path() {
        assert_eq!(
            token("./out/summary.md"),
            Some(PathShape::File("./out/summary.md".to_owned()))
        );
        assert_eq!(
            token("./out/summary.md."),
            Some(PathShape::File("./out/summary.md".to_owned()))
        );
        assert_eq!(
            token("orders-2026-09.csv"),
            Some(PathShape::File("orders-2026-09.csv".to_owned()))
        );
        assert_eq!(
            token("./notes"),
            Some(PathShape::Directory("./notes".to_owned()))
        );
        assert_eq!(
            token("./notes/"),
            Some(PathShape::Directory("./notes/".to_owned()))
        );
        assert_eq!(
            token("./notes/*.md"),
            Some(PathShape::Glob("./notes/*.md".to_owned()))
        );
        assert_eq!(
            token("./catalog/<slug>.md"),
            Some(PathShape::Placeholder("./catalog/<slug>.md".to_owned()))
        );
        for prose in [
            "note",
            "e.g.",
            "v1.2",
            "3.5",
            ".md",
            "https://example.invalid/a.txt",
            "a",
            "/",
        ] {
            assert_eq!(token(prose), None, "{prose}");
        }
    }

    #[test]
    fn prose_yields_only_its_path_tokens_in_order_without_duplicates() {
        let shapes = literals(
            "a short Markdown note naming the top 3 countries to ./out/summary.md (./out/summary.md)",
        );
        assert_eq!(shapes, vec![PathShape::File("./out/summary.md".to_owned())]);
        assert_eq!(
            single_file("a short Markdown note to ./out/summary.md"),
            Some("./out/summary.md".to_owned())
        );
        assert_eq!(single_file("./a.md ; ./b.md"), None);
        assert_eq!(single_file("./notes avec plein de fichiers .md"), None);
        assert_eq!(
            literals("./catalog/solar-lamp.md ; ./catalog/wind-chime.md").len(),
            2
        );
    }

    #[test]
    fn stems_extensions_and_directories_are_derived_from_the_literal() {
        assert_eq!(stem("./out/shipped-by-country.json"), "shipped_by_country");
        assert_eq!(stem("./out/2026-report.md"), "file_2026_report");
        assert_eq!(extension("./data/orders.CSV").as_deref(), Some("csv"));
        assert_eq!(extension("./notes"), None);
        assert_eq!(directory_of("./notes/*.md"), "./notes");
        assert_eq!(directory_of("./notes/"), "./notes");
        assert_eq!(directory_of("*.md"), ".");
        assert_eq!(Structured::of("./x.yml"), Some(Structured::Yaml));
        assert_eq!(Structured::of("./x.md"), None);
    }

    #[test]
    fn a_directory_is_read_as_every_file_directly_under_it_and_nothing_else_changes() {
        assert_eq!(material("./notes"), "./notes/*");
        assert_eq!(material("./notes/"), "./notes/*");
        assert_eq!(material("./notes/brief.md"), "./notes/brief.md");
        assert_eq!(material("./notes/*.md"), "./notes/*.md");
        assert_eq!(material("./catalog/<slug>.md"), "./catalog/<slug>.md");
    }
}
