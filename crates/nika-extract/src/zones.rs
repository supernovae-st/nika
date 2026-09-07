// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Trafilatura-grade rule cascade — the `mode: article` STAGE 1 (the
//! primary), ahead of the `dom_smoothie` readability stage and the
//! boilerpipe recall floor. CRAFT-ported from the Trafilatura algorithm
//! (Barbaresi, ACL'21 · GPLv3 source read for the algorithm, rewritten
//! from scratch · AGPL-3.0), which tops every 2024-2026 content-extraction
//! benchmark (rs-trafilatura 0.970 vs `dom_smoothie` 0.865 · `ScrapingHub`
//! Feb-2026; WCXB arXiv:2605.21097).
//!
//! Three phases on the real HTML tree (no internal vocabulary — the cleaned
//! subtree goes to `htmd` for the markdown conversion):
//!
//! 1. **DISCARD prune** — detach boilerplate zones (nav/share/related/
//!    cookie/footer/sidebar/comments…) by a class/id substring denylist,
//!    with the Trafilatura `with_backup` guard: if the prune would remove
//!    six-sevenths of the page text it is a false positive — skip it.
//!    PAGE-TYPE aware: on forums the discussion zones (`comment`/`reply`)
//!    are CONTENT, never pruned (the central WCXB structural fix).
//! 2. **ZONE target** — the `BODY_XPATH` cascade as an ordered CSS
//!    first-match: `[itemprop=articleBody]` → `<article>` → CMS
//!    content-class containers → `<main>`/`#content` → `[class^=main]`.
//!    A SEMANTIC container winning is the precision signal; if none match
//!    the rule cascade abstains (returns `None`) and the caller falls
//!    through to readability (whose scoring handles markup-poor pages).
//!    A container that matches but carries a SLIVER of the pruned page's
//!    text abstains too: a teaser `main` is not a body.
//! 3. **LINK-DENSITY finish** — detach the link-dense block remnants
//!    (`link chars > 0.8 · block chars` in a short block) that survive
//!    inside the container (menus, tag lists). Trafilatura's
//!    `link_density_test`, faithful.

use std::sync::LazyLock;

use ego_tree::NodeId;
use scraper::{Html, Selector};

use crate::page_type::PageType;

/// Minimum share (percent) of the pruned page's visible text the winning
/// zone must carry to be believed as the body.
const ZONE_MIN_SHARE_PERCENT: usize = 5;

/// Below this many chars of extracted text the rule result is THIN — the
/// caller falls through to readability. Trafilatura's `MIN_EXTRACTED_SIZE`.
/// Mirrors `article::THIN_THRESHOLD` (same constant · this gates SOURCE text,
/// `article` gates RENDERED markdown).
const MIN_EXTRACTED: usize = 250;

/// Hot-path selectors parsed ONCE (link-density runs per content block · a
/// per-call `Selector::parse` would reparse on every block). `"a"` /
/// `"div, ul, ol, p, section"` are static literals — the `Option` keeps the
/// no-unwrap contract while paying the parse a single time.
static ANCHOR_SEL: LazyLock<Option<Selector>> = LazyLock::new(|| Selector::parse("a").ok());
static BLOCK_SEL: LazyLock<Option<Selector>> =
    LazyLock::new(|| Selector::parse("div, ul, ol, p, section").ok());

/// Boilerplate class/id substring tokens (case-insensitive) — the
/// substring-reducible core of Trafilatura's `OVERALL_DISCARD_XPATH`
/// (D1+D2). Each becomes a `[class*="tok" i],[id*="tok" i]` select. The
/// anchored/word-boundary patterns (`^nav`, ` ad `) are a safe superset as
/// substrings here — the `with_backup` guard catches the rare over-prune.
const DISCARD_TOKENS: &[&str] = &[
    "share",
    "sharedaddy",
    "social",
    "syndication",
    "newsletter",
    "subscribe",
    "signup",
    "cookie",
    "consent",
    "gdpr",
    "sidebar",
    "banner",
    "breadcrumb",
    "byline",
    "related",
    "recirc",
    "read-more",
    "more-stories",
    "most-popular",
    "popular",
    "trending",
    "widget",
    "sponsor",
    "advert",
    "-ad-",
    "ad-container",
    "ad-slot",
    "ad-unit",
    "google-ad",
    "dfp",
    "outbrain",
    "taboola",
    "criteo",
    "promo",
    "teaser",
    "masthead",
    "site-header",
    "site-footer",
    "page-footer",
    "navbar",
    "navbox",
    "subnav",
    "menu",
    "skip-link",
    "pagination",
    "pager",
    "timestamp",
    "user-info",
    "user-profile",
    "author-box",
    "author-bio",
    "modal",
    "popup",
    "overlay",
    "lightbox",
    "paywall",
    "paid-content",
    "premium",
    "obfuscated",
    "blurred",
    "hidden",
    "visually-hidden",
    "sr-only",
    "noprint",
    "print-only",
    "tooltip",
];

/// Discussion zones — boilerplate on articles, CONTENT on forums (the WCXB
/// fix). Pruned for every [`PageType`] EXCEPT `Forum`.
const DISCUSSION_TOKENS: &[&str] = &[
    "comment",
    "comments",
    "reply",
    "replies",
    "respond",
    "disqus",
    "livefyre",
    "discussion",
];

/// Tags whose whole subtree is never content (Trafilatura `MANUALLY_CLEANED`
/// — the structural-chrome subset relevant after zone targeting).
const DISCARD_TAGS: &[&str] = &[
    "nav", "aside", "footer", "form", "header", "noscript", "template", "dialog", "menu",
];

/// The article-body cascade (Trafilatura's `BODY_XPATH`) as ordered CSS
/// selector groups — the first GROUP with a match wins, and its first
/// matching element (document order) is the content container. Priority =
/// Trafilatura's expr order (most specific semantic markers first, loosest
/// prefix match last). Used for Article / Product / Generic — the types
/// with a single main prose body.
const ARTICLE_CASCADE: &[&str] = &[
    // Expr 1 — explicit article-body markers.
    r#"[itemprop="articleBody" i], [id="articleContent" i]"#,
    // Expr 1/3 — CMS content-class containers (the post/entry/article body).
    r#"[class*="article-content" i], [class*="article-body" i], [class*="articlebody" i],
       [class*="article__body" i], [class*="article__content" i], [class*="post-content" i],
       [class*="post-body" i], [class*="post-text" i], [class*="entry-content" i],
       [class*="story-content" i], [class*="story-body" i], [class*="blog-content" i],
       [class*="single-content" i], [id*="article-content" i]"#,
    // Expr 2 — the semantic <article> element.
    "article",
    // Expr 4 — main/content containers.
    r#"main, [role="main" i], [id="main-content" i], [class*="main-content" i],
       [id="content" i], [class*="page-content" i], [class*="content-body" i],
       [class*="content-main" i]"#,
    // Expr 5 — loosest: a `main`-prefixed id/class.
    r#"[class^="main" i], [id^="main" i], [class*="fulltext" i]"#,
];

/// The forum/discussion cascade — a BROAD container holding ALL posts
/// (WCXB: the discussion IS the content, so the zone must not collapse to a
/// single article-body div). Per rs-trafilatura's per-type extraction
/// profiles.
const FORUM_CASCADE: &[&str] = &[
    r#"[class*="thread" i], [class*="discussion" i], [class*="topic" i],
       [id*="posts" i], [class*="posts" i], [class*="messages" i]"#,
    r#"main, [role="main" i], [id="content" i], [id="main-content" i], [class*="main-content" i]"#,
];

/// The generic broad cascade — main/content container, for Listing /
/// Search (link-collection pages with no single prose body).
const BROAD_CASCADE: &[&str] = &[
    r#"main, [role="main" i], [id="content" i], [id="main-content" i],
       [class*="main-content" i], [class*="page-content" i]"#,
];

/// The zone cascade for a page type (rs-trafilatura per-type profiles).
fn cascade_for(page_type: PageType) -> &'static [&'static str] {
    match page_type {
        PageType::Forum => FORUM_CASCADE,
        PageType::Listing | PageType::Search => BROAD_CASCADE,
        // Article / Product / Generic — single main prose body.
        _ => ARTICLE_CASCADE,
    }
}

/// Run the rule cascade. Returns the cleaned content-zone HTML (for the
/// caller to convert via `htmd`), or `None` when no semantic container is
/// found OR the extracted text is THIN — both signal "fall through to
/// readability". Pure · bounded (the depth guard ran upstream).
pub(crate) fn rule_content(body: &str, page_type: PageType) -> Option<String> {
    let mut doc = Html::parse_document(body);
    let total_text = body_text_len(&doc);

    // Phase 1 — DISCARD prune (decide-then-detach, with the over-prune guard).
    let discard = discard_ids(&doc, page_type, total_text);
    detach_all(&mut doc, &discard);

    // Phase 2 — ZONE target on the pruned tree (page-type cascade).
    let container_id = find_zone(&doc, page_type)?;

    // Phase 3 — LINK-DENSITY finish inside the container.
    let dense = link_dense_ids(&doc, container_id);
    detach_all(&mut doc, &dense);

    let remaining_text = body_text_len(&doc);
    let container = doc
        .tree
        .get(container_id)
        .and_then(scraper::ElementRef::wrap)?;
    // THIN gate on the TEXT content (not the serialized markup) — a
    // container that survived to near-empty is not a win.
    let text_len = visible_text_len(&container.text().collect::<String>());
    // SHARE gate — a zone holding a sliver of what the pruned page still
    // carries is a decorative container that happened to match (a teaser
    // `main`, a byline `article`), not the body. Absolute length cannot
    // see that: 300 characters clear MIN_EXTRACTED on a page carrying
    // 30,000. Abstaining hands the page to readability, which scores the
    // whole tree instead of trusting one selector.
    let carries_the_page =
        text_len.saturating_mul(100) >= remaining_text.saturating_mul(ZONE_MIN_SHARE_PERCENT);
    (text_len >= MIN_EXTRACTED && carries_the_page).then(|| container.html())
}

/// The visible text the served markup rendered INSIDE a semantic content
/// zone — the question `article.rs` asks before letting a `<noscript>`
/// fallback replace an extraction. Same prune + zone target as
/// [`rule_content`], WITHOUT its thin and share gates: the question here
/// is not "is this a good extraction" but "did the server render body
/// text at all". `0` means the page shipped a shell — a header, a
/// loading shim, an empty mount point — and nothing that reads as a body.
pub(crate) fn served_zone_text_len(body: &str, page_type: PageType) -> usize {
    let mut doc = Html::parse_document(body);
    let total_text = body_text_len(&doc);
    let discard = discard_ids(&doc, page_type, total_text);
    detach_all(&mut doc, &discard);
    let Some(container_id) = find_zone(&doc, page_type) else {
        return 0;
    };
    doc.tree
        .get(container_id)
        .and_then(scraper::ElementRef::wrap)
        .map_or(0, |el| visible_text_len(&el.text().collect::<String>()))
}

/// Collect the node-ids to discard: tag-chrome + the class/id denylist
/// (discussion tokens excluded on forums). Applies the `with_backup`
/// over-prune guard — if the matched zones carry >6/7 of the page text the
/// denylist is misfiring on this page; discard NOTHING.
fn discard_ids(doc: &Html, page_type: PageType, total_text: usize) -> Vec<NodeId> {
    let mut tokens: Vec<&str> = DISCARD_TOKENS.to_vec();
    if !page_type.comments_are_content() {
        tokens.extend_from_slice(DISCUSSION_TOKENS);
    }
    // One big selector: every discard tag + every token as class/id substring.
    let mut groups: Vec<String> = DISCARD_TAGS.iter().map(|t| (*t).to_owned()).collect();
    for tok in tokens {
        groups.push(format!(r#"[class*="{tok}" i]"#));
        groups.push(format!(r#"[id*="{tok}" i]"#));
    }
    let Some(sel) = Selector::parse(&groups.join(", ")).ok() else {
        return Vec::new();
    };

    // All matches, then keep only the OUTERMOST (an outer discard already
    // detaches its descendants). Counting only outermost text also avoids
    // DOUBLE-COUNTING nested matches (a `.comments` section AND its inner
    // `.comment` div), which would wrongly inflate the over-prune guard.
    let matched: std::collections::BTreeSet<NodeId> = doc.select(&sel).map(|el| el.id()).collect();
    let mut ids = Vec::new();
    let mut pruned_text = 0usize;
    for el in doc.select(&sel) {
        let has_matched_ancestor = el.ancestors().any(|a| matched.contains(&a.id()));
        if has_matched_ancestor {
            continue; // an outer match covers this one
        }
        ids.push(el.id());
        pruned_text += visible_text_len(&el.text().collect::<String>());
    }
    // with_backup guard: pruning >6/7 of the text is a false positive.
    if total_text > 0 && pruned_text.saturating_mul(7) > total_text.saturating_mul(6) {
        return Vec::new();
    }
    ids
}

/// The zone cascade: first selector GROUP (priority order) with a match
/// that is STILL ATTACHED to `<body>` → its id. A Phase-1 ancestor detach
/// leaves an inner zone target orphaned-but-intact (`detach()` nulls only
/// the detached node's own links), and scraper's linear arena scan still
/// surfaces it — so we skip orphans and keep looking (within a tier AND
/// across tiers), never serving content the prune meant to drop. `None`
/// when no attached semantic container exists.
fn find_zone(doc: &Html, page_type: PageType) -> Option<NodeId> {
    for group in cascade_for(page_type) {
        if let Ok(sel) = Selector::parse(group)
            && let Some(el) = doc.select(&sel).find(|el| attached_to_body(doc, el.id()))
        {
            return Some(el.id());
        }
    }
    None
}

/// True when `id`'s ancestor chain reaches the document `<body>` — i.e. the
/// node was NOT orphaned by a Phase-1 ancestor detach (`ancestors()`
/// excludes self, which is correct: no cascade targets `<body>` itself).
fn attached_to_body(doc: &Html, id: NodeId) -> bool {
    doc.tree.get(id).is_some_and(|node| {
        node.ancestors()
            .any(|a| a.value().as_element().is_some_and(|e| e.name() == "body"))
    })
}

/// Link-dense block ids inside `container` — Trafilatura's
/// `link_density_test` over `div`/`ul`/`ol`/`p` descendants: a SHORT block
/// whose anchor text dominates is navigation, not prose.
fn link_dense_ids(doc: &Html, container: NodeId) -> Vec<NodeId> {
    let Some(root) = doc.tree.get(container).and_then(scraper::ElementRef::wrap) else {
        return Vec::new();
    };
    let Some(block_sel) = BLOCK_SEL.as_ref() else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for el in root.select(block_sel) {
        if is_link_dense(el) {
            ids.push(el.id());
        }
    }
    ids
}

/// Trafilatura `link_density_test`: a block is link-dense (boilerplate)
/// when, for a SHORT block (`< limitlen` chars), its anchor text is ≥80% of
/// the text, or its anchors are mostly tiny (a menu). A block with no
/// anchors, or long prose, is content.
fn is_link_dense(el: scraper::ElementRef<'_>) -> bool {
    let Some(anchor_sel) = ANCHOR_SEL.as_ref() else {
        return false;
    };
    let text = el.text().collect::<String>();
    let text_len = visible_text_len(&text);
    if text_len == 0 {
        return false; // empty handled by CUT_EMPTY elsewhere; not "dense"
    }
    let mut link_len = 0usize;
    let mut count = 0usize;
    let mut shorts = 0usize;
    for a in el.select(anchor_sel) {
        let t = a.text().collect::<String>();
        let l = visible_text_len(t.trim());
        if l == 0 {
            continue;
        }
        link_len += l;
        count += 1;
        if l < 10 {
            shorts += 1;
        }
    }
    if count == 0 {
        return false; // no links → prose, keep
    }
    // `limitlen`: a small block is suspect; a large one is prose.
    let limitlen = if el.value().name() == "p" { 60 } else { 200 };
    if text_len >= limitlen {
        return false;
    }
    link_len.saturating_mul(10) > text_len.saturating_mul(8)
        || (count > 1 && shorts.saturating_mul(10) > count.saturating_mul(8))
}

/// Detach every id from its parent (orphans the subtree; idempotent over
/// already-orphaned nodes).
fn detach_all(doc: &mut Html, ids: &[NodeId]) {
    for &id in ids {
        if let Some(mut n) = doc.tree.get_mut(id) {
            n.detach();
        }
    }
}

/// Visible-text length: chars after collapsing whitespace runs (so a block
/// of newlines/indentation doesn't read as content).
fn visible_text_len(s: &str) -> usize {
    s.split_whitespace().map(str::len).sum()
}

/// Total visible text under `<body>` (the over-prune-guard denominator).
fn body_text_len(doc: &Html) -> usize {
    Selector::parse("body").ok().map_or(0, |sel| {
        doc.select(&sel)
            .next()
            .map_or(0, |b| visible_text_len(&b.text().collect::<String>()))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ARTICLE_CASCADE, BROAD_CASCADE, DISCARD_TAGS, DISCARD_TOKENS, DISCUSSION_TOKENS,
        FORUM_CASCADE, PageType, ZONE_MIN_SHARE_PERCENT, cascade_for, discard_ids, is_link_dense,
        link_dense_ids, rule_content,
    };
    use scraper::{Html, Selector};

    // ── Direct-unit harness for the private predicates ───────────────────
    // The link-density / discard / cascade math lives at exact arithmetic
    // boundaries that are far easier to pin DIRECTLY than to provoke through
    // the full `rule_content` pipeline (where parse quirks + the THIN gate
    // blur the operator under test). These helpers call the private fns with
    // crafted inputs sitting ON the decision boundary, so flipping any single
    // operator (`>`→`>=`/`==`/`<`, `+=`→`-=`/`*=`, `<`→`<=`, `&&`↔`||`)
    // changes the boolean result and FAILS the asserting test.

    /// Run `is_link_dense` on the first `sel` element of `fragment`. Keeps the
    /// owning `Html` on the stack so the `ElementRef` never escapes.
    fn dense(fragment: &str, sel: &str) -> bool {
        let frag = Html::parse_fragment(fragment);
        let selector = Selector::parse(sel).expect("test selector parses");
        let el = frag
            .select(&selector)
            .next()
            .expect("the crafted block is present");
        is_link_dense(el)
    }

    // Every cascade/discard selector MUST parse — `find_zone`/`discard_ids`
    // use `Selector::parse(..).ok()`, which would SILENTLY turn a future
    // typo'd group into a no-op tier (degrading extraction with zero signal).
    // This converts that silent runtime degradation into a loud test failure.
    #[test]
    fn every_selector_constant_parses() {
        for g in ARTICLE_CASCADE
            .iter()
            .chain(FORUM_CASCADE)
            .chain(BROAD_CASCADE)
        {
            assert!(Selector::parse(g).is_ok(), "cascade group: {g}");
        }
        for tok in DISCARD_TOKENS.iter().chain(DISCUSSION_TOKENS) {
            assert!(
                Selector::parse(&format!(r#"[class*="{tok}" i]"#)).is_ok(),
                "token: {tok}"
            );
        }
        for tag in DISCARD_TAGS {
            assert!(Selector::parse(tag).is_ok(), "tag: {tag}");
        }
    }

    // P1 regression (rust-pro review 2026-06-13): a discard-token ANCESTOR
    // wrapping a zone target leaves the inner target orphaned-but-intact
    // after Phase 1, and scraper's arena scan would still surface it. The
    // `attached_to_body` guard must reject the orphan so the LIVE <main>
    // wins — never serve content the prune meant to drop.
    #[test]
    fn zone_target_orphaned_by_ancestor_prune_is_rejected() {
        let page = r#"<html><body>
          <aside class="related">
            <article class="post-content"><p>related teaser junk that is short</p></article>
          </aside>
          <main><p>The genuine article body lives in main with plenty of real running
             words so that the extracted text comfortably clears the minimum extracted
             size threshold the cascade enforces, continuing with several more clauses
             of authentic prose a reader actually came to this page to read in full, and
             then carrying on with yet another sentence of real content to be safely over
             the threshold no matter how the visible-text length is counted here today.</p></main>
        </body></html>"#;
        let html = rule_content(page, PageType::Article).expect("main wins");
        assert!(html.contains("genuine article body"), "{html}");
        assert!(
            !html.contains("related teaser junk"),
            "orphaned <article> inside discarded <aside> must NOT be served: {html}"
        );
    }

    // A clean blog post: <article> container, nav/aside/footer/comments
    // boilerplate around AND inside it. The cascade must return the
    // article body only.
    const BLOG: &str = r#"<html><body>
      <header class="site-header"><nav><a href="/">Home</a> <a href="/x">X</a></nav></header>
      <article class="post-content">
        <h1>The Real Headline</h1>
        <p>This is the genuine opening paragraph of the article, with plenty of real
           running words that a reader actually came here to read today, continuing on
           with several more clauses so the body is unmistakably substantial prose.</p>
        <p>A second substantial paragraph continues the real article body with even more
           genuine prose and additional sentences so the extracted text comfortably
           clears the minimum-extracted-size threshold that the rule cascade enforces.</p>
        <aside class="related-posts"><a href="/a">Related junk one</a> <a href="/b">junk two</a></aside>
        <div class="share-buttons"><a href="/s">Tweet</a> <a href="/f">Share</a></div>
        <section class="comments"><p>spam comment body here</p></section>
      </article>
      <footer class="site-footer">copyright footer junk</footer>
    </body></html>"#;

    #[test]
    fn article_zone_kept_boilerplate_pruned() {
        let html = rule_content(BLOG, PageType::Article).expect("rule cascade extracts");
        assert!(html.contains("The Real Headline"), "{html}");
        assert!(html.contains("genuine opening paragraph"), "{html}");
        assert!(html.contains("second substantial paragraph"), "{html}");
        // Boilerplate gone — around AND inside the container.
        assert!(!html.contains("Home"), "nav pruned: {html}");
        assert!(!html.contains("Related junk"), "related pruned: {html}");
        assert!(!html.contains("Tweet"), "share pruned: {html}");
        assert!(!html.contains("spam comment"), "comments pruned: {html}");
        assert!(!html.contains("footer junk"), "footer pruned: {html}");
    }

    #[test]
    fn forum_keeps_comments_as_content() {
        // The WCXB structural fix: an article body PLUS a discussion section.
        // As an ARTICLE the comments are boilerplate (pruned · the article
        // body remains); as a FORUM the comments ARE the content (kept).
        let page = r#"<html><body>
          <main>
            <div class="post-content"><p>The original article body explains the topic in real
               detail across a couple of substantial sentences so that the article text on its
               own comfortably clears the minimum extracted size threshold that the cascade
               enforces, continuing with several additional clauses of genuine running prose
               that a reader actually came to this page in order to read from top to bottom.</p></div>
            <section class="comments">
              <div class="comment"><p>The top reply adds a genuinely useful and thorough
                 multi-sentence follow-up that on a forum page is the real content people
                 came to read, with concrete steps and plenty of running words here.</p></div>
            </section>
          </main>
        </body></html>"#;
        // FORUM: discussion kept.
        let forum = rule_content(page, PageType::Forum).expect("forum extracts");
        assert!(forum.contains("original article body"), "{forum}");
        assert!(
            forum.contains("top reply adds"),
            "forum keeps the discussion: {forum}"
        );
        // ARTICLE: discussion pruned, article body remains.
        let article = rule_content(page, PageType::Article).expect("article extracts");
        assert!(article.contains("original article body"), "{article}");
        assert!(
            !article.contains("top reply adds"),
            "as Article the comments are pruned: {article}"
        );
    }

    #[test]
    fn no_semantic_container_abstains() {
        // div-soup with no article/main/content markers → None (readability's job).
        let soup = "<html><body><div><div><p>some text here</p></div></div></body></html>";
        assert!(rule_content(soup, PageType::Generic).is_none());
    }

    #[test]
    fn over_prune_guard_reverts() {
        // If the WHOLE body matches a discard token (a page wrapped in
        // `<main class="content sidebar">`), pruning would nuke everything —
        // the guard must keep it (then the zone cascade still finds <main>).
        let body = r#"<html><body><main class="sidebar-wrapped">
           <h1>Title Here</h1>
           <p>The entire real article body lives inside a wrapper that happens to carry a
              denylisted class token, so the over-prune guard must NOT strip it, because
              that would throw away the whole page content wrongly and leave nothing behind
              for the reader, which is exactly the false positive the backup guard exists
              to prevent on pages whose outermost container is unluckily named.</p>
        </main></body></html>"#;
        let html = rule_content(body, PageType::Article).expect("guard kept the content");
        assert!(html.contains("entire real article body"), "{html}");
    }

    #[test]
    fn a_sliver_zone_abstains_even_when_it_clears_the_thin_floor() {
        // A decorative `<main>` teaser clears MIN_EXTRACTED on its own
        // (300 chars) while the page's real body, 10,000 chars of prose,
        // sits outside it. Absolute length alone cannot tell a body from a
        // teaser; the SHARE gate can, and abstaining hands the page to
        // readability rather than serving 3% of it.
        let teaser = "T".repeat(300);
        let body_prose = "B".repeat(10_000);
        let page = format!(
            "<html><body><main><p>{teaser}</p></main>\
             <div><p>{body_prose}</p></div></body></html>"
        );
        assert!(
            rule_content(&page, PageType::Article).is_none(),
            "a zone carrying under {ZONE_MIN_SHARE_PERCENT}% of the page is not the body"
        );
    }

    #[test]
    fn a_zone_at_exactly_the_share_boundary_is_kept() {
        // EXACTLY 5%: 500 characters of zone text against 10,000 of page
        // text. The gate is `text_len * 100 >= remaining * 5`, i.e.
        // 50_000 >= 50_000 — a `>=` turned `>` refuses this page, so this
        // case is the one that kills that mutant.
        let zone = "T".repeat(500);
        let rest = "B".repeat(9_500);
        let page = format!(
            "<html><body><main><p>{zone}</p></main>\
             <div><p>{rest}</p></div></body></html>"
        );
        let html = rule_content(&page, PageType::Article)
            .expect("a zone carrying exactly the minimum share is kept");
        assert!(html.contains(&zone), "the boundary zone is returned");

        // One character under the boundary abstains: 499 * 100 = 49_900
        // against 9_999 * 5 = 49_995. Pins that the acceptance above is
        // the boundary itself, not a blanket accept.
        let thinner = "T".repeat(499);
        let under = format!(
            "<html><body><main><p>{thinner}</p></main>\
             <div><p>{rest}</p></div></body></html>"
        );
        assert!(
            rule_content(&under, PageType::Article).is_none(),
            "one character under {ZONE_MIN_SHARE_PERCENT}% is a sliver, not the body"
        );
    }

    #[test]
    fn a_zone_carrying_the_page_still_wins() {
        // The same shape with the proportions reversed: `<main>` holds the
        // prose and the rest of the page is a scrap. The share gate must
        // not touch it (the companion that keeps the gate from becoming a
        // blanket abstention).
        let body_prose = "B".repeat(10_000);
        let scrap = "S".repeat(50);
        let page = format!(
            "<html><body><main><p>{body_prose}</p></main>\
             <div><p>{scrap}</p></div></body></html>"
        );
        let html = rule_content(&page, PageType::Article).expect("the body zone wins");
        assert!(html.contains(&body_prose), "the prose zone is returned");
    }

    #[test]
    fn thin_container_abstains() {
        // An <article> that's near-empty after pruning → None (not a win).
        let thin =
            r#"<html><body><article><nav><a href="/">only nav</a></nav></article></body></html>"#;
        assert!(rule_content(thin, PageType::Article).is_none());
    }

    // ════════════════════════════════════════════════════════════════════
    //  `is_link_dense` — the ratio-test boundaries (line 351 `link_len*10 >
    //  text_len*8`). `visible_text_len` drops whitespace, so every crafted
    //  token is a contiguous letter run with an EXACT char length.
    // ════════════════════════════════════════════════════════════════════

    // OVER the 0.8 ratio boundary by one char: link_len 9, text_len 10
    // (90 > 80). A short single-anchor `<p>` block ⇒ DENSE (true).
    //   · kills line 351 `>`→`<`     (90 < 80 would be false)
    //   · kills line 337 `link_len +=`→`-=`/`*=`  (link_len would collapse to
    //     0 ⇒ 0 > 80 false)
    //   · kills line 338 `count +=`→`-=`/`*=`     (count would collapse to 0 ⇒
    //     the `count == 0` guard returns false)
    //   · kills line 343 `count == 0`→`!=`        (with count==1, `!=` is true
    //     ⇒ early `return false`)
    //   · kills line 325 `text_len == 0`→`!=`     (with text_len==10, `!=` is
    //     true ⇒ early `return false`)
    //   · kills the `is_link_dense -> bool` body-replace-with-`false` mutant.
    #[test]
    fn link_dense_p_over_ratio_boundary_is_dense() {
        // plain "X" (1) + anchor "LLLLLLLLL" (9) ⇒ text_len 10, link_len 9.
        let frag = r#"<p>X<a href="/">LLLLLLLLL</a></p>"#;
        assert!(dense(frag, "p"), "90 > 80 ⇒ link-dense");
    }

    // EXACTLY ON the 0.8 ratio boundary: link_len 8, text_len 10 (80 == 80).
    // Strict `>` keeps it ⇒ NOT dense (false).
    //   · kills line 351 `>`→`>=`  (80 >= 80 would be true)
    //   · kills line 351 `>`→`==`  (80 == 80 would be true)
    // (count==1 so the line-352 shorts disjunct cannot rescue it.)
    #[test]
    fn link_dense_p_at_ratio_boundary_is_not_dense() {
        // plain "XX" (2) + anchor "LLLLLLLL" (8) ⇒ text_len 10, link_len 8.
        let frag = r#"<p>XX<a href="/">LLLLLLLL</a></p>"#;
        assert!(!dense(frag, "p"), "80 > 80 is false ⇒ kept as prose");
    }

    // A block with NO anchors is prose, never dense (count==0 ⇒ false).
    // The `is_link_dense -> bool` body-replace-with-`true` mutant returns true
    // here and fails; also a partner for line 343 from the count==0 side.
    #[test]
    fn link_dense_no_anchors_is_prose() {
        let frag = r"<p>plain running prose with no links at all here</p>";
        assert!(!dense(frag, "p"), "zero anchors ⇒ prose, not dense");
    }

    // ════════════════════════════════════════════════════════════════════
    //  `is_link_dense` — the `limitlen` short-vs-long gate (lines 347-348).
    //  `limitlen` is 60 for `<p>`, 200 otherwise. A dense-RATIO block with
    //  60 ≤ text_len < 200 is judged DIFFERENTLY by tag.
    // ════════════════════════════════════════════════════════════════════

    // text_len 100 at a dense ratio (link 90, 900 > 800). As a `<p>`
    // limitlen=60 ⇒ `100 >= 60` ⇒ early `return false` (long-for-a-paragraph
    // ⇒ prose). The `name() == "p"`→`!=` mutant would pick limitlen=200 ⇒
    // `100 >= 200` false ⇒ fall through ⇒ dense=true. So the `<p>` MUST be
    // not-dense.
    //   · kills line 347 `name() == "p"`→`!=`
    //   · kills line 348 `text_len >= limitlen`→`<`  (100 < 60 false ⇒ fall
    //     through ⇒ dense=true, but original is false)
    #[test]
    fn link_dense_long_paragraph_is_prose_not_dense() {
        // plain "P"×10 (10) + anchor "L"×90 (90) ⇒ text_len 100, link_len 90.
        let frag = format!(
            r#"<p>{}<a href="/">{}</a></p>"#,
            "P".repeat(10),
            "L".repeat(90)
        );
        assert!(
            !dense(&frag, "p"),
            "text_len 100 >= p-limitlen 60 ⇒ long enough to be prose"
        );
    }

    // The SAME dense-ratio block (text_len 100, link 90) as a `<div>`:
    // limitlen=200 ⇒ `100 >= 200` false ⇒ fall through ⇒ `900 > 800` ⇒ DENSE.
    // Confirms the tag genuinely swings the verdict (the other arm of the
    // line-347 mutant: a `<div>` flipped to limitlen 60 would wrongly drop it).
    #[test]
    fn link_dense_div_same_ratio_is_dense() {
        let frag = format!(
            r#"<div>{}<a href="/">{}</a></div>"#,
            "P".repeat(10),
            "L".repeat(90)
        );
        assert!(
            dense(&frag, "div"),
            "text_len 100 < div-limitlen 200 ⇒ short-enough dense block"
        );
    }

    // text_len EXACTLY at the `<p>` limitlen (60), dense ratio (link 54,
    // 540 > 480). `>=` keeps it as prose (false). The `>=`→`<` mutant would
    // make `60 < 60` false ⇒ fall through ⇒ dense=true.
    //   · kills line 348 `>=`→`<` at the exact equality point.
    #[test]
    fn link_dense_p_text_len_exactly_at_limitlen_is_prose() {
        // plain "P"×6 (6) + anchor "L"×54 (54) ⇒ text_len 60 == p-limitlen.
        let frag = format!(
            r#"<p>{}<a href="/">{}</a></p>"#,
            "P".repeat(6),
            "L".repeat(54)
        );
        assert!(
            !dense(&frag, "p"),
            "text_len 60 >= limitlen 60 ⇒ returns false before the ratio test"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    //  `is_link_dense` — the "menu of tiny links" disjunct (line 352:
    //  `count > 1 && shorts*10 > count*8`). Every menu below is built so the
    //  RATIO test (line 351) is FALSE, making the shorts heuristic the SOLE
    //  load-bearing decision — flipping any operator in line 352 (or the
    //  `||`/`&&` joins) changes the verdict.
    // ════════════════════════════════════════════════════════════════════

    // 5 short anchors (l=5, all < 10) + plain padding ⇒ text_len 50,
    // link_len 25 (250 > 400 is FALSE ⇒ line 351 off). count 5, shorts 5:
    // `5>1 && 50 > 40` ⇒ TRUE ⇒ DENSE via the shorts branch only.
    //   · kills line 351→352 `||`→`&&`   (false && true ⇒ false)
    //   · kills line 352 inner `>`→`<`    (shorts test 50 < 40 false)
    //   · kills line 339 `l < 10`→`==`    (5 == 10 false ⇒ shorts 0 ⇒ off)
    //   · kills line 339 `l < 10`→`>`     (5 > 10 false ⇒ shorts 0 ⇒ off)
    #[test]
    fn link_dense_menu_over_shorts_boundary_is_dense() {
        // padding "P"×25 (25) + 5×anchor "AAAAA" (5 each) ⇒ text_len 50.
        let frag = format!(
            r"<div>{}{}</div>",
            "P".repeat(25),
            r#"<a href="/">AAAAA</a>"#.repeat(5)
        );
        assert!(
            dense(&frag, "div"),
            "5 short links of 5, 5/5 shorts ⇒ menu, dense via line 352"
        );
    }

    // count 5, shorts EXACTLY 4 (4 anchors l=5 + 1 anchor l=10) ⇒ shorts test
    // `40 > 40` is FALSE under strict `>` ⇒ NOT dense. The padding keeps the
    // ratio test off (link_len 30, 300 > 400 false).
    //   · kills line 352 inner `>`→`>=`  (40 >= 40 true ⇒ dense)
    //   · kills line 352 inner `>`→`==`  (40 == 40 true ⇒ dense)
    #[test]
    fn link_dense_menu_at_shorts_boundary_is_not_dense() {
        // padding "P"×20 (20) + 4×"AAAAA" (5) + 1×"BBBBBBBBBB" (10) ⇒
        // text_len 50, link_len 30, count 5, shorts 4.
        let frag = format!(
            r#"<div>{}{}<a href="/">BBBBBBBBBB</a></div>"#,
            "P".repeat(20),
            r#"<a href="/">AAAAA</a>"#.repeat(4)
        );
        assert!(
            !dense(&frag, "div"),
            "shorts 4/5: 40 > 40 is false ⇒ not a tiny-link menu"
        );
    }

    // count 5 anchors of EXACTLY l=10 ⇒ none are short under `l < 10`
    // (`10 < 10` false). shorts 0 ⇒ line 352 off ⇒ NOT dense. The `<`→`<=`
    // mutant makes `10 <= 10` true ⇒ shorts 5 ⇒ `50 > 40` ⇒ dense.
    //   · kills line 339 `l < 10`→`<=` (the only mutant that needs an l==10
    //     anchor to distinguish from the original).
    #[test]
    fn link_dense_menu_anchors_exactly_ten_chars_is_not_dense() {
        // padding "P"×20 (20) + 5×"CCCCCCCCCC" (10) ⇒ text_len 70,
        // link_len 50 (500 > 560 false ⇒ ratio off), shorts 0.
        let frag = format!(
            r"<div>{}{}</div>",
            "P".repeat(20),
            r#"<a href="/">CCCCCCCCCC</a>"#.repeat(5)
        );
        assert!(
            !dense(&frag, "div"),
            "anchors of exactly 10 are NOT short (`l < 10`) ⇒ no shorts ⇒ kept"
        );
    }

    // count 2 LONG anchors (l=11) ⇒ shorts 0; ratio test off (link_len 22,
    // 220 > 400 false). Original line 352: `2>1 && 0 > 16` ⇒ `true && false`
    // ⇒ false ⇒ NOT dense. The `&&`→`||` mutant gives `true || false` ⇒ true
    // ⇒ dense, flipping the whole verdict.
    //   · kills line 352 `&&`→`||`.
    #[test]
    fn link_dense_menu_long_anchors_no_shorts_is_not_dense() {
        // padding "P"×28 (28) + 2×"DDDDDDDDDDD" (11) ⇒ text_len 50,
        // link_len 22, count 2, shorts 0.
        let frag = format!(
            r"<div>{}{}</div>",
            "P".repeat(28),
            r#"<a href="/">DDDDDDDDDDD</a>"#.repeat(2)
        );
        assert!(
            !dense(&frag, "div"),
            "count>1 but shorts 0: `&&` keeps it; `||` would wrongly drop it"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    //  `cascade_for` — the per-page-type zone profile (line 192 match arm).
    // ════════════════════════════════════════════════════════════════════

    // Listing AND Search MUST resolve to BROAD_CASCADE. Deleting the
    // `PageType::Listing | PageType::Search` arm drops them to the `_ =>
    // ARTICLE_CASCADE` fallback, so asserting BROAD (≠ ARTICLE) catches it.
    //   · kills the line-192 "delete match arm" mutant.
    #[test]
    fn cascade_for_listing_and_search_use_broad_cascade() {
        // Compare by content; BROAD_CASCADE and ARTICLE_CASCADE differ in
        // length and entries, so equality to BROAD distinguishes the arms.
        assert_eq!(
            cascade_for(PageType::Listing),
            BROAD_CASCADE,
            "Listing must use the broad (link-collection) cascade"
        );
        assert_eq!(
            cascade_for(PageType::Search),
            BROAD_CASCADE,
            "Search must use the broad (link-collection) cascade"
        );
        // Sanity anchor: the fallback arm is genuinely a DIFFERENT cascade,
        // so the deleted-arm mutant truly changes the returned slice.
        assert_ne!(
            BROAD_CASCADE, ARTICLE_CASCADE,
            "broad and article cascades must differ for the arm test to bite"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    //  `discard_ids` — the `with_backup` over-prune guard (line 262:
    //  `total_text > 0 && pruned_text*7 > total_text*6`). Called DIRECTLY so
    //  `total_text` and the matched-zone text are pinned to the boundary.
    // ════════════════════════════════════════════════════════════════════

    // pruned_text/total = 6/7 EXACTLY (42 == 42). Strict `>` ⇒ guard does NOT
    // fire ⇒ the discard ids ARE returned (the share zone is pruned).
    //   · kills line 262 site-2 `pruned*7 > total*6`→`>=` (42 >= 42 true ⇒
    //     guard fires ⇒ returns EMPTY).
    #[test]
    fn discard_ids_at_over_prune_boundary_still_prunes() {
        // share zone text "AAAAAA" (6) + content "B" (1) ⇒ total 7, pruned 6.
        let doc = Html::parse_document(
            r#"<html><body><div class="share">AAAAAA</div><p>B</p></body></html>"#,
        );
        let ids = discard_ids(&doc, PageType::Article, 7);
        assert!(
            !ids.is_empty(),
            "6/7 is exactly at the boundary: `>` keeps pruning; `>=` would revert"
        );
    }

    // total_text == 0 short-circuits the AND ⇒ guard skipped ⇒ ids returned.
    // The `total_text > 0`→`>=` mutant makes `0 >= 0` true, then `6*7 > 0*6`
    // ⇒ `42 > 0` true ⇒ guard fires ⇒ returns EMPTY. Passing total_text=0
    // with a real pruned zone (pruned 6) exercises exactly that site.
    //   · kills line 262 site-1 `total_text > 0`→`>=`.
    #[test]
    fn discard_ids_total_zero_short_circuits_guard() {
        let doc = Html::parse_document(
            r#"<html><body><div class="share">AAAAAA</div><p>BBBB</p></body></html>"#,
        );
        // total_text deliberately 0: `0 > 0` is false ⇒ guard never runs ⇒
        // the share div is still collected for discard.
        let ids = discard_ids(&doc, PageType::Article, 0);
        assert!(
            !ids.is_empty(),
            "`0 > 0` is false (short-circuit); `0 >= 0` would let the guard nuke the ids"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    //  `link_dense_ids` — Phase-3 collector (line 300). A container holding a
    //  link-dense menu MUST yield a non-empty id list; the
    //  "replace body with vec![]" mutant returns empty and fails.
    // ════════════════════════════════════════════════════════════════════

    #[test]
    fn link_dense_ids_finds_a_dense_block_in_the_container() {
        // <main> wraps a `<div>` tiny-link menu (5 short anchors, ratio test
        // off ⇒ dense via the shorts branch · same shape as the menu test
        // above). The inner <div> IS a BLOCK_SEL match, so `link_dense_ids`
        // descends into it and judges it dense.
        let doc = Html::parse_document(&format!(
            r#"<html><body><main><div class="nav">{}{}</div></main></body></html>"#,
            "P".repeat(25),
            r#"<a href="/">AAAAA</a>"#.repeat(5)
        ));
        // Resolve the <main> NodeId (the `container` argument).
        let main_sel = Selector::parse("main").expect("main selector");
        let container = doc.select(&main_sel).next().expect("main present").id();
        let ids = link_dense_ids(&doc, container);
        assert!(
            !ids.is_empty(),
            "the link-dense menu <div> inside <main> is collected (not vec![])"
        );
    }
}
