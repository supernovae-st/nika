# Research Report: Rust HTML Processing Crates for Nika

**Date**: 2026-03-19
**Crates investigated**: `scraper` 0.26.0, `htmd` 0.5.1, `dom_smoothie` 0.16.0
**Purpose**: Integration into Nika's YAML workflow engine (fetch: verb post-processing)

---

## Executive Summary

Three complementary crates form a complete HTML processing pipeline:
1. **scraper** -- parse HTML and query with CSS selectors (extract specific data)
2. **htmd** -- convert HTML to clean Markdown (LLM-friendly output)
3. **dom_smoothie** -- extract main article content from noisy web pages (readability)

They can be composed: `fetch:` raw HTML --> `dom_smoothie` (extract article) --> `htmd` (to markdown) or `scraper` (structured extraction).

---

## 1. scraper (v0.26.0)

**Repository**: https://github.com/rust-scraper/scraper
**License**: ISC
**Downloads**: ~14.8M total
**Documentation coverage**: 98%

### What It Does

Wraps Servo's `html5ever` parser and `selectors` crate to provide jQuery-like CSS selector querying over parsed HTML documents. Browser-grade HTML parsing (handles malformed HTML gracefully).

### Complete API Surface

#### Core Types

| Type | Description |
|------|-------------|
| `Html` | Parsed HTML tree (document or fragment) |
| `Selector` | Compiled CSS selector (comma-separated group) |
| `ElementRef<'a>` | Borrowed reference to an element node |
| `Node` | Enum: Document, Fragment, Doctype, Comment, Text, Element, ProcessingInstruction |
| `Element` | Element data: name, attributes, id, classes |
| `Select<'a, 'b>` | Iterator over matched elements (from `Html::select`) |
| `element_ref::Select<'a, 'b>` | Iterator over matched descendants (from `ElementRef::select`) |
| `Text<'a>` | Iterator over descendant text nodes |
| `HtmlTreeSink` | TreeSink adapter for html5ever (DOM manipulation) |
| `Selectable` | Trait abstracting over Html and ElementRef |

#### `Html` Methods

```rust
Html::new_document() -> Html           // Empty document
Html::new_fragment() -> Html           // Empty fragment
Html::parse_document(&str) -> Html     // Parse full HTML document
Html::parse_fragment(&str) -> Html     // Parse HTML fragment
html.select(&Selector) -> Select      // Query matching elements
html.root_element() -> ElementRef      // Get <html> root
html.html() -> String                  // Serialize back to HTML
```

#### `Selector` Methods

```rust
Selector::parse(&str) -> Result<Selector, SelectorErrorKind>  // Compile CSS selector
selector.matches(&ElementRef) -> bool                          // Test if element matches
selector.matches_with_scope(&ElementRef, Option<ElementRef>) -> bool  // Match with :scope
```

Also implements `TryFrom<&str>`, `ToCss`, and optionally `Serialize`/`Deserialize`.

#### `ElementRef<'a>` Methods

```rust
ElementRef::wrap(NodeRef) -> Option<ElementRef>  // Wrap a node (must be Element)
el.value() -> &Element                            // Access underlying Element
el.select(&Selector) -> Select                    // Query descendants
el.html() -> String                               // Outer HTML
el.inner_html() -> String                         // Inner HTML
el.attr(&str) -> Option<&str>                     // Get attribute value
el.text() -> Text                                 // Iterator over descendant text
el.child_elements() -> impl Iterator<Item = ElementRef>      // Direct child elements
el.descendent_elements() -> impl Iterator<Item = ElementRef> // All descendant elements
```

Deref to `NodeRef<'a, Node>` (exposes `parent()`, `children()`, `next_siblings()`, `prev_siblings()`, `traverse()`, `id()`, etc. from ego-tree).

#### `Element` Methods

```rust
element.name() -> &str                           // Tag name (e.g., "div")
element.id() -> Option<&str>                     // id attribute (cached)
element.classes() -> Classes                      // Iterator over class names (cached, sorted, deduped)
element.has_class(&str, CaseSensitivity) -> bool  // Check class membership
element.attr(&str) -> Option<&str>                // Get any attribute value
```

#### `Node` Enum Variants and Methods

```rust
Node::Document | Node::Fragment | Node::Doctype(Doctype) | Node::Comment(Comment)
Node::Text(Text) | Node::Element(Element) | Node::ProcessingInstruction(ProcessingInstruction)

node.is_document() / is_fragment() / is_doctype() / is_comment() / is_text() / is_element()
node.as_doctype() / as_comment() / as_text() / as_element() / as_processing_instruction()
```

### CSS Selectors Supported

Uses Servo's `selectors` crate (CSS Selectors Level 4 partial implementation):

| Category | Supported | Examples |
|----------|-----------|----------|
| Type selectors | Yes | `div`, `p`, `a` |
| Class selectors | Yes | `.foo`, `.bar.baz` |
| ID selectors | Yes | `#main` |
| Universal selector | Yes | `*` |
| Attribute selectors | Yes | `[href]`, `[type="text"]`, `[class~="foo"]`, `[href^="https"]`, `[src$=".png"]`, `[data*="val"]` |
| Descendant combinator | Yes | `div p` |
| Child combinator | Yes | `div > p` |
| Adjacent sibling | Yes | `h1 + p` |
| General sibling | Yes | `h1 ~ p` |
| Grouping (comma) | Yes | `h1, h2, h3` |
| `:first-child` | Yes | `li:first-child` |
| `:last-child` | Yes | `li:last-child` |
| `:nth-child()` | Yes | `tr:nth-child(2n+1)` |
| `:nth-last-child()` | Yes | |
| `:only-child` | Yes | |
| `:empty` | Yes | `p:empty` |
| `:root` | Yes | |
| `:scope` | Yes | Used in scoped `ElementRef::select` |
| `:not()` | Yes | `div:not(.hidden)` |
| `:is()` | Yes | `:is(h1, h2, h3)` |
| `:where()` | Yes | `:where(h1, h2, h3)` |
| `:has()` | Yes | `div:has(> img)` |
| Namespace selectors | Yes | |
| `:hover`, `:active`, etc. | **No** | Non-tree-structural pseudo-classes not supported |
| `::before`, `::after` | **No** | Pseudo-elements not supported |
| `:visited`, `:link` | **No** | Link pseudo-classes not meaningful in static HTML |

### Feature Flags

| Feature | Description |
|---------|-------------|
| `default` | `["main", "errors"]` |
| `errors` | Store parse errors in `Html.errors` |
| `atomic` | Thread-safe tendril strings (enables `Send + Sync` on `Html`) |
| `deterministic` | Order-preserving attributes via `indexmap` |
| `serde` | Serialize/Deserialize for `Selector` |
| `main` | Binary entrypoint (not needed as library) |

### Dependencies (runtime)

- `html5ever` 0.39 -- HTML parser (Servo)
- `selectors` 0.36 -- CSS selector engine (Servo)
- `cssparser` 0.36 -- CSS parser (Servo)
- `ego-tree` 0.11 -- Tree data structure
- `tendril` 0.5 -- Efficient small strings
- `precomputed-hash` 0.1

### Minimal Usage Example

```rust
use scraper::{Html, Selector};

let html = r#"<ul><li class="active">One</li><li>Two</li></ul>"#;
let doc = Html::parse_fragment(html);
let sel = Selector::parse("li.active").unwrap();

for el in doc.select(&sel) {
    let text: String = el.text().collect();
    let class = el.value().attr("class");
    println!("text={text}, class={class:?}");
    // text=One, class=Some("active")
}
```

### Performance Characteristics

- **Parsing**: O(n) single-pass html5ever parser. Very fast for typical web pages.
- **Selector compilation**: One-time cost per `Selector::parse()`. Reuse selectors across queries.
- **Querying**: `Html::select()` iterates all tree nodes, testing each against selector. O(n) per query. Uses `SelectorCaches` for nth-index caching.
- **Memory**: Uses `ego-tree` (vec-backed arena). Compact. `tendril` for zero-copy small strings.
- **Thread safety**: NOT `Send + Sync` by default. Enable `atomic` feature for thread safety (small perf cost).

### Gotchas and Limitations

1. **Not Send/Sync by default** -- `Html` uses non-atomic tendrils. Enable `atomic` feature if sharing across threads.
2. **No DOM mutation API** -- Read-only by design. For mutation, use `HtmlTreeSink` directly with `html5ever::tree_builder::TreeSink` trait (cumbersome).
3. **No pseudo-elements or dynamic pseudo-classes** -- `NonTSPseudoClass` and `PseudoElement` are empty enums.
4. **Selector errors** -- `SelectorErrorKind` borrows the input string (lifetime `'_`), so errors cannot be stored long-term without converting to string.
5. **Attribute order** -- Not deterministic by default. Enable `deterministic` feature for `indexmap`-backed attributes.
6. **`text()` returns segments** -- Call `.collect::<String>()` or `.collect::<Vec<_>>()`. Does not insert spaces between sibling text nodes.

---

## 2. htmd (v0.5.1)

**Repository**: https://github.com/letmutex/htmd
**License**: Apache-2.0
**Downloads**: ~248K total
**Inspired by**: turndown.js (passes all turndown.js test cases)

### What It Does

Converts HTML strings to Markdown. Inspired by and faithful to turndown.js behavior. Handles tables, code blocks, links, images, emphasis, headings, lists, blockquotes, and more.

### Complete API Surface

#### Core Types

| Type | Description |
|------|-------------|
| `HtmlToMarkdown` | Main converter |
| `HtmlToMarkdownBuilder` | Builder for custom configuration |
| `Element<'a>` | DOM element passed to handlers (tag, attrs, node) |
| `Options` | Conversion options (heading style, link style, code block style, etc.) |
| `ElementHandler` (trait) | Custom tag conversion handler |
| `Handlers` (trait) | Access to handler chain (fallback, handle, walk_children, options) |
| `HandlerResult` | Result from an element handler (content + markdown_translated flag) |

#### Free Function

```rust
htmd::convert(html: &str) -> Result<String, std::io::Error>
```

One-shot conversion with default options.

#### `HtmlToMarkdown` Methods

```rust
HtmlToMarkdown::new() -> Self                    // Default converter
HtmlToMarkdown::builder() -> HtmlToMarkdownBuilder  // Builder pattern
converter.convert(&str) -> Result<String, std::io::Error>  // Convert HTML to Markdown
```

#### `HtmlToMarkdownBuilder` Methods

```rust
HtmlToMarkdownBuilder::new() -> Self
builder.options(Options) -> Self                  // Set conversion options
builder.skip_tags(Vec<&str>) -> Self              // Ignore specific tags
builder.add_handler(Vec<&str>, Handler) -> Self   // Custom handler for tags
builder.scripting_enabled(bool) -> Self           // Control <noscript> parsing
builder.build() -> HtmlToMarkdown                 // Finalize
```

#### `Element<'a>` Fields

```rust
element.node: &Rc<Node>          // html5ever node
element.tag: &str                // Tag name
element.attrs: &[Attribute]      // Attribute list
element.markdown_translated: bool // Whether children were markdown-translated
```

#### `Handlers` Trait

```rust
fn fallback(&self, element: Element) -> Option<HandlerResult>  // Delegate to previous handler
fn handle(&self, node: &Rc<Node>) -> Option<HandlerResult>     // Process a node
fn walk_children(&self, node: &Rc<Node>) -> HandlerResult      // Walk and convert children
fn options(&self) -> &Options                                   // Get current options
```

#### `ElementHandler` Trait

```rust
fn handle(&self, handlers: &dyn Handlers, element: Element) -> Option<HandlerResult>
fn append(&self) -> Option<String>  // Optional: append content after conversion (default: None)
```

Closures `Fn(&dyn Handlers, Element) -> Option<HandlerResult>` auto-implement `ElementHandler`.

#### `Options` Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `heading_style` | `HeadingStyle` | `Atx` | `#` headers vs underline (`Setex`) |
| `hr_style` | `HrStyle` | `Asterisks` | `* * *` vs `- - -` vs `_ _ _` |
| `br_style` | `BrStyle` | `TwoSpaces` | Two spaces vs backslash |
| `link_style` | `LinkStyle` | `Inlined` | `[text](url)` vs referenced vs autolinks |
| `link_reference_style` | `LinkReferenceStyle` | `Full` | Full vs collapsed vs shortcut |
| `code_block_style` | `CodeBlockStyle` | `Fenced` | Fenced vs indented |
| `code_block_fence` | `CodeBlockFence` | `Backticks` | Backticks vs tildes |
| `bullet_list_marker` | `BulletListMarker` | `Asterisk` | `*` vs `-` |
| `ul_bullet_spacing` | `u8` | `3` | Spaces after bullet |
| `ol_number_spacing` | `u8` | `2` | Spaces after number |
| `preformatted_code` | `bool` | `false` | Preserve whitespace in inline code |
| `translation_mode` | `TranslationMode` | `Pure` | Pure (drop unsupported) vs Faithful (preserve HTML) |

### Built-in Element Handlers (tag coverage)

Handles ALL common HTML elements:

| Category | Tags |
|----------|------|
| Headings | `h1`-`h6` |
| Emphasis | `strong`, `b`, `i`, `em` |
| Links | `a` (with reference link support) |
| Images | `img` |
| Lists | `ol`, `ul`, `li` |
| Code | `code`, `pre` |
| Blockquotes | `blockquote` |
| Horizontal rules | `hr` |
| Line breaks | `br` |
| Tables | `table`, `thead`, `tbody`, `tr`, `td`, `th`, `caption` |
| Paragraphs | `p` |
| Block elements | `div`, `article`, `section`, `nav`, `header`, `footer`, `main`, `aside`, `details`, `summary`, `figure`, `figcaption`, `form`, `fieldset`, `dialog`, etc. |
| Passthrough | `html`, `head`, `body`, `span` |

### What it handles well

- **Tables**: Full GFM table support with header separator row and alignment
- **Code blocks**: Fenced with language detection from class (e.g., `<code class="language-rust">`)
- **Links**: Inline, referenced, and autolink styles
- **Images**: `![alt](src "title")`
- **Nested lists**: Proper indentation
- **Faithful mode**: Preserves HTML when Markdown cannot represent it

### Dependencies (runtime, minimal)

- `html5ever` 0.36 -- HTML parser
- `markup5ever_rcdom` 0.36 -- RC-based DOM tree
- `phf` 0.13 -- Perfect hash maps (compile-time)

### Minimal Usage Example

```rust
use htmd::{HtmlToMarkdown, options::Options};

// Simple one-liner
let md = htmd::convert("<h1>Hello</h1><p>World</p>").unwrap();
assert_eq!(md, "# Hello\n\nWorld");

// With custom options and tag skipping
let converter = HtmlToMarkdown::builder()
    .options(Options {
        heading_style: htmd::options::HeadingStyle::Atx,
        link_style: htmd::options::LinkStyle::Inlined,
        ..Default::default()
    })
    .skip_tags(vec!["script", "style", "nav"])
    .build();
let md = converter.convert(html_string).unwrap();
```

### Custom Handler Example

```rust
use htmd::{Element, HtmlToMarkdown, element_handler::Handlers};

let converter = HtmlToMarkdown::builder()
    .add_handler(vec!["svg"], |_h: &dyn Handlers, _el: Element| {
        Some("[SVG Image]".into())
    })
    .add_handler(vec!["video"], |handlers: &dyn Handlers, el: Element| {
        let src = el.attrs.iter()
            .find(|a| &*a.name.local == "src")
            .map(|a| a.value.to_string())
            .unwrap_or_default();
        Some(format!("[Video: {src}]").into())
    })
    .build();
```

### Performance Characteristics

- **Speed**: ~16ms to convert a 1.37MB Wikipedia page (Apple M4). Very fast.
- **Memory**: Single-pass tree walk. RC-based DOM (no arena). Moderate memory.
- **Thread safety**: `HtmlToMarkdown` is `Send + Sync` with built-in handlers. Custom stateful handlers need manual synchronization.
- **Allocation**: String concatenation during walk. No streaming output.

### Gotchas and Limitations

1. **Error type** -- Returns `std::io::Error` (from html5ever read), not a custom error type. Conversion logic itself is infallible.
2. **No CSS selector API** -- Cannot target specific elements for conversion. It converts the ENTIRE document. To convert only part, pre-extract with `scraper` or `dom_smoothie` first.
3. **html5ever version mismatch** -- Uses `html5ever` 0.36 and `markup5ever_rcdom` 0.36, while `scraper` uses `html5ever` 0.39. They cannot share parsed DOMs directly.
4. **No streaming** -- Entire HTML must be in memory; entire Markdown output is in memory.
5. **Table limitations** -- Complex tables (colspan, rowspan) are flattened. Nested tables may produce unexpected output.
6. **TranslationMode::Faithful** -- New in 0.5.0. Preserves HTML for unsupported tags but increases output verbosity.

---

## 3. dom_smoothie (v0.16.0)

**Repository**: https://github.com/niklak/dom_smoothie
**License**: MIT
**Downloads**: ~36K total
**Based on**: Mozilla Readability.js (faithful Rust port)

### What It Does

Extracts the main readable content from web pages by removing navigation, ads, sidebars, and boilerplate. Equivalent to Firefox's Reader View. Returns clean HTML + text content + metadata.

### Complete API Surface

#### Core Types

| Type | Description |
|------|-------------|
| `Readability` | Main processor. Holds document, URL, and config |
| `Article` | Extraction result (title, content, text, metadata) |
| `Metadata` | Document metadata (title, author, dates, image, etc.) |
| `Config` | Configuration options |
| `ReadabilityError` | Error enum (BadDocumentURL, GrabFailed, TooManyElements) |
| `CandidateSelectMode` | Algorithm choice: Readability.js vs DomSmoothie |
| `ParsePolicy` | Cleaning aggressiveness: Strict, Moderate, Clean, Raw |
| `TextMode` | Output format: Raw, Formatted, Markdown |

#### `Readability` Constructors

```rust
// From HTML string, with optional URL and config
Readability::new(html, document_url: Option<&str>, cfg: Option<Config>)
    -> Result<Readability, ReadabilityError>

// From pre-parsed dom_query::Document
Readability::with_document(doc: Document, url: Option<&str>, cfg: Option<Config>)
    -> Result<Readability, ReadabilityError>

// Simple: From<T: Into<StrTendril>> (no URL, default config)
Readability::from(html)
```

#### `Readability` Methods

```rust
readability.parse() -> Result<Article, ReadabilityError>
    // Full extraction: tries all policies, keeps best result

readability.parse_with_policy(ParsePolicy) -> Result<Article, ReadabilityError>
    // Single-policy extraction (lower memory, may fail with GrabFailed)

readability.get_article_title() -> StrTendril
    // Extract just the title (fast, no content extraction)

readability.get_article_metadata(json_ld: Option<Metadata>) -> Metadata
    // Extract metadata from <meta> tags, optionally merging with JSON-LD

readability.parse_json_ld() -> Option<Metadata>
    // Extract metadata from <script type="application/ld+json">

readability.is_probably_readable() -> bool
    // Quick heuristic check: is there enough content to extract?
```

#### `Article` Fields

```rust
pub struct Article {
    pub title: String,              // Cleaned article title
    pub byline: Option<String>,     // Author
    pub content: StrTendril,        // Cleaned HTML content
    pub text_content: StrTendril,   // Plain text content
    pub length: usize,              // Text length
    pub excerpt: Option<String>,    // Article excerpt/description
    pub site_name: Option<String>,  // Site name
    pub dir: Option<String>,        // Text direction (ltr/rtl)
    pub lang: Option<String>,       // Document language
    pub published_time: Option<String>,
    pub modified_time: Option<String>,
    pub image: Option<String>,      // Main image URL
    pub favicon: Option<String>,    // Favicon URL
    pub url: Option<String>,        // Canonical URL
}
```

#### `Metadata` Fields

Same fields as `Article` metadata subset: `title`, `byline`, `excerpt`, `site_name`, `published_time`, `modified_time`, `image`, `favicon`, `lang`, `url`, `dir`.

#### `Config` Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `keep_classes` | `bool` | `false` | Preserve all CSS classes |
| `classes_to_preserve` | `Vec<String>` | `[]` | Specific classes to keep |
| `max_elements_to_parse` | `usize` | `0` (unlimited) | Element count limit (guard against huge DOMs) |
| `disable_json_ld` | `bool` | `false` | Skip JSON-LD metadata extraction |
| `n_top_candidates` | `usize` | `5` | Number of top scoring candidates |
| `char_threshold` | `usize` | `500` | Minimum characters for content |
| `min_score_to_adjust` | `f32` | `5.0` | Score threshold for node adjustment |
| `readable_min_score` | `f32` | `20.0` | Threshold for `is_probably_readable` |
| `readable_min_content_length` | `usize` | `140` | Min content length for readability check |
| `candidate_select_mode` | `CandidateSelectMode` | `Readability` | Algorithm: mozilla vs dom_smoothie |
| `text_mode` | `TextMode` | `Raw` | Text output: Raw, Formatted, or Markdown |

#### `ParsePolicy` Variants

| Policy | Behavior |
|--------|----------|
| `Strict` | Remove unlikely elements, use id/class for scoring, aggressive cleaning. Slowest but cleanest. |
| `Moderate` | Use id/class scoring + cleaning, but keep unlikely elements. |
| `Clean` | Only applies post-extraction cleaning. |
| `Raw` | No cleaning at all. Fastest. |

#### Free Function

```rust
is_probably_readable(doc: &Document, min_score: Option<f32>, min_content_length: Option<usize>) -> bool
```

### Comparison with Mozilla Readability.js

| Aspect | Readability.js | dom_smoothie |
|--------|---------------|--------------|
| Language | JavaScript | Rust |
| DOM Parser | Browser native | dom_query (html5ever-based) |
| Algorithm | Single scoring pass | Same + alternative `DomSmoothie` mode |
| Multiple policies | Retries with weaker flags | Same (parse tries all, parse_with_policy tries one) |
| JSON-LD | Yes | Yes (via gjson) |
| Metadata | title, byline, excerpt, siteName, dir, lang | Same + publishedTime, modifiedTime, image, favicon, url |
| Text output | Raw only | Raw, Formatted, or Markdown |
| `is_probably_readable` | Yes | Yes (same algorithm) |
| URL resolution | Yes | Yes (relative to absolute) |
| Performance | ~100ms typical | ~2-5x faster (Rust) |
| WASM support | N/A (JS native) | Yes (wasm-bindgen-test in dev-deps) |

**Key differences**:
- dom_smoothie extracts MORE metadata (favicon, published_time, modified_time, image, url from JSON-LD)
- `TextMode::Markdown` is unique to dom_smoothie (built-in via dom_query's `markdown` feature)
- `CandidateSelectMode::DomSmoothie` alternative algorithm captures more content in edge cases where Readability.js discards too aggressively
- `ParsePolicy` gives explicit control vs Readability.js's internal retry logic

### Dependencies (runtime)

- `dom_query` 0.26 -- HTML parsing + CSS selector querying (with `mini_selector` + `markdown` features)
- `tendril` 0.5 -- Efficient strings
- `gjson` 0.8 -- JSON parsing for LD+JSON metadata
- `html-escape` 0.2 -- HTML entity decoding
- `once_cell` 1.x -- Lazy statics
- `flagset` 0.4 -- Bitflag sets for parse policies
- `unicode-segmentation` 1.12 -- Unicode-aware text processing
- `thiserror` 2.0 -- Error derive
- `phf` 0.13 -- Compile-time perfect hash maps
- `foldhash` 0.2 -- Fast hash maps
- Optional: `serde` (for Config/Article serialization), `aho-corasick` (faster pattern matching)

### Minimal Usage Example

```rust
use dom_smoothie::{Readability, Config, TextMode};

let html = fetch_html("https://example.com/article");
let cfg = Config {
    text_mode: TextMode::Markdown,
    max_elements_to_parse: 10_000,
    ..Default::default()
};

let mut reader = Readability::new(&html, Some("https://example.com/article"), Some(cfg))?;

if reader.is_probably_readable() {
    let article = reader.parse()?;
    println!("Title: {}", article.title);
    println!("Author: {:?}", article.byline);
    println!("Content (md): {}", article.text_content);  // Markdown when TextMode::Markdown
    println!("Content (html): {}", article.content);      // Always clean HTML
}
```

### Performance Characteristics

- **Parsing**: Single pass HTML parsing via dom_query (html5ever). Fast.
- **Scoring**: Multiple passes over DOM for candidate scoring. `Strict` policy is slowest (removes unlikely elements, scores all candidates).
- **Memory**: `parse()` keeps best attempt in memory (stores cloned DOM states). `parse_with_policy()` uses significantly less memory (single attempt).
- **Element limit**: `max_elements_to_parse` prevents OOM on adversarial inputs. Returns `TooManyElements` error.
- **Thread safety**: `Readability` is NOT `Send + Sync` (dom_query Document uses Rc internally).

### Gotchas and Limitations

1. **Mutable parse** -- `readability.parse()` takes `&mut self` and modifies the internal document. Cannot parse twice.
2. **GrabFailed** -- `parse_with_policy()` can fail with `GrabFailed` if the chosen policy is too aggressive. `parse()` retries with weaker policies automatically.
3. **Text quality** -- `text_content` in `Raw` mode may squash words together when HTML elements lack whitespace. Use `TextMode::Formatted` or `TextMode::Markdown` for better results.
4. **Not a general-purpose parser** -- Designed specifically for article extraction. Does not work well on: forums, e-commerce product pages, dashboards, single-page apps.
5. **URL requirement** -- For relative URL resolution, must provide an absolute document URL. Returns `BadDocumentURL` if relative.
6. **gjson quirk** -- JSON-LD parsing replaces `"@"` with `"^"` internally due to gjson limitations. Could cause issues with unusual LD+JSON.
7. **Different DOM library** -- Uses `dom_query`, not `scraper`'s `ego-tree`. Cannot share parsed documents between `scraper` and `dom_smoothie`.

---

## Dependency Overlap Analysis

```
                scraper         htmd            dom_smoothie
                -------         ----            ------------
html5ever       0.39            0.36*           (via dom_query)
tendril         0.5             (via html5ever) 0.5
phf             --              0.13            0.13
selectors       0.36            --              --
ego-tree        0.11            --              --
dom_query       --              --              0.26
markup5ever_rcdom --            0.36            --
```

*Note: `htmd` and `scraper` use different `html5ever` major versions (0.36 vs 0.39). They will each pull their own version. This means approximately 2 copies of the html5ever parser in the binary.*

**Recommendation**: If binary size matters, consider that adding all three crates pulls in two different HTML parsers. For the `scraper` + `htmd` combo, HTML must be parsed twice (once by each crate). For `dom_smoothie` + `htmd`, the `dom_smoothie` Article's `content` field (clean HTML string) can be fed to `htmd::convert()` without re-parsing the original page.

---

## Integration Strategy for Nika

### Pipeline Architecture

```
fetch: verb
  |
  v
Raw HTML string
  |
  +---> [dom_smoothie] ---> Article { content (HTML), text_content, title, metadata }
  |                              |
  |                              +---> [htmd] ---> Markdown string (for LLM infer:)
  |
  +---> [scraper] ---> Structured data extraction (CSS selectors)
```

### Recommended Feature Flags

```toml
[dependencies]
scraper = { version = "0.26", default-features = false, features = ["errors"] }
htmd = "0.5"
dom_smoothie = { version = "0.16", features = ["serde"] }
```

- `scraper`: disable `main` feature (binary entrypoint not needed). Keep `errors` for diagnostics.
- `dom_smoothie`: enable `serde` so `Config` and `Article` can be serialized to/from YAML workflow bindings.

### Nika Verb Integration Ideas

```yaml
# Extract article content with readability
- task: extract_article
  fetch:
    url: "{{with.url}}"
  readability:
    text_mode: markdown
    max_elements: 10000

# CSS selector extraction
- task: scrape_prices
  fetch:
    url: "{{with.url}}"
  select:
    selector: ".product-price"
    extract: text  # or "attr:href", "html", "inner_html"

# HTML to Markdown conversion
- task: to_markdown
  fetch:
    url: "{{with.url}}"
  to_markdown:
    skip_tags: ["script", "style", "nav"]
    heading_style: atx
```

---

## Confidence Level

**High** -- All three crates were analyzed from primary sources (GitHub source code, crates.io API, docs.rs). Version numbers, API surfaces, and dependency trees are verified against actual source. Performance claims come from crate authors' benchmarks.

## Sources

1. [scraper source code](https://github.com/rust-scraper/scraper) -- Full API from src/lib.rs, html/mod.rs, element_ref/mod.rs, selector.rs, node.rs, selectable.rs
2. [htmd source code](https://github.com/letmutex/htmd) -- Full API from src/lib.rs, element_handler/mod.rs, options.rs
3. [dom_smoothie source code](https://github.com/niklak/dom_smoothie) -- Full API from src/lib.rs, readability.rs, config.rs, readable.rs
4. [crates.io API](https://crates.io) -- Download counts, version info, license, dependency lists
5. [docs.rs](https://docs.rs) -- API documentation pages for all three crates

## Methodology

- Tools used: curl (GitHub raw source), crates.io REST API, docs.rs HTML
- Files analyzed: ~25 source files across 3 repositories
- All data verified against latest published versions as of 2026-03-19
