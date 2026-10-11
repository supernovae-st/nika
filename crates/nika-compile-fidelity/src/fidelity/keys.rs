// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The canonical key of a finding (2026-10-10): what a host compares across two revisions to
//! know whether the same defect still stands. A key is the finding's kind, its law's head when
//! the message opens with one (`INVENTED LITERAL`, or a check code such as `NIKA-SEC-004`), and
//! the subject the finding is about: the first value its message quotes between backticks or
//! guillemets (a literal, a path, a task, the request's words). Wording around the subject and
//! a remedy appended to the message change nothing; another subject or another law is another
//! key. These laws own the messages, so they own the key. A host keys a compile outcome's finding
//! the same way, by its target and message (`finding(&d.target, &d.message)`): a law refusal of
//! a Session's conversation is `conversation:invented-literal:./digest.md`, a judge's part
//! `semantic_verification:write the digest`. No target changes for it.

use super::Diagnostic;

impl Diagnostic {
    /// This finding's canonical key ([`finding`]).
    #[must_use]
    pub fn key(&self) -> String {
        finding(self.kind, &self.message)
    }
}

/// The canonical key of a finding of `kind` (a law's kind, or the target a compile outcome's
/// finding carries) whose words are `message`: `kind`, then the law's head (its words in lower
/// case joined by `-`, a check code as it is spelt), then the subject, each part present only
/// when the message carries it, joined by `:`.
#[must_use]
pub fn finding(kind: &str, message: &str) -> String {
    let (head, rest) = match message.split_once(": ") {
        Some((head, rest)) if is_head(head) => (head, rest),
        _ => ("", message),
    };
    let head = if head.starts_with("NIKA-") {
        head.to_owned()
    } else {
        head.to_lowercase().replace(' ', "-")
    };
    let mut key = kind.to_owned();
    for part in [head.as_str(), quoted(rest).unwrap_or_default()] {
        if !part.is_empty() {
            key.push(':');
            key.push_str(part);
        }
    }
    key
}

/// Whether `text` is a law's head: capital letters, digits, spaces and hyphens, a letter first.
fn is_head(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_uppercase())
        && text
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == ' ' || c == '-')
}

/// The first value `text` quotes, between backticks or between « and », trimmed; none when it
/// quotes nothing.
fn quoted(text: &str) -> Option<&str> {
    let between = |open: char, close: char| {
        let at = text.find(open)?;
        let rest = &text[at + open.len_utf8()..];
        rest.find(close).map(|end| (at, &rest[..end]))
    };
    let first = match (between('`', '`'), between('«', '»')) {
        (Some(tick), Some(quote)) => Some(if tick.0 < quote.0 { tick } else { quote }),
        (one, other) => one.or(other),
    };
    first
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, finding};

    fn identity(kind: &'static str, message: &str) -> String {
        Diagnostic {
            kind,
            message: message.to_owned(),
        }
        .key()
    }

    #[test]
    fn a_law_finding_keys_on_its_kind_its_head_and_its_subject() {
        let literal = "INVENTED LITERAL: `./news/digest.md` is not in the request. Use the request's own path or host, or declare a `const:` placeholder and ask for it.";
        assert_eq!(
            identity("literal", literal),
            "literal:invented-literal:./news/digest.md"
        );
        // A remedy appended or reworded around the subject is the same finding.
        let remedied = format!("{literal} When the person's words authorize it, state it.");
        assert_eq!(identity("literal", &remedied), identity("literal", literal));
        // The guillemets the request's words ride in, when they come first.
        let unwritten = "UNWRITTEN DESTINATION: the request asks to write into a file it does not name (« dans un fichier ») and no `nika:write` task carries it.";
        assert_eq!(
            identity("destination", unwritten),
            "destination:unwritten-destination:dans un fichier"
        );
        // Two laws over one task are two findings; one law over two tasks too.
        let order = "APPROVAL ORDER: the effect task `send` does not run only on the human's yes.";
        let unproven = "APPROVAL UNPROVEN: the task `send` may act.";
        assert_eq!(identity("gate", order), "gate:approval-order:send");
        assert_ne!(identity("gate", order), identity("gate", unproven));
        assert_ne!(
            identity("gate", order),
            identity("gate", &order.replace("`send`", "`post`"))
        );
        assert_eq!(
            identity(
                "records",
                "NON-TEXT AS JSON: the task `parse` applies `fromjson`."
            ),
            "records:non-text-as-json:parse"
        );
    }

    #[test]
    fn a_check_finding_keys_on_its_code_and_a_judge_finding_on_its_part() {
        assert_eq!(
            identity(
                "check",
                "NIKA-SEC-004: task `fetch_news` reaches a host no permit grants"
            ),
            "check:NIKA-SEC-004:fetch_news"
        );
        assert_eq!(
            finding(
                "verify",
                "The judge compared the whole request with the candidate's bytes: it does not carry « write the digest ». 1 repair(s) from that defect did not settle it."
            ),
            "verify:write the digest"
        );
        // No head and nothing quoted: the kind alone.
        assert_eq!(finding("verify", "Nothing is READY."), "verify");
        assert_eq!(
            finding("parse", "the candidate does not parse: line 3"),
            "parse"
        );
    }
}
