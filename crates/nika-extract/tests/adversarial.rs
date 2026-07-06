// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Adversarial input battery — `nika:fetch` pulls attacker-controlled
//! URLs whose responses feed an LLM agent loop, so every extract mode
//! must be TOTAL against hostile bodies: no panic, no stack overflow,
//! no exponential expansion, no unbounded allocation. Each test runs on
//! a default-stack thread (the faithful reproduction of the
//! `spawn_blocking` pool the builtin actually uses).

use nika_extract::{ExtractMode, ExtractOptions, extract};

fn run(body: &str, mode: ExtractMode) -> Result<serde_json::Value, nika_extract::ExtractError> {
    let mut opts = ExtractOptions::new();
    opts.selector = Some("div");
    opts.base_url = Some("https://example.com/");
    extract(body, mode, &opts)
}

fn deep(depth: usize) -> String {
    format!("{}content{}", "<div>".repeat(depth), "</div>".repeat(depth))
}

// One test per mode: a 50 000-deep `<div>` must be REJECTED (the guard),
// FAST (early-exit, no full parse), and without aborting the process.
// Pre-fix this SIGABRTed via htmd's recursive rcdom Drop; the bisect
// shape survives so any regression names the exact mode that died.
fn assert_deep_rejected_fast(mode: ExtractMode) {
    let started = std::time::Instant::now();
    let out = run(&deep(50_000), mode);
    assert!(
        out.is_err(),
        "{mode:?}: 50k-deep must be rejected by the depth guard"
    );
    assert!(
        started.elapsed().as_millis() < 500,
        "{mode:?}: guard must early-exit, not parse — took {:?}",
        started.elapsed()
    );
}
#[test]
fn deep_nesting_markdown() {
    assert_deep_rejected_fast(ExtractMode::Markdown);
}
#[test]
fn deep_nesting_article() {
    assert_deep_rejected_fast(ExtractMode::Article);
}
#[test]
fn deep_nesting_text() {
    assert_deep_rejected_fast(ExtractMode::Text);
}
#[test]
fn deep_nesting_selector() {
    assert_deep_rejected_fast(ExtractMode::Selector);
}
#[test]
fn deep_nesting_metadata() {
    assert_deep_rejected_fast(ExtractMode::Metadata);
}
#[test]
fn deep_nesting_links() {
    assert_deep_rejected_fast(ExtractMode::Links);
}

/// `<div/>` is NOT self-closing in HTML5 (html5ever nests it) — a
/// `/>`-honoring guard would let 50 000 of them through to the crash.
/// The byte-scan must count them as nesting.
#[test]
fn self_closing_div_bypass_is_closed() {
    let bypass = "<div/>".repeat(50_000);
    assert!(
        run(&bypass, ExtractMode::Markdown).is_err(),
        "<div/>×50000 must be rejected — HTML5 nests it"
    );
}

/// Void elements never nest: 50 000 `<br>`/`<img>` is a FLAT document
/// and must be ACCEPTED (no false-positive rejection of legit content).
#[test]
fn void_element_flood_is_accepted_not_rejected() {
    let brs = format!("<html><body>{}</body></html>", "<br>".repeat(50_000));
    assert!(
        run(&brs, ExtractMode::Text).is_ok(),
        "50k <br> is flat, not deep"
    );
}

/// Markup-like content inside a COMMENT is text, not nesting — a legit
/// page with a big commented-out template must NOT be falsely rejected
/// (the depth guard skips `<!-- … -->` wholesale).
#[test]
fn markup_inside_comment_is_not_counted() {
    let body = format!(
        "<html><body><!-- {} --><p>real</p></body></html>",
        "<div>".repeat(50_000)
    );
    assert!(
        run(&body, ExtractMode::Text).is_ok(),
        "50k <div> inside a comment is text, not depth"
    );
}

/// Markup-like strings inside `<script>`/`<style>` (JSON-LD, hydration
/// data, CSS combinators, JS template literals) are RAWTEXT, never
/// nested elements — must NOT trip the depth guard.
#[test]
fn markup_inside_script_and_style_is_not_counted() {
    let script = format!(
        "<html><head><script>var x = `{}`;</script></head><body><p>real</p></body></html>",
        "<div>".repeat(50_000)
    );
    assert!(
        run(&script, ExtractMode::Text).is_ok(),
        "50k <div> inside <script> is rawtext"
    );
    let style = format!(
        "<html><head><style>{}</style></head><body><p>x</p></body></html>",
        "a > b { color: red } ".repeat(20_000)
    );
    assert!(
        run(&style, ExtractMode::Text).is_ok(),
        "CSS `>` combinators inside <style> are rawtext"
    );
    // …but a REAL deep tree AFTER a script still gets caught (the skip
    // doesn't swallow past the matching close tag).
    let mixed = format!(
        "<html><body><script>x</script>{}deep</body></html>",
        "<div>".repeat(50_000)
    );
    assert!(
        run(&mixed, ExtractMode::Text).is_err(),
        "real nesting after a script is still rejected"
    );
}

/// THE rawtext-bypass P0 (verified 2026-06-12, rust-pro swarm). A naive
/// close-counting byte scan pops on the `</div>` INSIDE the `<script>` —
/// so `<div><script></div></script>` nets to depth 0 and the scan stays
/// flat forever, waving an unbounded document through to htmd's recursive
/// rcdom `Drop` (SIGABRT at ~56 KiB). The real parser treats that
/// `</div>` as script text and nests +1 per unit. The guard MUST see the
/// same: jump from `<script>` to its matching `</script>`, never counting
/// the inner close — so the document is rejected at the cap, FAST.
#[test]
fn rawtext_close_tag_bypass_is_rejected() {
    let exploit = "<div><script></div></script>".repeat(50_000);
    let started = std::time::Instant::now();
    assert!(
        run(&exploit, ExtractMode::Markdown).is_err(),
        "<div><script></div></script>×N must be rejected — the inner \
         </div> is script text, the <div> really nests"
    );
    assert!(
        started.elapsed().as_millis() < 500,
        "guard must early-exit on the rawtext bypass, took {:?}",
        started.elapsed()
    );
}

/// A bare `<plaintext>` makes the REST of the document text — no markup
/// after it ever nests. 50 000 `<div>` following a `<plaintext>` is flat
/// (the parser would never build a tree from them), so it must be
/// ACCEPTED, not falsely rejected.
#[test]
fn plaintext_makes_rest_text_not_depth() {
    let body = format!("<html><body><plaintext>{}", "<div>".repeat(50_000));
    assert!(
        run(&body, ExtractMode::Text).is_ok(),
        "markup after <plaintext> is text, not depth"
    );
}

/// An abrupt-closing empty comment `<!-->` ends IMMEDIATELY (HTML5). A
/// guard that scanned for a full `-->` would swallow everything after it
/// to EOF and UNDER-count — so `<!-->` then 50 000 real `<div>` must
/// still be REJECTED (the deep tree after the comment is seen).
#[test]
fn abrupt_comment_does_not_swallow_following_markup() {
    let body = format!("<!-->{}deep", "<div>".repeat(50_000));
    assert!(
        run(&body, ExtractMode::Markdown).is_err(),
        "<!--> ends at once; the <div>×50000 after it must be caught"
    );
}

/// THE foreign-content P0 (verified 2026-06-12, rust-security swarm).
/// Whether an element is RAWTEXT is CONTEXT-dependent, not name-dependent:
/// inside SVG/MathML foreign content NONE of the rawtext names tokenize as
/// text — they NEST (html5ever `step_foreign`). `<svg><title><g>×N` made a
/// name-only guard skip to a `</title>` that never comes, counting depth 2
/// while html5ever built an N-deep foreign DOM → htmd recursive-Drop
/// SIGABRT on the DEFAULT markdown path. The guard must NOT skip rawtext
/// inside foreign content — every wrapper here must be rejected, fast.
#[test]
fn foreign_content_rawtext_bypass_is_rejected() {
    let wrappers = [
        "<svg><title>",
        "<svg><style>",
        "<svg><script>",
        "<svg><xmp>",
        "<svg><iframe>",
        "<svg><noembed>",
        "<svg><noframes>",
        "<svg><textarea>",
        "<svg><plaintext>",
        "<math><title>",
        "<html><body><svg><title>",
    ];
    for w in wrappers {
        let exploit = format!("{w}{}", "<g>".repeat(50_000));
        let started = std::time::Instant::now();
        assert!(
            run(&exploit, ExtractMode::Markdown).is_err(),
            "foreign-content bypass via {w:?} must be rejected"
        );
        assert!(
            started.elapsed().as_millis() < 500,
            "guard must early-exit on {w:?}, took {:?}",
            started.elapsed()
        );
    }
}

/// CDATA is not a skip vector. Inside SVG foreign content `<![CDATA[ … ]]>`
/// is text, but the guard treats `<!…` (non-comment) as a bogus-comment skip
/// to the FIRST `>` — it can never swallow a deep subtree the way a rawtext
/// skip-to-close could. So a CDATA-wrapped `<g>` bomb is at worst over-counted
/// (rejected), NEVER under-counted to a crash. Pins that the `<!`/`<?` arm
/// stays a one-`>` advance, not a skip-to-terminator.
#[test]
fn cdata_in_foreign_content_is_not_an_undercount_skip() {
    let body = format!("<svg><![CDATA[{}]]>", "<g>".repeat(50_000));
    let started = std::time::Instant::now();
    assert!(
        run(&body, ExtractMode::Markdown).is_err(),
        "deep markup inside CDATA in <svg> must be rejected, not crashed through"
    );
    assert!(
        started.elapsed().as_millis() < 500,
        "guard must early-exit on the CDATA bomb, took {:?}",
        started.elapsed()
    );
}

/// The integration-point refinement must NOT over-reject: inside
/// `<svg><foreignObject>` (an HTML integration point) HTML parsing RESUMES,
/// so `<style>` is rawtext again and a big CSS blob there stays shallow —
/// ACCEPTED, not falsely rejected (the skip is restored, not lost).
#[test]
fn integration_point_restores_rawtext_skip() {
    let body = format!(
        "<svg><foreignObject><style>{}</style></foreignObject></svg>",
        "a > b {{ color: red }} ".repeat(20_000)
    );
    assert!(
        run(&body, ExtractMode::Text).is_ok(),
        "foreignObject restores HTML context — style is rawtext again, shallow"
    );
}

/// Microdata's recursive item walk must stay bounded on hostile input.
/// 1000 nested `itemscope`s pass the 2048 depth guard (so the walk runs),
/// but the `MAX_MICRODATA_DEPTH` cap must stop recursion well before any
/// stack risk — bounded, no panic, no hang. Plus a flat flood of items
/// (the `MAX_MICRODATA_ITEMS` cap). metadata mode is TOTAL either way.
#[test]
fn microdata_nesting_and_flood_are_bounded() {
    // Deep nested-item chain (< guard cap, exercises the depth limiter).
    let deep = format!(
        "<div itemscope itemtype=\"https://schema.org/Thing\">{}{}",
        "<div itemprop=\"part\" itemscope>".repeat(1_000),
        "<span itemprop=\"n\">x</span></div>".repeat(1_000)
    );
    let started = std::time::Instant::now();
    let out = run(&deep, ExtractMode::Metadata);
    assert!(out.is_ok(), "deep microdata nesting must be total");
    assert!(
        started.elapsed().as_secs() < 5,
        "depth cap bounds the walk, took {:?}",
        started.elapsed()
    );
    // Flat flood of top-level items (exercises the item-count cap).
    let flood = "<div itemscope itemtype=\"https://schema.org/X\"></div>".repeat(50_000);
    let out = run(&flood, ExtractMode::Metadata);
    let count = out
        .as_ref()
        .ok()
        .and_then(|v| v.get("microdata"))
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    assert!(out.is_ok(), "flood is total");
    assert!(count <= 64, "item count is capped: {count}");
}

/// The article rule cascade (zone targeting + prune-by-detach + link-density)
/// must stay bounded on a hostile page: a flood of discard-class divs (the
/// detach set), deeply-nested article markup, and link-dense blocks. The
/// depth guard caps nesting upstream, but the prune/zone walks must not blow
/// up on width. Total + fast, no panic.
#[test]
fn article_rule_cascade_is_bounded_on_hostile_input() {
    // 30k sibling boilerplate divs around a real article — the discard
    // select + outermost-dedup + detach must handle the width.
    let junk = r#"<div class="share-widget"><a href="/x">share</a></div>"#.repeat(30_000);
    let body = format!(
        r#"<html><body>{junk}<article class="post-content">
           <h1>Title</h1><p>{}</p></article>{junk}</body></html>"#,
        "real article prose with plenty of running words for content mass ".repeat(20)
    );
    let started = std::time::Instant::now();
    let out = run(&body, ExtractMode::Article);
    assert!(out.is_ok(), "hostile article page must be total");
    // The subject is NO BLOWUP on width — quadratic over 30k siblings
    // would run for hours anywhere. The bound is a blowup tripwire, not
    // a speed pin: 10s was a fast-mac debug number (observed 14.8s on
    // the 2-core CI runner the day this suite first ran there).
    assert!(
        started.elapsed().as_secs() < 60,
        "rule cascade bounded, took {:?}",
        started.elapsed()
    );
    // The article body still wins through the noise.
    assert!(
        out.unwrap_or_default()
            .as_str()
            .is_some_and(|m| m.contains("real article prose")),
        "article body extracted despite the flood"
    );
}

/// Deeply-nested anchors stress the boilerpipe/links anchor-depth walk.
#[test]
fn deep_nested_anchors_are_total() {
    let depth = 20_000;
    let body = format!(
        "<body>{}link{}</body>",
        "<a href=\"/x\">".repeat(depth),
        "</a>".repeat(depth)
    );
    let _ = run(&body, ExtractMode::Article);
    let _ = run(&body, ExtractMode::Links);
}

/// Billion-laughs in a sitemap: nested custom ENTITY declarations must
/// NOT expand exponentially. quick-xml emits unexpanded `GeneralRef`
/// tokens; our handler resolves predefined entities + numeric refs only
/// and preserves unknown ones VERBATIM — so `&lol9;` stays 5 bytes, it
/// never expands to 10^9 'a's. Bounded time + bounded output.
#[test]
fn sitemap_billion_laughs_does_not_expand() {
    let bomb = r#"<?xml version="1.0"?>
<!DOCTYPE urlset [
  <!ENTITY lol "lol">
  <!ENTITY lol1 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
  <!ENTITY lol2 "&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;">
  <!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;">
  <!ENTITY lol4 "&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;">
  <!ENTITY lol5 "&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;">
  <!ENTITY lol6 "&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;">
  <!ENTITY lol7 "&lol6;&lol6;&lol6;&lol6;&lol6;&lol6;&lol6;&lol6;&lol6;&lol6;">
  <!ENTITY lol8 "&lol7;&lol7;&lol7;&lol7;&lol7;&lol7;&lol7;&lol7;&lol7;&lol7;">
  <!ENTITY lol9 "&lol8;&lol8;&lol8;&lol8;&lol8;&lol8;&lol8;&lol8;&lol8;&lol8;">
]>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/&lol9;</loc></url>
</urlset>"#;
    let started = std::time::Instant::now();
    let out = extract(bomb, ExtractMode::Sitemap, &ExtractOptions::new());
    // The bound that matters: it RETURNED (no hang) and the output is
    // tiny (no 10^9-byte expansion). Either an Ok with the verbatim
    // entity preserved, or a typed parse error — never a memory bomb.
    assert!(
        started.elapsed().as_secs() < 5,
        "billion-laughs must not hang"
    );
    if let Ok(serde_json::Value::Array(entries)) = out {
        for entry in entries {
            let loc = entry["loc"].as_str().unwrap_or("");
            assert!(
                loc.len() < 10_000,
                "loc must not expand exponentially: {} bytes",
                loc.len()
            );
        }
    }
}

/// Billion-laughs in a feed (feed-rs path): same non-expansion bound.
#[test]
fn feed_billion_laughs_does_not_expand() {
    let bomb = r#"<?xml version="1.0"?>
<!DOCTYPE feed [
  <!ENTITY lol "lol">
  <!ENTITY lol1 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;">
  <!ENTITY lol2 "&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;&lol1;">
  <!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;">
  <!ENTITY lol4 "&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;">
  <!ENTITY lol5 "&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;&lol4;">
  <!ENTITY lol6 "&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;&lol5;">
]>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>&lol6;</title>
  <entry><title>x</title></entry>
</feed>"#;
    let started = std::time::Instant::now();
    let out = extract(bomb, ExtractMode::Feed, &ExtractOptions::new());
    assert!(started.elapsed().as_secs() < 5, "feed bomb must not hang");
    if let Ok(value) = out {
        let title = value["title"].as_str().unwrap_or("");
        assert!(
            title.len() < 1_000_000,
            "title must not expand: {}",
            title.len()
        );
    }
}

/// XXE: a feed/sitemap referencing an EXTERNAL entity (file/URL) must
/// NOT read the file or fetch the URL (no XXE → no local-file leak, no
/// SSRF via the parser).
#[test]
fn external_entity_is_not_resolved() {
    let xxe = r#"<?xml version="1.0"?>
<!DOCTYPE urlset [ <!ENTITY xxe SYSTEM "file:///etc/passwd"> ]>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/&xxe;</loc></url>
</urlset>"#;
    let out = extract(xxe, ExtractMode::Sitemap, &ExtractOptions::new());
    if let Ok(serde_json::Value::Array(entries)) = out {
        for entry in entries {
            let loc = entry["loc"].as_str().unwrap_or("");
            assert!(
                !loc.contains("root:"),
                "must NOT have read /etc/passwd: {loc}"
            );
            assert!(
                !loc.contains("/bin/"),
                "no passwd contents leaked into the loc: {loc}"
            );
        }
    }
}

/// A huge FLAT document (10 MB of sibling paragraphs) extracts in
/// bounded time without quadratic blowup.
#[test]
fn huge_flat_document_is_linear() {
    let para = "<p>paragraph with several plain words in it for density</p>";
    let body = format!("<html><body>{}</body></html>", para.repeat(150_000));
    let started = std::time::Instant::now();
    let _ = run(&body, ExtractMode::Markdown);
    let _ = run(&body, ExtractMode::Text);
    assert!(
        started.elapsed().as_secs() < 30,
        "10 MB flat doc must stay roughly linear, took {:?}",
        started.elapsed()
    );
}

/// A pathological selector matching deeply-nested elements must hit the
/// output ceiling, not OOM (the O(N²) serialization guard).
#[test]
fn pathological_selector_hits_the_ceiling_not_oom() {
    let depth = 30_000;
    let body = format!("{}x{}", "<div>".repeat(depth), "</div>".repeat(depth));
    let mut opts = ExtractOptions::new();
    opts.selector = Some("div");
    // Either a bounded string or the typed ceiling error — never OOM.
    let _ = extract(&body, ExtractMode::Selector, &opts);
}
