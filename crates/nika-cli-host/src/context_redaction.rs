// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shape-based redaction for outgoing model context. Callers own the read boundary;
//! this pure filter hides recognized credential forms and reports their kinds.

/// Redact obvious secrets before anything leaves: API keys, private key
/// blocks, `password=`/`token=` values. Returns the text and the KINDS
/// found (never a value). An unterminated private-key block hides the rest
/// of the input; this shape-based filter is not a general secret detector.
/// Eight key characters after a key marker make a key wherever the marker
/// starts, even after an encoded separator (`token%3Dsk-…`); only a marker
/// that continues a word and is followed by no digit and no capital is that
/// word (`task-scheduler`, `risk-assessment`).
#[must_use]
pub fn redact(text: &str) -> (String, Vec<String>) {
    let mut kinds = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut private_key_end: Option<String> = None;
    for line in text.lines() {
        // A missing or mismatched footer keeps the remaining material hidden.
        if let Some(end) = &private_key_end {
            if line.contains(end) {
                private_key_end = None;
            }
            continue;
        }
        if let Some((_, rest)) = line.split_once("-----BEGIN ")
            && let Some((label, _)) = rest.split_once("-----")
            && label.contains("PRIVATE KEY")
        {
            let end = format!("-----END {label}-----");
            private_key_end = (!rest.contains(&end)).then_some(end);
            out.push_str("[redacted private key block]\n");
            kinds.push("private key".to_owned());
            continue;
        }
        let mut l = line.to_owned();
        for (marker, kind) in [
            ("sk-", "api key"),
            ("AKIA", "aws key"),
            ("ghp_", "github token"),
            ("xoxb-", "slack token"),
        ] {
            let mut scan = 0;
            let mut kept = 0;
            let mut redacted = String::new();
            while let Some(offset) = l[scan..].find(marker) {
                let i = scan + offset;
                scan = i + marker.len();
                if !is_key(&l[..i], &l[scan..]) {
                    continue;
                }
                let end = l[i..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',')
                    .map_or(l.len(), |e| i + e);
                // Copy each untouched span once instead of repeatedly shifting
                // the tail of a line containing many credentials.
                redacted.push_str(&l[kept..i]);
                redacted.push_str("[redacted]");
                kept = end;
                scan = end;
            }
            if kept != 0 {
                redacted.push_str(&l[kept..]);
                l = redacted;
                kinds.push(kind.to_owned());
            }
        }
        if l.contains("-----BEGIN") {
            "[redacted private key block]".clone_into(&mut l);
            kinds.push("private key".to_owned());
        }
        for key in ["password", "token", "secret"] {
            let lower = l.to_ascii_lowercase();
            let hit = lower
                .find(&format!("{key}="))
                .or_else(|| lower.find(&format!("{key}: ")));
            if let Some(i) = hit {
                let start = i + key.len() + 1;
                let start = if l[start..].starts_with(' ') {
                    start + 1
                } else {
                    start
                };
                if start < l.len()
                    && !l[start..].trim().is_empty()
                    && !l[start..].starts_with("${{")
                {
                    l.replace_range(start.., "[redacted]");
                    kinds.push(key.to_owned());
                }
            }
        }
        out.push_str(&l);
        out.push('\n');
    }
    if !text.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    kinds.sort();
    kinds.dedup();
    (out, kinds)
}

/// A character an API key's text can hold.
fn key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Whether the key characters that open `after` (the text after a key marker) are a key; what
/// precedes the marker (`before`) never hides one: eight make a key wherever the marker starts,
/// even glued to an encoded separator (`token%3Dsk-…`, a JSON `\n`). Only after a marker that
/// continues a word is a run with no digit and no capital that word (`ta|sk-scheduler`).
fn is_key(before: &str, after: &str) -> bool {
    let run = &after[..after.find(|c: char| !key_char(c)).unwrap_or(after.len())];
    let keyed = run
        .bytes()
        .any(|b| b.is_ascii_digit() || b.is_ascii_uppercase());
    let inside_word = before.chars().next_back().is_some_and(key_char);
    run.len() >= 8 && (keyed || !inside_word)
}

#[cfg(test)]
mod tests {
    use super::redact;

    /// A word that holds a key marker is the word: these names and phrases reach a model, and a
    /// program's record, exactly as they were written.
    #[test]
    fn a_marker_inside_a_word_is_the_word_never_a_key() {
        for text in [
            "nika: weekly-task-scheduler",
            "nika: ask-questions-intake",
            "Write the risk-assessment summary",
            "the task-scheduler reads ./in/task-scheduler.csv",
            "desk-reporting, disk-usage-alerts and mask-everything",
            "choose task-write_output or task-read_source",
        ] {
            assert_eq!(redact(text), (text.to_owned(), Vec::new()), "{text}");
        }
    }

    /// The same marker where a key can start is still a key, and a credential's value is never
    /// kept.
    #[test]
    fn a_key_at_a_word_boundary_is_still_redacted() {
        let key = "sk-live-ABCDEFGH123456";
        for text in [
            key.to_owned(),
            format!("OPENAI_API_KEY={key}"),
            format!("key: {key}"),
            format!("\"{key}\""),
            format!("({key})"),
            format!("Bearer {key} then text"),
        ] {
            let (out, kinds) = redact(&text);
            assert!(!out.contains("ABCDEFGH123456"), "{text} -> {out}");
            assert_eq!(kinds, vec!["api key"], "{text}");
        }
        let (out, kinds) = redact("https://x.test/feed?token=abc123&page=2");
        assert_eq!(out, "https://x.test/feed?token=[redacted]");
        assert_eq!(kinds, vec!["token"]);
    }

    /// A key glued to what precedes it is still a key when its own characters are a key's (a
    /// digit, a capital): after an encoded separator (`%3D`, a JSON `\n` or `\u003d`,
    /// quoted-printable `=3D`), an underscore or a letter.
    #[test]
    fn a_key_after_an_encoded_separator_is_still_redacted() {
        let key = "sk-live-ABCDEFGH123456";
        for text in [
            format!("https://x.test/feed?token%3D{key}"),
            format!(r#"{{"note": "first line\n{key}"}}"#),
            format!(r"token\u003d{key}"),
            format!("key=3D{key}"),
            format!("KEY_{key}"),
            format!("x{key}"),
        ] {
            let (out, kinds) = redact(&text);
            assert!(!out.contains("ABCDEFGH123456"), "{text} -> {out}");
            assert_eq!(kinds, vec!["api key"], "{text}");
        }
    }
}
