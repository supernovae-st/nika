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
