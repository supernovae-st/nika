// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The canonical readings of what a run consumed and wrote. JSON is read strictly (RFC 8259):
//! a number keeps its exact decimal value, a repeated key makes the document ambiguous, and
//! nothing may follow it. A CSV file is read as `nika:convert` reads it (the same `csv` crate
//! and settings: a header row, every cell text). Every reading is bound to the sha256 of the
//! bytes it read.

use std::collections::{BTreeMap, BTreeSet};

use super::numbers::{Law, exact};
use super::values::{Datum, Row, row_of};

/// How deeply a JSON document may nest before it is refused.
const DEPTH: usize = 128;

/// The format a path names by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Format {
    /// One JSON document (`.json`).
    Json,
    /// One JSON value per line (`.jsonl`, `.ndjson`).
    JsonLines,
    /// A header row, then one record per row (`.csv`).
    Csv,
    /// Plain text (`.txt`, `.md`).
    Text,
}

impl Format {
    /// The format of a path by its extension, when it is one this component reads.
    #[must_use]
    pub fn of_path(path: &str) -> Option<Self> {
        let extension = std::path::Path::new(path)
            .extension()?
            .to_str()?
            .to_ascii_lowercase();
        match extension.as_str() {
            "json" => Some(Self::Json),
            "jsonl" | "ndjson" => Some(Self::JsonLines),
            "csv" => Some(Self::Csv),
            "txt" | "md" => Some(Self::Text),
            _ => None,
        }
    }

    /// The format's name in evidence messages.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::JsonLines => "JSON Lines",
            Self::Csv => "CSV",
            Self::Text => "text",
        }
    }
}

/// How much of what a run consumed or wrote the evidence holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Coverage {
    /// Exactly the bytes the run consumed or wrote.
    Complete,
    /// A sample of what the run consumed: never enough to judge that run.
    Sampled,
    /// Cut at a byte bound.
    Truncated,
}

impl Coverage {
    /// The coverage in evidence messages.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Sampled => "a sample",
            Self::Truncated => "cut at a byte bound",
        }
    }
}

/// Why a reading failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The bytes are not one well-formed document of the format, or the document is ambiguous.
    Malformed(String),
    /// A well-formed document whose records are not an array of objects (one object per line).
    NotRecords(String),
    /// A number beyond the precision this component compares exactly.
    Beyond,
}

fn malformed(why: impl Into<String>) -> Refusal {
    Refusal::Malformed(why.into())
}

/// The lowercase hex sha256 of the bytes a reading read.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    use std::fmt::Write as _;
    let digest = sha2::Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// A strict reader over one JSON text.
struct Reader<'a> {
    text: &'a str,
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn blanks(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<Datum, Refusal> {
        if depth > DEPTH {
            return Err(malformed("the document nests too deeply"));
        }
        self.blanks();
        match self.peek() {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Datum::Text),
            Some(b't') => self.word("true", Datum::Bool(true)),
            Some(b'f') => self.word("false", Datum::Bool(false)),
            Some(b'n') => self.word("null", Datum::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(malformed(format!(
                "an unexpected character at byte {}",
                self.at
            ))),
            None => Err(malformed("the document ends early")),
        }
    }

    fn word(&mut self, word: &str, value: Datum) -> Result<Datum, Refusal> {
        let rest = self.text.get(self.at..).unwrap_or_default();
        if rest.starts_with(word) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(malformed(format!("an unknown word at byte {}", self.at)))
        }
    }

    fn number(&mut self) -> Result<Datum, Refusal> {
        let start = self.at;
        while matches!(
            self.peek(),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.at += 1;
        }
        let token = self.text.get(start..self.at).unwrap_or_default();
        match exact(token) {
            Law::Number(number) => Ok(Datum::Number(number)),
            Law::Beyond => Err(Refusal::Beyond),
            Law::NotANumber => Err(malformed(format!("{token:?} is no finite JSON number"))),
        }
    }

    fn hex4(&mut self) -> Result<u32, Refusal> {
        let digits = self
            .text
            .get(self.at..self.at + 4)
            .filter(|digits| digits.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| malformed("a unicode escape that is not four hexadecimal digits"))?;
        let unit = u32::from_str_radix(digits, 16)
            .map_err(|_| malformed("a unicode escape that is not four hexadecimal digits"))?;
        self.at += 4;
        Ok(unit)
    }

    /// The character a `u` escape names, a surrogate pair read as one character.
    fn unicode(&mut self) -> Result<char, Refusal> {
        let unit = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&unit) {
            let rest = self.text.get(self.at..).unwrap_or_default();
            if !rest.starts_with("\\u") {
                return Err(malformed("a high surrogate without its pair"));
            }
            self.at += 2;
            let low = self.hex4()?;
            if !(0xDC00..0xE000).contains(&low) {
                return Err(malformed("a high surrogate without its pair"));
            }
            0x1_0000 + ((unit - 0xD800) << 10) + (low - 0xDC00)
        } else if (0xDC00..0xE000).contains(&unit) {
            return Err(malformed("a lone low surrogate"));
        } else {
            unit
        };
        char::from_u32(code).ok_or_else(|| malformed("an escape that names no character"))
    }

    fn escape(&mut self, out: &mut String) -> Result<(), Refusal> {
        let code = self
            .peek()
            .ok_or_else(|| malformed("an escape at the end"))?;
        self.at += 1;
        let decoded = match code {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => self.unicode()?,
            _ => return Err(malformed("an unknown escape")),
        };
        out.push(decoded);
        Ok(())
    }

    fn string(&mut self) -> Result<String, Refusal> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let next = self
                .text
                .get(self.at..)
                .and_then(|rest| rest.chars().next())
                .ok_or_else(|| malformed("an unterminated string"))?;
            self.at += next.len_utf8();
            match next {
                '"' => return Ok(out),
                '\\' => self.escape(&mut out)?,
                control if u32::from(control) < 0x20 => {
                    return Err(malformed("a control character inside a string"));
                }
                other => out.push(other),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Datum, Refusal> {
        self.at += 1;
        let mut items = Vec::new();
        self.blanks();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Datum::List(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.blanks();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Datum::List(items));
                }
                _ => return Err(malformed(format!("an unclosed array at byte {}", self.at))),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Datum, Refusal> {
        self.at += 1;
        let mut fields = BTreeMap::new();
        self.blanks();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Datum::Record(fields));
        }
        loop {
            self.blanks();
            if self.peek() != Some(b'"') {
                return Err(malformed(format!(
                    "an object key at byte {} is no string",
                    self.at
                )));
            }
            let key = self.string()?;
            self.blanks();
            if self.peek() != Some(b':') {
                return Err(malformed(format!(
                    "a key without its value at byte {}",
                    self.at
                )));
            }
            self.at += 1;
            let value = self.value(depth + 1)?;
            if fields.contains_key(&key) {
                return Err(malformed(format!("the key {key:?} repeats")));
            }
            fields.insert(key, value);
            self.blanks();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Datum::Record(fields));
                }
                _ => return Err(malformed(format!("an unclosed object at byte {}", self.at))),
            }
        }
    }
}

/// One JSON document, strictly: nothing but blanks may follow it.
pub(crate) fn json_document(text: &str) -> Result<Datum, Refusal> {
    let mut reader = Reader { text, at: 0 };
    let value = reader.value(0)?;
    reader.blanks();
    if reader.at == text.len() {
        Ok(value)
    } else {
        Err(malformed(format!(
            "content after the document at byte {}",
            reader.at
        )))
    }
}

/// One JSON document per non-empty line.
pub(crate) fn json_lines(text: &str) -> Result<Vec<Datum>, Refusal> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| !line.trim().is_empty())
        .map(json_document)
        .collect()
}

/// The records of a CSV text as `nika:convert` reads them: the header row names the columns
/// and every cell is text. A repeated column name makes the file ambiguous.
pub(crate) fn csv_records(text: &str) -> Result<Vec<Row>, Refusal> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| malformed(format!("the CSV header: {e}")))?
        .clone();
    let mut seen = BTreeSet::new();
    for header in &headers {
        if !seen.insert(header) {
            return Err(malformed(format!("the column {header:?} repeats")));
        }
    }
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| malformed(format!("a CSV row: {e}")))?;
        let fields: BTreeMap<String, Datum> = headers
            .iter()
            .zip(record.iter())
            .map(|(header, cell)| (header.to_owned(), Datum::Text(cell.to_owned())))
            .collect();
        rows.push(row_of(fields, true));
    }
    Ok(rows)
}

/// One record of a JSON source or output: an object, its values typed.
fn object_row(item: Datum, why: &str) -> Result<Row, Refusal> {
    if let Datum::Record(fields) = item {
        Ok(row_of(fields, false))
    } else {
        Err(Refusal::NotRecords(why.to_owned()))
    }
}

/// The records a JSON value holds: an array of objects.
pub(crate) fn json_records(document: Datum) -> Result<Vec<Row>, Refusal> {
    let Datum::List(items) = document else {
        return Err(Refusal::NotRecords(
            "the document is not an array of objects".to_owned(),
        ));
    };
    items
        .into_iter()
        .map(|item| object_row(item, "an item of the array is not an object"))
        .collect()
}

/// The records of a source or an output of `format`.
pub(crate) fn records(format: Format, text: &str) -> Result<Vec<Row>, Refusal> {
    match format {
        Format::Json => json_records(json_document(text)?),
        Format::JsonLines => json_lines(text)?
            .into_iter()
            .map(|line| object_row(line, "a line is not an object"))
            .collect(),
        Format::Csv => csv_records(text),
        Format::Text => Err(Refusal::NotRecords(
            "plain text holds no records".to_owned(),
        )),
    }
}
