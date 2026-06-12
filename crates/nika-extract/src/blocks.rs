// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Boilerplate classification over atomic text blocks — Kohlschütter,
//! Fankhauser & Nejdl, *Boilerplate Detection using Shallow Text
//! Features* (WSDM 2010 · DOI 10.1145/1718487.1718542), **Algorithm 2**
//! ("Classifier based on Number of Words" — boilerpipe's
//! `NumWordsRulesClassifier`), transcribed VERBATIM from the paper.
//!
//! Why this one (per the 2026-06-12 primary-source survey): best
//! precision/complexity ratio in the literature — one streaming pass,
//! two integers per block, a frozen 9-branch tree, zero language
//! resources (F1 92.2 on the paper's news corpus · macro F1 0.834 in
//! the neutral SIGIR'23 re-evaluation, DOI 10.1145/3539618.3591920) —
//! and its failure mode is DECORRELATED from Readability's: it needs
//! only raw text-vs-anchor statistics, so it recovers prose on
//! div-soup pages where DOM/class-signal extractors starve. That makes
//! it the `mode: article` FALLBACK, not a replacement.
//!
//! Segmentation per the paper: atomic text blocks are maximal
//! character-data runs separated by any tag EXCEPT `<a>` (anchors do
//! not split blocks — that is what makes per-block link density
//! computable). The decision thresholds (0.333333 · 0.555556 · 16 ·
//! 15 · 4 · 40 · 17) are C4.8-learned quantile cuts — FROZEN
//! constants, never tuned here.

use ego_tree::iter::Edge;
use scraper::{Html, Node};

/// Subtrees that never contribute text (same set as `html.rs`).
const SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template"];

/// One atomic text block's shallow features.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Block {
    /// Whitespace-delimited tokens containing ≥1 alphanumeric char.
    words: u32,
    /// The subset of `words` inside `<a>` elements.
    anchor_words: u32,
}

impl Block {
    fn link_density(self) -> f64 {
        if self.words == 0 {
            0.0
        } else {
            f64::from(self.anchor_words) / f64::from(self.words)
        }
    }
}

/// The paper's Algorithm 2, verbatim (curr/prev/next shallow features →
/// CONTENT?). The constants are the published tree's split points.
fn is_content(prev: Block, curr: Block, next: Block) -> bool {
    if curr.link_density() <= 0.333_333 {
        if prev.link_density() <= 0.555_556 {
            if curr.words <= 16 {
                if next.words <= 15 {
                    prev.words > 4
                } else {
                    true
                }
            } else {
                true
            }
        } else if curr.words <= 40 {
            next.words > 17
        } else {
            true
        }
    } else {
        false
    }
}

/// The paper's degenerate last-resort rule ("keep blocks with ≥ min
/// text mass and bounded link density" — the CleanEval-retrained
/// variant, shown near-competitive on its own): guaranteed-recall
/// floor when even Algorithm 2 returns nothing.
fn is_content_degenerate(block: Block) -> bool {
    block.words >= 10 && block.link_density() <= 0.35
}

/// Segment `body` into atomic text blocks with (words · anchor-words)
/// per block. Iterative walk (hostile depth never becomes stack
/// depth); `<a>` toggles anchor counting WITHOUT flushing the block;
/// every other element boundary flushes.
fn segment(body: &str) -> Vec<(Block, String)> {
    let doc = Html::parse_document(body);
    let mut blocks: Vec<(Block, String)> = Vec::new();
    let mut current = Block::default();
    let mut text = String::new();
    let mut skip_depth = 0usize;
    let mut anchor_depth = 0usize;

    let mut flush = |current: &mut Block, text: &mut String| {
        if current.words > 0 {
            blocks.push((*current, std::mem::take(text)));
        } else {
            text.clear();
        }
        *current = Block::default();
    };

    for edge in doc.root_element().traverse() {
        match edge {
            Edge::Open(node) => match node.value() {
                Node::Element(el) => {
                    let name = el.name();
                    if SKIP_TAGS.contains(&name) {
                        skip_depth += 1;
                    } else if name == "a" {
                        anchor_depth += 1; // anchors never split a block
                    } else if skip_depth == 0 {
                        flush(&mut current, &mut text);
                    }
                }
                Node::Text(t) if skip_depth == 0 => {
                    for token in t.split_whitespace() {
                        if token.chars().any(char::is_alphanumeric) {
                            current.words += 1;
                            if anchor_depth > 0 {
                                current.anchor_words += 1;
                            }
                        }
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(token);
                    }
                }
                _ => {}
            },
            Edge::Close(node) => {
                if let Node::Element(el) = node.value() {
                    let name = el.name();
                    if SKIP_TAGS.contains(&name) {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else if name == "a" {
                        anchor_depth = anchor_depth.saturating_sub(1);
                    } else if skip_depth == 0 {
                        flush(&mut current, &mut text);
                    }
                }
            }
        }
    }
    flush(&mut current, &mut text);
    blocks
}

/// Run Algorithm 2 over the page: CONTENT blocks joined as Markdown
/// paragraphs (blank-line separated). Empty result = the page carried
/// no qualifying prose (the caller decides the next fallback).
pub(crate) fn boilerpipe_content(body: &str) -> String {
    let blocks = segment(body);
    let kept = classify_blocks(&blocks, is_content);
    if !kept.is_empty() {
        return kept.join("\n\n");
    }
    // Last resort: the degenerate recall floor.
    classify_blocks(&blocks, |_, curr, _| is_content_degenerate(curr)).join("\n\n")
}

/// Apply a (prev · curr · next) predicate over the block sequence
/// (boundary blocks see a zero-feature virtual neighbour, per the
/// paper's sliding window).
fn classify_blocks(
    blocks: &[(Block, String)],
    predicate: impl Fn(Block, Block, Block) -> bool,
) -> Vec<String> {
    let feature = |i: isize| -> Block {
        usize::try_from(i)
            .ok()
            .and_then(|idx| blocks.get(idx))
            .map_or_else(Block::default, |(b, _)| *b)
    };
    blocks
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            #[allow(clippy::cast_possible_wrap)] // blocks.len() ≤ isize::MAX (alloc bound)
            let i = *i as isize;
            predicate(feature(i - 1), feature(i), feature(i + 1))
        })
        .map(|(_, (_, text))| text.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(words: u32, anchor_words: u32) -> Block {
        Block {
            words,
            anchor_words,
        }
    }

    #[test]
    fn the_frozen_tree_branches_verbatim() {
        let zero = Block::default();
        // curr link-dense → boilerplate (the root split).
        assert!(!is_content(zero, block(30, 20), zero));
        // long block, clean neighbourhood → content.
        assert!(is_content(zero, block(50, 0), zero));
        // short block · short next · prev ≤ 4 words → boilerplate.
        assert!(!is_content(block(3, 0), block(10, 0), block(5, 0)));
        // short block · short next · prev > 4 words → content (flow text).
        assert!(is_content(block(20, 0), block(10, 0), block(5, 0)));
        // short block but LONG next → content (headline before a paragraph).
        assert!(is_content(zero, block(8, 0), block(30, 0)));
        // link-dense PREV arm: ≤40 words needs next > 17.
        let dense_prev = block(10, 9); // link_density 0.9 > 0.555556
        assert!(!is_content(dense_prev, block(30, 0), block(10, 0)));
        assert!(is_content(dense_prev, block(30, 0), block(20, 0)));
        assert!(is_content(dense_prev, block(45, 0), zero)); // > 40 wins alone
    }

    #[test]
    fn segmentation_anchors_do_not_split_and_count_anchor_words() {
        let html = r#"<html><body>
            <p>Plain words then <a href="/x">linked words here</a> then more.</p>
        </body></html>"#;
        let blocks = segment(html);
        // ONE block: the <a> did not split the paragraph run.
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        let (features, text) = &blocks[0];
        assert_eq!(features.words, 8, "{text}");
        assert_eq!(features.anchor_words, 3);
    }

    #[test]
    fn nav_lists_die_and_prose_survives_end_to_end() {
        // ≥18 words: after a link-dense nav block the published tree
        // demands `next.words > 17` for a ≤40-word paragraph — a
        // 17-word fixture sits exactly in the classifier's gray zone
        // (verified against the verbatim Algorithm 2 thresholds).
        let prose = "Real article prose carries more than enough running words \
                     to be recognised as genuine page content by the shallow \
                     classifier decision rules every single time.";
        let html = format!(
            r#"<html><body>
            <ul>
              <li><a href="/a">Home</a></li>
              <li><a href="/b">Products</a></li>
              <li><a href="/c">About us</a></li>
            </ul>
            <p>{prose}</p>
            <p>{prose}</p>
            <div class="footer-links"><a href="/l">Legal</a> · <a href="/p">Privacy</a></div>
            </body></html>"#
        );
        let out = boilerpipe_content(&html);
        assert!(out.contains("Real article prose"), "{out}");
        assert!(!out.contains("Products"), "nav links killed: {out}");
        assert!(!out.contains("Privacy"), "footer links killed: {out}");
        assert_eq!(
            out.matches("Real article prose").count(),
            2,
            "both paragraphs"
        );
    }

    #[test]
    fn degenerate_floor_fires_when_the_tree_keeps_nothing() {
        // A single isolated medium block with zero-word neighbours and
        // ≤16 words: Algorithm 2 rejects it (prev ≤ 4) — the degenerate
        // ≥10-words floor recovers it.
        let html = "<html><body>
            <div>eleven plain running words sit inside this lonely div block</div>
        </body></html>";
        let out = boilerpipe_content(html);
        assert!(out.contains("eleven plain running words"), "{out}");
    }

    #[test]
    fn empty_and_linkonly_pages_yield_empty() {
        assert_eq!(boilerpipe_content("<html><body></body></html>"), "");
        let linkonly = r#"<html><body><a href="/x">one</a> <a href="/y">two</a></body></html>"#;
        assert_eq!(boilerpipe_content(linkonly), "");
    }
}
