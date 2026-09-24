// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Path-shaped literals: the deterministic guard between prose and a local path.
//!
//! A plan carries verbatim phrases ("a short Markdown note … to ./out/summary.md",
//! "./notes avec plein de fichiers .md"). Only a single token that looks like a
//! path may become a constant, a permit entry or a tool argument; prose never
//! does. The shape decides the structure: one file is one read or write, several
//! files are a fan-out, a directory needs a glob, a placeholder needs a human.
//! A name with spaces is one literal quoted or given whole; unquoted in prose, its
//! extent is a human's to settle (a placeholder), never a truncated other file.

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
    /// A placeholder a human must resolve (`<slug>`, `{name}`, `$x`), or an unquoted
    /// multiword name whose extent the prose leaves open (`Notes équipe.txt`).
    Placeholder(String),
}

/// Quote pairs that make one literal of a multiword span (a whole string may also be
/// wrapped in straight single quotes).
const QUOTES: [(char, char); 4] = [('"', '"'), ('“', '”'), ('«', '»'), ('`', '`')];
/// Punctuation that may follow a literal without being part of it.
const TAIL: [char; 6] = ['.', ',', ';', ':', '!', '?'];
/// What ends a word no name runs over into the next one.
const CLOSERS: [char; 12] = ['.', ',', ';', ':', '!', '?', ')', ']', '»', '"', '`', '”'];
const ROOTS: [&str; 4] = ["../", "./", "~/", "/"];
/// Nouns that introduce a file's name (`le fichier Notes équipe.txt`).
const FILE_NOUNS: &[&str] = &["file", "fichier", "archivo", "fichero", "datei", "arquivo"];
/// Words never part of an unquoted file name (articles, prepositions, conjunctions,
/// pronouns, auxiliaries, read and write verbs), one per line.
const FUNCTION_WORDS: &str = include_str!("../assets/function_words.txt");

/// Every path-shaped literal of a phrase, in order, without duplicates: a quoted
/// multiword span is one literal; a bare file an unquoted name visibly runs over is a
/// placeholder naming those words, never the truncated tail.
#[must_use]
pub fn literals(text: &str) -> Vec<PathShape> {
    let mut found: Vec<PathShape> = Vec::new();
    for (shape, ..) in located(text) {
        if !found.contains(&shape) {
            found.push(shape);
        }
    }
    found
}

/// Every literal of [`literals`] in order, duplicates kept, with the byte span its words
/// (quotes included) cover in `text`.
pub(crate) fn located(text: &str) -> Vec<(PathShape, usize, usize)> {
    let items = items(text);
    // Every item is a subslice of `text`: its offset is its distance from the start.
    let offset = |part: &str| part.as_ptr() as usize - text.as_ptr() as usize;
    let mut found = Vec::new();
    for (at, (word, span)) in items.iter().enumerate() {
        let Some(shape) = span.clone().or_else(|| token(word)) else {
            continue;
        };
        let glued = match &shape {
            PathShape::File(file) if !rooted(file) => name_start(&items, at),
            _ => None,
        };
        let start = offset(glued.map_or(*word, |from| items[from].0));
        let shape = match (glued, shape) {
            (Some(from), PathShape::File(file)) => {
                let run: Vec<&str> = items[from..at].iter().map(|(w, _)| trim(w)).collect();
                PathShape::Placeholder(format!("{} {file}", run.join(" ")))
            }
            (_, shape) => shape,
        };
        found.push((shape, start, offset(word) + word.len()));
    }
    found
}

/// The one exact file a phrase names: the whole phrase when it is one file's literal
/// (a quoted name, a name given as an answer), else its only literal when a file.
#[must_use]
pub fn single_file(text: &str) -> Option<String> {
    if let Some(PathShape::File(path)) = token(text) {
        return Some(path);
    }
    match literals(text).as_slice() {
        [PathShape::File(path)] => Some(path.clone()),
        _ => None,
    }
}

/// One literal read as a path, or nothing when it is prose. A word is the common case;
/// a whole string with spaces (an answer, a quoted span) is one literal only when quoted,
/// or when it can only be a file's name.
pub fn token(word: &str) -> Option<PathShape> {
    let outer = word.trim().trim_end_matches(TAIL);
    let wrapped = (outer.chars().next().zip(outer.chars().next_back()))
        .is_some_and(|pair| pair == ('\'', '\'') || QUOTES.contains(&pair));
    let word = trim(word);
    if word.len() < 2 || word.contains("://") || word.contains(['\n', '\r', '\t']) {
        return None;
    }
    let spaced = word.contains(char::is_whitespace);
    let admits: fn(&str) -> bool = if wrapped { one_path } else { plain_name };
    if (spaced && !admits(word)) || (!rooted(word) && !bare_filename(word, spaced)) {
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

/// Quotes and brackets wrap a literal, punctuation follows it, and prose nests them
/// (`(./a.md, ./b.md),`): trim until nothing changes.
fn trim(mut word: &str) -> &str {
    loop {
        let next = word
            .trim()
            .trim_matches(|c: char| "\"'`()[]«»“”".contains(c))
            .trim_end_matches(TAIL);
        if next == word {
            return word;
        }
        word = next;
    }
}

fn rooted(word: &str) -> bool {
    ROOTS.iter().any(|root| word.starts_with(root))
}

fn function_word(word: &str) -> bool {
    let lower = word.to_lowercase();
    FUNCTION_WORDS.lines().any(|w| w == lower)
}

fn file_noun(word: &str) -> bool {
    FILE_NOUNS.contains(&word.to_lowercase().as_str())
}

/// A quoted literal with spaces: a bare name, or one rooted path whose segments open
/// and close on a visible character (no prose, no second path rides a leading slash).
fn one_path(word: &str) -> bool {
    !rooted(word)
        || (word.split('/').skip(1).all(|seg| seg.trim() == seg)
            && !word.split_whitespace().skip(1).any(rooted))
}

/// An unquoted string that can only be one file's name: two to six words of name
/// characters, none a function word or a file noun, no slash, no path before the last.
fn plain_name(word: &str) -> bool {
    let parts: Vec<&str> = word.split_whitespace().collect();
    (2..=6).contains(&parts.len())
        && !word.contains('/')
        && parts.iter().enumerate().all(|(at, part)| {
            part.chars()
                .all(|c| c.is_alphanumeric() || "_-.".contains(c))
                && part.chars().any(char::is_alphanumeric)
                && !function_word(part)
                && !file_noun(part)
                && (at + 1 == parts.len() || token(part).is_none())
        })
}

/// The phrase as `(text, path)` items: a quoted stretch that is one multiword path is
/// one item, every other stretch (quotes of prose included) its words.
fn items(text: &str) -> Vec<(&str, Option<PathShape>)> {
    let (mut items, mut from, mut at) = (Vec::new(), 0, 0);
    while let Some(open) = text.get(at..).and_then(|rest| rest.chars().next()) {
        let start = at;
        at += open.len_utf8();
        let Some(&(_, close)) = QUOTES.iter().find(|(o, _)| *o == open) else {
            continue;
        };
        let Some(end) = text.get(at..).and_then(|rest| rest.find(close)) else {
            continue;
        };
        let inner = text.get(at..at + end).unwrap_or_default().trim();
        at += end + close.len_utf8();
        let worded = inner
            .split_whitespace()
            .all(|w| w.chars().any(char::is_alphanumeric));
        if inner.contains(char::is_whitespace)
            && !inner.ends_with(TAIL)
            && worded
            && let Some(span) = token(&format!("\"{inner}\""))
        {
            let before = text.get(from..start).unwrap_or_default();
            items.extend(before.split_whitespace().map(|word| (word, None)));
            items.push((text.get(start..at).unwrap_or_default(), Some(span)));
            from = at;
        }
    }
    let rest = text.get(from..).unwrap_or_default();
    items.extend(rest.split_whitespace().map(|word| (word, None)));
    items
}

/// Where the name of the bare file at `at` visibly begins: at the first word after a
/// file noun (`le fichier Notes équipe.txt`) or at a capitalized word inside a sentence
/// (`dans Copie équipe.txt`); `None` when the token alone names the file.
fn name_start(items: &[(&str, Option<PathShape>)], at: usize) -> Option<usize> {
    let (mut run, mut capital) = (None, None);
    for index in (0..at).rev() {
        let (word, span) = &items[index];
        let core = trim(word);
        if span.is_some() || word.ends_with(CLOSERS) || function_word(core) {
            break;
        }
        if file_noun(core) {
            return run;
        }
        if token(word).is_some() || !core.chars().any(char::is_alphanumeric) {
            break;
        }
        run = Some(index);
        // A capital opening the phrase or a sentence is the sentence's, not the name's.
        let opens = index == 0 || items[index - 1].0.ends_with(['.', '!', '?', ':']);
        if !opens && core.starts_with(char::is_uppercase) {
            capital = Some(index);
        }
    }
    capital
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
/// an abbreviation (`e.g`) or a version (`v1.2`). A name given whole may hold spaces.
fn bare_filename(word: &str, spaced: bool) -> bool {
    if word.contains('/') {
        return false;
    }
    let Some((stem, ext)) = word.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && stem.chars().all(|c| {
            c.is_alphanumeric()
                || "_-.".contains(c)
                || (spaced && (c.is_whitespace() || "'’()&+,".contains(c)))
        })
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

    fn file(path: &str) -> PathShape {
        PathShape::File(path.to_owned())
    }

    fn open(name: &str) -> PathShape {
        PathShape::Placeholder(name.to_owned())
    }

    #[test]
    fn a_quoted_multiword_name_is_one_literal_in_every_quote_style() {
        for (text, source, destination) in [
            (
                "Copie \"Notes équipe.txt\" dans \"Copie équipe.txt\".",
                "Notes équipe.txt",
                "Copie équipe.txt",
            ),
            (
                "Copie « Notes équipe.txt » dans «Copie équipe.txt».",
                "Notes équipe.txt",
                "Copie équipe.txt",
            ),
            (
                "Copy “Team notes.txt” to `Team copy.txt`",
                "Team notes.txt",
                "Team copy.txt",
            ),
            (
                "Lies \"Über uns.md\" und schreib nach \"Bericht März.md\"",
                "Über uns.md",
                "Bericht März.md",
            ),
            (
                "Copy \"Notes de réunion (v2).txt\", then \"./Mes docs/Compte rendu.md\"",
                "Notes de réunion (v2).txt",
                "./Mes docs/Compte rendu.md",
            ),
        ] {
            assert_eq!(
                literals(text),
                vec![file(source), file(destination)],
                "{text}"
            );
        }
    }

    #[test]
    fn an_unquoted_multiword_name_is_never_truncated_to_its_last_word() {
        assert_eq!(
            literals(
                "Copie exactement le fichier Notes équipe.txt dans un nouveau fichier Copie équipe.txt"
            ),
            vec![open("Notes équipe.txt"), open("Copie équipe.txt")]
        );
        assert_eq!(
            literals(
                "un workflow qui lit Notes équipe.txt et écrit son contenu dans Copie équipe.txt."
            ),
            vec![open("Notes équipe.txt"), open("Copie équipe.txt")]
        );
        assert_eq!(
            literals("Copy Team notes.txt to Team copy.txt"),
            vec![open("Team notes.txt"), open("Team copy.txt")]
        );
        assert_eq!(
            literals("Read the file budget 2026.csv"),
            vec![open("budget 2026.csv")]
        );
        for text in [
            "Copie exactement le fichier Notes équipe.txt dans un nouveau fichier Copie équipe.txt",
            "dans Copie équipe.txt",
            "le fichier Notes équipe.txt",
        ] {
            assert!(!literals(text).contains(&file("équipe.txt")), "{text}");
            assert_eq!(single_file(text), None, "{text}");
        }
    }

    #[test]
    fn ordinary_bare_files_and_sentence_openers_keep_their_single_token() {
        for (text, expected) in [
            (
                "Copie entree.txt dans sortie.txt.",
                vec![file("entree.txt"), file("sortie.txt")],
            ),
            ("Fais une copie de entree.txt.", vec![file("entree.txt")]),
            ("Read the attached notes.txt", vec![file("notes.txt")]),
            ("Read Notes.txt", vec![file("Notes.txt")]),
            ("Done. Read AGENTS.md", vec![file("AGENTS.md")]),
            ("Lis le fichier entree.txt", vec![file("entree.txt")]),
            ("what the file may read (`fs.read`)", vec![file("fs.read")]),
            ("Ask \"Write final.md?\" first", vec![file("final.md")]),
        ] {
            assert_eq!(literals(text), expected, "{text}");
        }
    }

    #[test]
    fn a_whole_answer_may_name_a_file_with_spaces_and_prose_never_does() {
        for answer in [
            "Copie équipe.txt",
            "copie équipe.txt",
            "Copie équipe.txt.",
            "« Notes de réunion.txt »",
            "`Notes de réunion.txt`",
            "'Notes de réunion.txt'",
        ] {
            let name = answer
                .trim_matches(|c: char| "«»`'. ".contains(c))
                .to_owned();
            assert_eq!(
                token(answer),
                Some(PathShape::File(name.clone())),
                "{answer}"
            );
            assert_eq!(single_file(answer), Some(name), "{answer}");
        }
        for prose in [
            "Notes de réunion.txt",
            "write it to report.md",
            "the output file.md",
            "./a.txt ./b.txt",
            "Copy a.txt b.txt",
            "report.md please",
        ] {
            assert_ne!(
                token(prose).map(|shape| matches!(shape, PathShape::File(p) if p.contains(' '))),
                Some(true),
                "{prose}"
            );
        }
        assert_eq!(
            single_file("a short Markdown note to ./out/summary.md").as_deref(),
            Some("./out/summary.md")
        );
    }

    #[test]
    fn a_quoted_span_keeps_path_traversal_verbatim_and_never_holds_two_paths() {
        assert_eq!(
            literals("Read « ../secret notes.txt »"),
            vec![file("../secret notes.txt")]
        );
        assert_eq!(
            token("\"~/Mes documents/a b.md\""),
            Some(file("~/Mes documents/a b.md"))
        );
        for text in [
            "\"./a.txt ./b.txt\"",
            "\"/ leading space.txt\"",
            "\"./out /x.txt\"",
            "\"https://example.invalid/a b.txt\"",
            "\"Notes\néquipe.txt\"",
        ] {
            assert!(
                !matches!(token(text), Some(PathShape::File(p)) if p.contains(' ')),
                "{text}: {:?}",
                token(text)
            );
            assert!(
                literals(text)
                    .iter()
                    .all(|shape| !matches!(shape, PathShape::File(p) if p.contains(' '))),
                "{text}: {:?}",
                literals(text)
            );
        }
    }
}
