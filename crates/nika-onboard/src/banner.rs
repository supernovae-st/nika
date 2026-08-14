// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The entry's own words — read from its BANNER.
//!
//! Every pack file opens on a comment banner, and the banner is where
//! the file says in human language what it is for. Three shapes ship,
//! all regular:
//!
//! ```text
//! 01 · Hello world — the smallest COMPLETE Nika workflow.
//! TEMPLATE · chain · gather facts → one model step → persist.
//! showcase · T2 chain · finance / freelance
//!     ⟵ a LABEL line · the sentence lives in the paragraph below it
//! ```
//!
//! Those words used to live in the envelope, as `workflow.description`.
//! They left with the 9-key nuke, and BOTH readers broke in the same
//! silent way. The clarify menu started answering with an INPUT's
//! description (a `description:` at indent 4 still matches « the first
//! line whose trim starts with `description:` »), so `agent-loop` read
//! « What the agent must accomplish » and eight of ten templates read
//! nothing. The BM25 index — which strips comments on purpose, so the
//! `# SLOT:` scaffolding prose cannot pollute it — simply lost the one
//! line a human sentence matches on, and answered `localization-factory`
//! to « chase unpaid invoices ». One reader went WRONG, one went QUIET.
//!
//! So the source moves, once, here — for the reader that had a right
//! answer to go back to. The menu row does.
//!
//! The ROUTER does not, and this module deliberately does not pretend
//! otherwise. Its index is a calibrated BM25 whose thresholds were
//! tuned against a corpus that carried `description:` — a line authored
//! to be MATCHED. A banner is authored to be READ. Feeding it the
//! banner instead was measured three ways on the live corpus (the
//! sentence · the descriptive head · the whole banner) and each one
//! traded two failing probes for two different ones: the confidence
//! moves, the calibration does not follow. Re-tuning it needs a probe
//! corpus, not a test suite to select on.

/// Every prose line of the banner, in order — the SPDX tag, the schema
/// hint and any copyright excluded (they are tooling, not words).
fn lines(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in body.lines() {
        let Some(rest) = line.strip_prefix('#') else {
            if line.trim().is_empty() {
                continue;
            }
            break; // the first YAML line closes the banner
        };
        let rest = rest.trim();
        if rest.is_empty()
            || rest.starts_with("SPDX-License-Identifier")
            || rest.starts_with("yaml-language-server")
            || rest.starts_with("Copyright")
        {
            continue;
        }
        out.push(rest);
    }
    out
}

/// Whether a `·`-separated segment of a title is a LABEL — the kind
/// marker, the number, the entry's own name, the tier tag. Labels are
/// single words (`TEMPLATE` · `01` · `human-gated-ship` · `showcase`)
/// or a tier pair (`T2 chain`); anything else is prose.
fn is_label(seg: &str) -> bool {
    !seg.contains(' ')
        || seg
            .split_once(' ')
            .is_some_and(|(head, _)| head.len() == 2 && head.starts_with('T'))
}

/// One line for a menu row — the entry's sentence, without the labels
/// that precede it (the row prints the name and the facet itself, so
/// repeating them would spend the row's width on nothing).
///
/// The labels are a PREFIX, and only a prefix: `human-gated-ship`'s own
/// title carries a `·` inside its sentence, so taking the trail after
/// the LAST separator loses two thirds of it and lands on an ASCII
/// diagram. Drop the leading labels, keep everything after them.
///
/// A title that is labels all the way down is a classification line
/// (`showcase · T2 chain · finance / freelance`) — the sentence is then
/// the paragraph under it.
pub(crate) fn sentence(body: &str) -> Option<String> {
    let lines = lines(body);
    let first = lines.first()?;
    let rest: Vec<&str> = first.split(" · ").skip_while(|seg| is_label(seg)).collect();
    let trail = rest.join(" · ");
    if trail.split_whitespace().count() >= 4 {
        return Some(trail);
    }
    lines
        .get(1)
        .map(|l| (*l).trim().to_owned())
        .filter(|l| !l.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Derived over the LIVE pack, never a fixture: a shape that stops
    /// yielding a sentence is a menu row gone blank, and a shape that
    /// stops yielding prose is evidence the router silently lost.
    #[test]
    fn every_pack_shape_yields_its_words() {
        let mut seen = 0;
        for name in nika_pack::template_names() {
            let body = nika_pack::template(&name).expect("embedded");
            let s = sentence(body).unwrap_or_default();
            assert!(s.split_whitespace().count() >= 4, "{name}: « {s} »");
            seen += 1;
        }
        for slug in nika_pack::example_slugs() {
            let body = nika_pack::example(&slug).expect("embedded");
            let s = sentence(body).unwrap_or_default();
            assert!(s.split_whitespace().count() >= 4, "{slug}: « {s} »");
            seen += 1;
        }
        assert!(seen > 30, "the pack shrank to {seen} entries");
    }

    #[test]
    fn a_label_line_defers_to_the_paragraph_under_it() {
        let body = "# SPDX-License-Identifier: Apache-2.0\n#\n# showcase · T2 chain · finance / freelance\n#\n# Friday ritual, automated — the ledger is filtered for overdue rows.\nnika: x\n";
        assert_eq!(
            sentence(body).as_deref(),
            Some("Friday ritual, automated — the ledger is filtered for overdue rows.")
        );
    }

    #[test]
    fn a_file_that_opens_on_yaml_has_no_banner() {
        assert_eq!(sentence("nika: bare\ntasks: {}\n"), None);
    }
}
