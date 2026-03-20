# PR5 — Fetch Upgrade + HTML Extraction

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `extract:` and `selector:` fields to the `fetch:` verb so workflows can get clean markdown, article text, or CSS-selected content instead of raw HTML.

**Architecture:** Thread two new optional fields (`extract`, `selector`) through the 3-phase AST pipeline (Raw → Analyzed → Lower → Runtime), then post-process the HTTP response body in `run_fetch` using `scraper` (CSS selectors), `htmd` (HTML→Markdown), and `dom_smoothie` (article extraction). Also add 3 builtin tools (`nika:html_to_md`, `nika:css_select`, `nika:readability`) for use in `invoke:` pipelines.

**Tech Stack:** scraper 0.26 (14.8M downloads), htmd 0.5 (248K), dom_smoothie 0.16 (16K), reqwest 0.12 (existing)

**Baseline:** 6255 tests, 0 clippy warnings, 0 failures

---

## New Dependencies

```toml
# Cargo.toml [dependencies]
scraper = { version = "0.26", optional = true }
htmd = { version = "0.5", optional = true }
dom_smoothie = { version = "0.16", optional = true }

# Cargo.toml [features]
fetch-extract = ["dep:scraper", "dep:htmd"]               # extract: markdown|text|selector
fetch-readability = ["dep:dom_smoothie", "dep:scraper"]    # extract: article
```

## YAML Syntax (what users write)

```yaml
# Extract clean markdown from HTML page
fetch:
  url: "https://blog.example.com/article"
  extract: markdown

# Extract article content only (like Readability)
fetch:
  url: "https://news.site.com/story"
  extract: article

# CSS selector extraction
fetch:
  url: "https://example.com/products"
  selector: "div.product-card h2"
  extract: text

# Backward compatible (no extract = raw body as before)
fetch:
  url: "https://api.example.com/data"
  method: GET
```

---

## Phase 1: AST Pipeline (extract + selector fields)

### Task 1.0: Add fields to RawFetchAction

**Files:**
- Modify: `src/ast/raw/action.rs:103-126` — add 2 fields to struct
- Modify: `src/ast/raw/parser.rs:644-679` — parse new fields

**Step 1: Add fields to RawFetchAction**

In `src/ast/raw/action.rs`, add after `follow_redirects` (line ~123):

```rust
/// Post-processing extraction mode: markdown, article, text, selector
pub extract: Option<Spanned<String>>,

/// CSS selector for element extraction (used with extract: text or extract: selector)
pub selector: Option<Spanned<String>>,
```

**Step 2: Parse new fields in parser**

In `src/ast/raw/parser.rs`, inside `parse_fetch_action` mapping branch (after `follow_redirects` parsing, around line 675):

```rust
let extract = get_string_field(file, m, "extract")?;
let selector = get_string_field(file, m, "selector")?;
```

And add to the `Ok(RawFetchAction { ... })` struct construction:

```rust
extract,
selector,
```

**Step 3: Run tests**

```bash
cargo test --lib -q -- ast::raw::parser::tests
```

Expected: all existing tests pass (new fields are Option, default None)

**Step 4: Commit**

```
feat(ast): add extract and selector fields to RawFetchAction
```

---

### Task 1.1: Thread through Analyzed + Lower

**Files:**
- Modify: `src/ast/analyzed/task.rs:178-204` — add fields to AnalyzedFetchAction
- Modify: `src/ast/analyzer/analyze.rs:711-741` — map raw→analyzed
- Modify: `src/ast/lower.rs:198-210` — lower to FetchParams
- Modify: `src/ast/action.rs:338-404` — add to FetchParams + Deserialize

**Step 1: AnalyzedFetchAction**

In `src/ast/analyzed/task.rs`, add after `follow_redirects` field:

```rust
/// Post-processing extraction: "markdown", "article", "text", "selector"
pub extract: Option<String>,

/// CSS selector for targeted extraction
pub selector: Option<String>,
```

**Step 2: analyze_fetch**

In `src/ast/analyzer/analyze.rs`, inside `analyze_fetch` (around line 738):

```rust
extract: raw.extract.as_ref().map(|s| s.value.clone()),
selector: raw.selector.as_ref().map(|s| s.value.clone()),
```

**Step 3: FetchParams (runtime)**

In `src/ast/action.rs`, add to `FetchParams` struct (after `follow_redirects`):

```rust
/// Post-processing extraction mode
#[serde(default)]
pub extract: Option<String>,

/// CSS selector for element extraction
#[serde(default)]
pub selector: Option<String>,
```

Also update the `Deserialize` impl if it has a manual one (check — FetchParams uses `#[derive(Deserialize)]` so this should work automatically).

**Step 4: lower_fetch**

In `src/ast/lower.rs`, inside `lower_fetch` (around line 208):

```rust
extract: fetch.extract,
selector: fetch.selector,
```

Also update `unlower_action` for the Fetch branch to include:

```rust
extract: None,
selector: None,
```

**Step 5: Run tests**

```bash
cargo test --lib -q
```

Expected: all 6255 tests pass (new fields default to None)

**Step 6: Commit**

```
feat(ast): thread extract/selector through analyzed → lower → FetchParams
```

---

### Task 1.2: Parser validation + tests

**Files:**
- Modify: `src/ast/action.rs` — validate extract values
- Add tests to: `src/ast/tests_200_workflows.rs`

**Step 1: Add validation in FetchParams::validate()**

In `src/ast/action.rs`, inside `validate()` (around line 380), add:

```rust
// Validate extract mode
if let Some(ref extract) = self.extract {
    let valid = ["markdown", "article", "text", "selector"];
    if !valid.contains(&extract.as_str()) {
        return Err(NikaError::ValidationError {
            reason: format!(
                "fetch extract must be one of: {}, got '{}'",
                valid.join(", "), extract
            ),
        });
    }
}

// selector requires extract to be set
if self.selector.is_some() && self.extract.is_none() {
    return Err(NikaError::ValidationError {
        reason: "fetch 'selector' requires 'extract' to be set".to_string(),
    });
}
```

**Step 2: Add parser tests**

Add to `src/ast/tests_200_workflows.rs`:

```rust
#[test]
fn fetch_extract_markdown() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test
provider: mock
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: markdown
"#;
    let wf = parse_workflow(yaml).unwrap();
    match &wf.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract.as_deref(), Some("markdown"));
            assert_eq!(fetch.selector, None);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn fetch_extract_with_selector() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test
provider: mock
tasks:
  - id: scrape
    fetch:
      url: "https://example.com"
      extract: text
      selector: "article.main h2"
"#;
    let wf = parse_workflow(yaml).unwrap();
    match &wf.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract.as_deref(), Some("text"));
            assert_eq!(fetch.selector.as_deref(), Some("article.main h2"));
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn fetch_no_extract_backward_compat() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test
provider: mock
tasks:
  - id: api
    fetch:
      url: "https://api.example.com"
"#;
    let wf = parse_workflow(yaml).unwrap();
    match &wf.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.extract, None);
            assert_eq!(fetch.selector, None);
        }
        _ => panic!("expected Fetch"),
    }
}

#[test]
fn fetch_extract_invalid_value_error() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test
provider: mock
tasks:
  - id: bad
    fetch:
      url: "https://example.com"
      extract: xml
"#;
    let wf = parse_workflow(yaml).unwrap();
    match &wf.tasks[0].action {
        TaskAction::Fetch { fetch } => {
            assert!(fetch.validate().is_err());
        }
        _ => panic!("expected Fetch"),
    }
}
```

**Step 3: Run tests**

```bash
cargo test --lib -q -- ast::tests_200_workflows::fetch_extract
cargo test --lib -q
```

**Step 4: Commit**

```
feat(ast): validate extract/selector + parser tests for fetch upgrade
```

---

## Phase 2: Runtime — HTML Extraction

### Task 2.0: Add scraper + htmd dependencies

**Files:**
- Modify: `Cargo.toml` — add dependencies + feature flags

**Step 1: Add to Cargo.toml**

```toml
# Under [dependencies]
scraper = { version = "0.26", optional = true }
htmd = { version = "0.5", optional = true }
dom_smoothie = { version = "0.16", optional = true }

# Under [features]
fetch-extract = ["dep:scraper", "dep:htmd"]
fetch-readability = ["dep:dom_smoothie", "dep:scraper"]
```

**Step 2: Verify compilation**

```bash
cargo check --features fetch-extract
cargo check --features fetch-readability
cargo check  # default features still work
```

**Step 3: Commit**

```
chore(deps): add scraper, htmd, dom_smoothie for fetch extraction
```

---

### Task 2.1: Implement extraction in run_fetch

**Files:**
- Modify: `src/runtime/executor/verbs.rs:1021` — add post-processing after response.text()

**Step 1: Add extraction module**

Create `src/runtime/executor/extract.rs`:

```rust
//! HTML extraction utilities for the fetch: verb.
//!
//! Supports 4 extraction modes:
//! - `markdown`: Convert full HTML to clean Markdown (htmd)
//! - `article`: Extract main article content (dom_smoothie readability)
//! - `text`: Extract visible text only (scraper)
//! - `selector`: Extract elements matching CSS selector (scraper)

use crate::error::NikaError;

/// Apply extraction to raw HTML body.
///
/// Returns the processed text, or the original body if no extraction is configured.
pub fn apply_extract(
    body: &str,
    extract: Option<&str>,
    selector: Option<&str>,
) -> Result<String, NikaError> {
    match extract {
        None => Ok(body.to_string()),

        #[cfg(feature = "fetch-extract")]
        Some("markdown") => {
            let md = htmd::HtmlToMarkdown::new().convert(body)
                .map_err(|e| NikaError::Execution(format!("HTML to markdown failed: {e}")))?;
            Ok(md)
        }

        #[cfg(feature = "fetch-extract")]
        Some("text") => {
            let document = scraper::Html::parse_document(body);
            if let Some(css) = selector {
                let sel = scraper::Selector::parse(css)
                    .map_err(|e| NikaError::Execution(format!("Invalid CSS selector '{}': {:?}", css, e)))?;
                let texts: Vec<String> = document.select(&sel)
                    .map(|el| el.text().collect::<Vec<_>>().join(" "))
                    .collect();
                Ok(texts.join("\n"))
            } else {
                // No selector — extract all visible text
                Ok(document.root_element().text().collect::<Vec<_>>().join(" "))
            }
        }

        #[cfg(feature = "fetch-extract")]
        Some("selector") => {
            let css = selector.ok_or_else(|| NikaError::Execution(
                "extract: selector requires 'selector' field".to_string()
            ))?;
            let document = scraper::Html::parse_document(body);
            let sel = scraper::Selector::parse(css)
                .map_err(|e| NikaError::Execution(format!("Invalid CSS selector '{}': {:?}", css, e)))?;
            let html_parts: Vec<String> = document.select(&sel)
                .map(|el| el.html())
                .collect();
            Ok(html_parts.join("\n"))
        }

        #[cfg(feature = "fetch-readability")]
        Some("article") => {
            let doc = dom_smoothie::Readability::new(body)
                .map_err(|e| NikaError::Execution(format!("Readability parse failed: {e}")))?;
            let article = doc.parse();
            Ok(article.text_content)
        }

        Some(unknown) => Err(NikaError::Execution(format!(
            "Unknown extract mode '{}'. Available: markdown, article, text, selector",
            unknown
        ))),
    }
}
```

**Step 2: Register module**

In `src/runtime/executor/mod.rs`, add:

```rust
mod extract;
```

**Step 3: Wire into run_fetch**

In `src/runtime/executor/verbs.rs`, replace line 1021:

```rust
// BEFORE (line 1021):
return response.text().await.map_err(|e| {
    NikaError::Execution(format!("Failed to read response: {}", e))
});

// AFTER:
let raw_body = response.text().await.map_err(|e| {
    NikaError::Execution(format!("Failed to read response: {}", e))
})?;
return extract::apply_extract(
    &raw_body,
    fetch.extract.as_deref(),
    fetch.selector.as_deref(),
);
```

**Step 4: Run tests**

```bash
cargo test --lib -q
cargo test --lib --features fetch-extract -q
```

**Step 5: Commit**

```
feat(runtime): implement extract/selector post-processing in fetch verb
```

---

### Task 2.2: E2E tests with wiremock

**Files:**
- Modify: `src/runtime/executor/tests.rs` — add fetch extraction tests

**Step 1: Write extraction tests**

```rust
#[cfg(feature = "fetch-extract")]
mod fetch_extract_tests {
    use super::*;

    const HTML_PAGE: &str = r#"
    <html>
    <head><title>Test Page</title></head>
    <body>
        <article>
            <h1>Hello World</h1>
            <p>This is a <strong>test</strong> paragraph.</p>
            <ul><li>Item 1</li><li>Item 2</li></ul>
        </article>
        <footer>Copyright 2026</footer>
    </body>
    </html>
    "#;

    #[test]
    fn extract_markdown_from_html() {
        let result = crate::runtime::executor::extract::apply_extract(
            HTML_PAGE, Some("markdown"), None,
        ).unwrap();
        assert!(result.contains("# Hello World"), "should have markdown heading");
        assert!(result.contains("**test**"), "should have bold text");
        assert!(!result.contains("<html>"), "should not have HTML tags");
    }

    #[test]
    fn extract_text_all() {
        let result = crate::runtime::executor::extract::apply_extract(
            HTML_PAGE, Some("text"), None,
        ).unwrap();
        assert!(result.contains("Hello World"));
        assert!(result.contains("test paragraph"));
        assert!(!result.contains("<h1>"));
    }

    #[test]
    fn extract_text_with_selector() {
        let result = crate::runtime::executor::extract::apply_extract(
            HTML_PAGE, Some("text"), Some("article h1"),
        ).unwrap();
        assert_eq!(result.trim(), "Hello World");
    }

    #[test]
    fn extract_selector_html() {
        let result = crate::runtime::executor::extract::apply_extract(
            HTML_PAGE, Some("selector"), Some("article ul li"),
        ).unwrap();
        assert!(result.contains("Item 1"));
        assert!(result.contains("Item 2"));
    }

    #[test]
    fn extract_none_returns_raw() {
        let result = crate::runtime::executor::extract::apply_extract(
            HTML_PAGE, None, None,
        ).unwrap();
        assert!(result.contains("<html>"), "no extract = raw HTML");
    }

    #[test]
    fn extract_invalid_selector_error() {
        let result = crate::runtime::executor::extract::apply_extract(
            HTML_PAGE, Some("text"), Some("[[[invalid"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn extract_unknown_mode_error() {
        let result = crate::runtime::executor::extract::apply_extract(
            HTML_PAGE, Some("xml"), None,
        );
        assert!(result.is_err());
    }
}
```

**Step 2: Run tests**

```bash
cargo test --lib --features fetch-extract -q -- fetch_extract_tests
cargo test --lib -q  # default features still pass
```

**Step 3: Commit**

```
test(fetch): add extraction tests for markdown, text, selector modes
```

---

## Phase 3: Builtin Tools

### Task 3.0: nika:html_to_md builtin

**Files:**
- Create: `src/runtime/builtin/media/html_to_md.rs`
- Modify: `src/runtime/builtin/media/mod.rs` — register tool

**Pattern:** Follow existing tool pattern (import.rs, phash.rs). Input: CAS hash of HTML. Output: CAS hash of Markdown.

```rust
pub struct HtmlToMdOp;

impl MediaOp for HtmlToMdOp {
    fn name(&self) -> &'static str { "html_to_md" }
    fn description(&self) -> &'static str {
        "Convert HTML content to clean Markdown"
    }
    // Input: { hash: "blake3:..." } or { html: "raw html string" }
    // Output: { markdown: "...", char_count: N }
}
```

Register behind `fetch-extract` feature gate.

**Commit:** `feat(media): add nika:html_to_md — HTML to Markdown conversion [fetch-extract]`

---

### Task 3.1: nika:css_select builtin

**Files:**
- Create: `src/runtime/builtin/media/css_select.rs`

```rust
pub struct CssSelectOp;
// Input: { hash: "blake3:...", selector: "div.content h2", output: "text"|"html" }
// Output: { matches: [...], count: N }
```

**Commit:** `feat(media): add nika:css_select — CSS selector extraction [fetch-extract]`

---

### Task 3.2: nika:readability builtin

**Files:**
- Create: `src/runtime/builtin/media/readability.rs`

```rust
pub struct ReadabilityOp;
// Input: { hash: "blake3:..." }
// Output: { title: "...", content: "...", excerpt: "...", char_count: N }
```

Register behind `fetch-readability` feature gate.

**Commit:** `feat(media): add nika:readability — article extraction [fetch-readability]`

---

## Phase 4: Documentation + Cleanup

### Task 4.0: Update CLAUDE.md

Add to Media Tools section:

```markdown
### Tier 3 — Opt-in (continued)
| `nika:html_to_md` | fetch-extract | Convert HTML to clean Markdown |
| `nika:css_select` | fetch-extract | Extract elements via CSS selectors |
| `nika:readability` | fetch-readability | Extract article content (Readability) |
```

Add to Fetch verb docs:

```markdown
## Fetch Extraction (v0.35.0 — PR5)

The `fetch:` verb supports optional `extract:` and `selector:` for HTML processing:

- `extract: markdown` — Clean Markdown via htmd
- `extract: article` — Main article content via dom_smoothie
- `extract: text` — Visible text only (optionally filtered by `selector:`)
- `extract: selector` — Raw HTML of matching elements
- No extract — Raw body (backward compatible)
```

### Task 4.1: Update tool count + tests_e2e_workflow.rs

Update router tool count test for new tools.

### Task 4.2: Example workflows

Create `examples/fetch-extract.nika.yaml`:

```yaml
schema: "nika/workflow@0.12"
workflow: web-to-summary
description: "Fetch a web page, extract article, summarize with LLM"
provider: claude
model: claude-sonnet-4-6

tasks:
  - id: fetch_page
    fetch:
      url: "https://blog.example.com/article"
      extract: markdown

  - id: summarize
    infer:
      prompt: |
        Summarize this article in 3 bullet points:

        {{with.page}}
    with:
      page: $fetch_page
    depends_on: [fetch_page]
```

---

## Verification Checklist

```bash
# Phase 1: AST
cargo test --lib -q -- ast::tests_200_workflows::fetch_extract    # New tests
cargo test --lib -q                                                # No regression

# Phase 2: Runtime
cargo test --lib --features fetch-extract -q                       # Extract tests
cargo test --lib --features fetch-readability -q                   # Readability tests
cargo test --lib -q                                                # Default still works

# Phase 3: Builtins
cargo test --lib --features fetch-extract -q -- html_to_md         # Tool tests
cargo test --lib --features fetch-extract -q -- css_select
cargo test --lib --features fetch-readability -q -- readability

# Phase 4: Full
cargo clippy -- -D warnings                                        # 0 warnings
cargo clippy --features "fetch-extract,fetch-readability" -- -D warnings
cargo check --no-default-features --features fetch-extract         # Isolated
cargo check --no-default-features --features fetch-readability     # Isolated
```

---

## Bonus: Fix OPTIONS method bug

Found during analysis: `run_fetch` line 912-924 has no `OPTIONS` branch — it falls through to GET silently.

Add between HEAD and the default GET:

```rust
} else if fetch.method.eq_ignore_ascii_case("OPTIONS") {
    http_client.request(reqwest::Method::OPTIONS, url.as_ref())
```

**Commit:** `fix(runtime): add missing OPTIONS method in fetch verb`

---

## Success Criteria

- `fetch:` with no `extract:` works exactly as before (zero regression)
- `extract: markdown` converts HTML to clean Markdown
- `extract: article` extracts main content (skips nav, footer, ads)
- `extract: text` + `selector:` extracts targeted text
- 3 new builtin tools available via `invoke: nika:*`
- All feature combos compile independently
- 6255+ existing tests pass + 10+ new tests
