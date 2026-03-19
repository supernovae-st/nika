# Research Report: Rust Crates for PDF and Document Processing

**Date**: 2026-03-18
**Scope**: Comprehensive survey of the Rust ecosystem for reading, writing, rendering,
and converting PDF, DOCX, XLSX, SVG, and Markdown documents.

---

## Summary

The Rust document processing ecosystem is mature and competitive. For PDF reading,
**pdf_oxide** (pure Rust, 0.8ms mean, 100% pass rate) and **lopdf** (5M downloads, mature)
lead the field. For PDF writing, the **typst** ecosystem (**pdf-writer** + **krilla**)
represents the state of the art. For rendering PDF to images, **pdfium-render** (FFI to
Chromium's Pdfium) is unmatched. SVG rendering is dominated by the pure-Rust **resvg/usvg**
stack (10M+ downloads). Spreadsheets are handled by **calamine** (read) and
**rust_xlsxwriter** (write). DOCX writing is handled by **docx-rs**.

---

## 1. PDF Reading / Parsing

### 1.1 lopdf

| Property | Value |
|----------|-------|
| Crate | `lopdf` |
| Version | 0.39.0 |
| Downloads | 5,019,552 |
| Stars | 2,095 |
| License | MIT |
| Pure Rust | Yes |
| Repo | https://github.com/J-F-Liu/lopdf |

**Capabilities**: Read, write, and manipulate PDF document structure. Operates at the
PDF object level (dictionaries, streams, content operations). Does NOT extract text
natively -- it gives you raw access to the PDF object tree.

**Strengths**:
- Most downloaded PDF-specific crate in Rust
- Full read/write/modify cycle
- Pure Rust, no FFI dependencies
- Supports object streams, cross-reference streams, incremental updates
- Foundation crate: used by `printpdf`, `genpdf`, `pdf-extract`, and many others
- Async support via `tokio` feature flag
- Serde serialization support

**Weaknesses**:
- Low-level: no text extraction, no rendering
- Only 80.2% pass rate on diverse PDF corpus (per pdf_oxide benchmarks)
- API is verbose for simple tasks

**Code Example** (reading):
```rust
use lopdf::Document;

let doc = Document::load("example.pdf")?;
for (page_num, page_id) in doc.get_pages() {
    let content = doc.get_page_content(page_id)?;
    let fonts = doc.get_page_fonts(page_id);
    println!("Page {}: {} bytes of content", page_num, content.len());
}
```

---

### 1.2 pdf (pdf-rs)

| Property | Value |
|----------|-------|
| Crate | `pdf` |
| Version | 0.10.0 |
| Downloads | 487,834 |
| Stars | 1,627 |
| License | MIT |
| Pure Rust | Yes |
| Repo | https://github.com/pdf-rs/pdf |

**Capabilities**: Read, alter, and write PDF files. Higher-level abstraction than lopdf.
Provides typed access to PDF objects (pages, fonts, images, annotations).

**Strengths**:
- Strongly typed PDF object model
- Memory-mapped file reading (`mmap` feature)
- Sync/Send support for concurrent access
- Cache feature for parsed objects
- Active community (Zulip chat)
- Companion renderer via Pathfinder (`pdf_render` crate)

**Weaknesses**:
- Writing is still experimental
- Smaller ecosystem than lopdf
- Fewer downstream dependents

**Code Example**:
```rust
use pdf::file::FileOptions;

let file = FileOptions::cached().open("example.pdf")?;
for page in file.pages() {
    let page = page?;
    if let Some(ref content) = page.contents {
        // Access page content operations
    }
}
```

---

### 1.3 pdf_oxide

| Property | Value |
|----------|-------|
| Crate | `pdf_oxide` |
| Version | 0.3.17 |
| Downloads | 7,006 |
| Stars | 433 |
| License | MIT OR Apache-2.0 |
| Pure Rust | Yes (core), optional FFI features |
| Repo | https://github.com/yfedoseev/pdf_oxide |

**Capabilities**: Text extraction, image extraction, markdown conversion, PDF creation,
PDF merging, search, and editing. Multi-platform (Rust, Python, WASM, CLI, MCP server).

**Strengths**:
- **Fastest**: 0.8ms mean text extraction (5x faster than PyMuPDF)
- **Most reliable**: 100% pass rate on 3,830 real-world PDFs
- Built-in text extraction, image extraction, markdown conversion
- 99.5% text parity vs PyMuPDF and pypdfium2
- MCP server for AI assistants (directly relevant to Nika)
- Comprehensive feature set with optional deps (barcode, QR, OCR via ONNX)
- CLI tool with 22 commands

**Weaknesses**:
- Very new (low download count)
- Large dependency tree when all features enabled
- Some optional features pull in heavy deps (ort, linfa, pdfium-render)

**Code Example**:
```rust
use pdf_oxide::PdfDocument;

let mut doc = PdfDocument::open("paper.pdf")?;
let text = doc.extract_text(0)?;           // Page 0 text
let images = doc.extract_images(0)?;       // Page 0 images
let markdown = doc.to_markdown(0, Default::default())?;
```

**Verdict**: The rising star. If benchmarks hold, this is the best pure-Rust PDF reader
available today. The MCP server integration is a perfect fit for Nika workflows.

---

### 1.4 pdf-extract

| Property | Value |
|----------|-------|
| Crate | `pdf-extract` |
| Version | 0.10.0 |
| Downloads | 884,977 |
| Stars | 575 |
| License | (check repo) |
| Pure Rust | Yes |
| Repo | https://github.com/jrmuizel/pdf-extract |

**Capabilities**: Text extraction from PDF files. Built on top of `lopdf`.

**Strengths**:
- Simple, focused API: give it a PDF, get text back
- Uses lopdf for parsing, adds font decoding and text positioning
- Supports CFF and Type1 fonts

**Weaknesses**:
- 91.5% pass rate (per pdf_oxide benchmarks)
- No image extraction
- No writing capabilities
- Depends on older lopdf (0.38)

**Code Example**:
```rust
let bytes = std::fs::read("document.pdf")?;
let text = pdf_extract::extract_text_from_mem(&bytes)?;
println!("{}", text);
```

---

## 2. PDF Writing / Generation

### 2.1 pdf-writer

| Property | Value |
|----------|-------|
| Crate | `pdf-writer` |
| Version | 0.14.0 |
| Downloads | 1,124,351 |
| Stars | 680 |
| License | MIT OR Apache-2.0 |
| Pure Rust | Yes |
| Repo | https://github.com/typst/pdf-writer |

**Capabilities**: Low-level, step-by-step PDF creation. Builder pattern for PDF objects.
No parsing -- write-only.

**Strengths**:
- From the Typst team (powers Typst's PDF export)
- Strongly typed builders for every PDF object type
- Minimal allocations (borrows parent buffers)
- Zero `unsafe` code
- Foundation for `krilla` and `typst-pdf`
- Excellent documentation

**Weaknesses**:
- Very low-level: you must understand PDF spec
- No text layout, no font subsetting, no image handling (by design)
- Write-only, cannot read PDFs

**Code Example**:
```rust
use pdf_writer::{Pdf, Rect, Ref};

let catalog_id = Ref::new(1);
let page_tree_id = Ref::new(2);
let page_id = Ref::new(3);

let mut pdf = Pdf::new();
pdf.catalog(catalog_id).pages(page_tree_id);
pdf.pages(page_tree_id).kids([page_id]).count(1);
pdf.page(page_id)
    .parent(page_tree_id)
    .media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
    .resources();

std::fs::write("empty.pdf", pdf.finish())?;
```

---

### 2.2 krilla

| Property | Value |
|----------|-------|
| Crate | `krilla` |
| Version | 0.6.0 |
| Downloads | 223,866 |
| Stars | 359 |
| License | MIT OR Apache-2.0 |
| Pure Rust | Yes |
| Repo | https://github.com/LaurenzV/krilla |

**Capabilities**: High-level PDF creation with 2D graphics primitives.
Fills, strokes, gradients, glyphs, images, transforms, masks, clip paths, blend modes.

**Strengths**:
- Built on `pdf-writer` (Typst ecosystem)
- **Best-in-class testing**: 90+ snapshot tests, 210+ visual regression tests across
  6 PDF viewers (Ghostscript, MuPDF, Poppler, PDFBox, Pdfium, Quartz)
- PDF/A-1, PDF/A-2, PDF/A-3, PDF/A-4, PDF/UA-1 validated export
- Tagged PDF for accessibility
- Excellent OpenType font support (CFF + TTF, color fonts)
- Font subsetting via `subsetter`
- SVG embedding via `krilla-svg`
- Optional `rayon` parallelism
- PDF versions 1.4 through 2.0

**Weaknesses**:
- No text layout (by design -- expects pre-layouted content)
- No page breaking, headers/footers
- No encryption or digital signatures (yet)
- Relatively new

**Target use case**: Libraries that have an intermediate representation of layouted content
and want to emit PDF. This is EXACTLY what Nika's media pipeline needs.

**Code Example**:
```rust
use krilla::Document;
// krilla provides high-level primitives:
// - Surface for drawing (fill, stroke, text, images)
// - Document for multi-page PDF assembly
// - Automatic font subsetting and embedding
```

---

### 2.3 printpdf

| Property | Value |
|----------|-------|
| Crate | `printpdf` |
| Version | 0.9.1 |
| Downloads | 895,507 |
| Stars | 1,053 |
| License | MIT |
| Pure Rust | Yes |
| Repo | https://github.com/fschutt/printpdf |

**Capabilities**: Read and write PDF files. Focus on print-friendly PDFs.
Built on `lopdf`.

**Strengths**:
- Both read and write
- SVG embedding via `svg2pdf` (optional feature)
- Image embedding (via `image` crate)
- Font embedding via `allsorts`
- HTML-to-PDF capabilities (optional `kuchiki` feature)
- Good for generating reports, invoices, labels

**Weaknesses**:
- API can be verbose
- Less modern than krilla
- Font handling has rough edges
- Based on lopdf (inherits its limitations)

**Code Example**:
```rust
use printpdf::*;

let (doc, page1, layer1) = PdfDocument::new("My PDF", Mm(210.0), Mm(297.0), "Layer 1");
let current_layer = doc.get_page(page1).get_layer(layer1);

let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
current_layer.use_text("Hello World!", 24.0, Mm(10.0), Mm(250.0), &font);
doc.save(&mut BufWriter::new(File::create("output.pdf")?))?;
```

---

### 2.4 genpdf

| Property | Value |
|----------|-------|
| Crate | `genpdf` |
| Version | 0.2.0 |
| Downloads | 307,299 |
| Stars | (sr.ht -- no stars) |
| License | Apache-2.0 |
| Pure Rust | Yes |
| Repo | https://git.sr.ht/~ireas/genpdf-rs |

**Capabilities**: User-friendly PDF generation with automatic text layout,
page breaking, and basic document structure.

**Strengths**:
- Highest-level API for document generation
- Automatic page breaking
- Basic elements: paragraphs, tables, images, ordered/unordered lists
- Built on `printpdf` and `lopdf`
- Hyphenation support (optional)

**Weaknesses**:
- Limited styling options
- No headers/footers
- Based on older printpdf (0.3.4) and lopdf (0.26)
- Not actively updated (last release: 2021)

---

## 3. PDF Rendering (PDF to Image)

### 3.1 pdfium-render

| Property | Value |
|----------|-------|
| Crate | `pdfium-render` |
| Version | 0.8.37 (latest 0.9.0) |
| Downloads | 803,304 |
| Stars | 622 |
| License | MIT OR Apache-2.0 |
| Pure Rust | **No** -- FFI to Pdfium (C++) |
| Repo | https://github.com/ajrcarey/pdfium-render |

**Capabilities**: Full PDF rendering, text/image extraction, form filling, annotation
reading, document creation, page manipulation. The most feature-complete PDF crate.

**Strengths**:
- Powered by Pdfium (Google Chromium's PDF engine) -- battle-tested on billions of PDFs
- Render PDF pages to bitmaps (via `image` crate integration)
- Text extraction with positioning
- Image extraction
- Form field reading and filling
- Digital signature introspection
- Page object manipulation (text, paths, bitmaps)
- Multi-page tiled rendering
- Watermarking
- Document concatenation
- WASM support
- Thread safety (Send + Sync in 0.9.0)

**Weaknesses**:
- **Requires external Pdfium binary** (not bundled, must be provided at runtime)
- FFI-based (not pure Rust)
- Pdfium itself is ~25MB binary
- AGPL concerns if using Pdfium from certain sources

**Code Example**:
```rust
use pdfium_render::prelude::*;

let pdfium = Pdfium::default();
let document = pdfium.load_pdf_from_file("input.pdf", None)?;

let render_config = PdfRenderConfig::new()
    .set_target_width(2000)
    .set_maximum_height(2000)
    .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true);

for (index, page) in document.pages().iter().enumerate() {
    page.render_with_config(&render_config)?
        .as_image()?
        .into_rgb8()
        .save_with_format(
            format!("page-{}.jpg", index),
            image::ImageFormat::Jpeg
        )
        .map_err(|_| PdfiumError::ImageError)?;
}
```

---

### 3.2 mupdf

| Property | Value |
|----------|-------|
| Crate | `mupdf` |
| Version | 0.6.0 |
| Downloads | 689,485 |
| Stars | 179 |
| License | **AGPL-3.0** |
| Pure Rust | **No** -- FFI to MuPDF (C) |
| Repo | https://github.com/messense/mupdf-rs |

**Capabilities**: PDF rendering, text extraction, annotations. Bindings to the
Artifex MuPDF library.

**Strengths**:
- MuPDF is extremely mature and handles complex PDFs well
- Supports PDF, XPS, EPUB, CBZ, and image formats
- Fast rendering

**Weaknesses**:
- **AGPL-3.0 license** -- viral copyleft, problematic for commercial use
- FFI-based, requires MuPDF C library compilation
- Work in progress
- Build complexity (C/C++ toolchain required)

---

### 3.3 poppler-rs

| Property | Value |
|----------|-------|
| Crate | `poppler-rs` |
| Version | 0.26.0 |
| Downloads | 181,717 |
| License | GPL-2.0 |
| Pure Rust | **No** -- FFI to Poppler/GLib |
| Repo | https://gitlab.gnome.org/World/Rust/poppler-rs |

**Capabilities**: PDF rendering and text extraction via Poppler (the Linux PDF library).

**Weaknesses**:
- GPL-2.0 license
- Requires system Poppler installation
- GLib dependency
- Platform-limited (primarily Linux)

---

## 4. Document Typesetting

### 4.1 typst

| Property | Value |
|----------|-------|
| Crate | `typst` |
| Version | 0.14.2 |
| Downloads | 777,591 |
| Stars | **52,118** |
| License | Apache-2.0 |
| Pure Rust | Yes |
| Repo | https://github.com/typst/typst |

**Capabilities**: Complete markup-based typesetting system. Input: Typst markup.
Output: PDF (via `typst-pdf`) or raster images (via `typst-render`).

**Strengths**:
- The most popular Rust project in the document space (52K stars)
- Full document typesetting: text layout, math, bibliography, tables, headers/footers
- Incremental compilation for fast re-renders
- Produces high-quality PDF output
- Rich scripting language built in
- Package ecosystem

**Sub-crates**:
- `typst-pdf` (v0.14.2, 698K downloads) -- PDF export
- `typst-render` (v0.14.2, 122K downloads) -- Raster image export
- `pdf-writer` (v0.14.0, 1.1M downloads) -- Low-level PDF writing
- `svg2pdf` (v0.13.0, 778K downloads) -- SVG to PDF conversion
- `subsetter` (v0.2.3, 1.1M downloads) -- Font subsetting

**Weaknesses**:
- Heavy dependency (full typesetting engine)
- Input must be Typst markup (not HTML, not Markdown directly)
- Overkill if you just need simple PDF generation

**Use case for Nika**: Could be used as a "document rendering backend" where Nika
workflows generate Typst markup, then compile to PDF. Very powerful but adds
significant binary size.

---

## 5. SVG Processing

### 5.1 resvg

| Property | Value |
|----------|-------|
| Crate | `resvg` |
| Version | 0.47.0 |
| Downloads | 10,912,853 |
| Stars | 3,714 |
| License | MPL-2.0 |
| Pure Rust | **Yes** (100% Rust, including all dependencies) |
| Repo | https://github.com/linebender/resvg |

**Capabilities**: SVG rendering to raster images. The gold standard for SVG in Rust.

**Strengths**:
- ~1,600 regression tests
- 100% pure Rust (no system library dependencies)
- Reproducible output across all platforms (pixel-identical)
- Handles SVG edge cases better than many browsers
- < 3MB binary size, zero external dependencies
- WASM-compatible
- Fast (uses `tiny-skia` for rendering)
- Excellent security (minimal `unsafe`, recursion/loop protection)

**Weaknesses**:
- No animations (static SVG only)
- No native text rendering (uses own text engine)
- MPL-2.0 license (file-level copyleft, compatible with MIT/Apache projects)

**Code Example**:
```rust
let tree = usvg::Tree::from_str(&svg_string, &usvg::Options::default())?;
let size = tree.size().to_int_size();
let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
pixmap.save_png("output.png")?;
```

---

### 5.2 usvg

| Property | Value |
|----------|-------|
| Crate | `usvg` |
| Version | 0.47.0 |
| Downloads | 12,075,956 |
| Stars | (same repo as resvg) |
| License | MPL-2.0 |
| Pure Rust | Yes |

**Capabilities**: SVG simplification/preprocessing. Converts complex SVG into a
simplified tree that is easy to render. Used by `resvg` internally but can be
used standalone to build custom SVG renderers.

---

### 5.3 svg2pdf

| Property | Value |
|----------|-------|
| Crate | `svg2pdf` |
| Version | 0.13.0 |
| Downloads | 778,971 |
| Stars | 386 |
| License | MIT OR Apache-2.0 |
| Pure Rust | Yes |
| Repo | https://github.com/typst/svg2pdf |

**Capabilities**: Converts SVG files to PDF. Uses `usvg` for parsing and `pdf-writer`
for output.

**Strengths**:
- Part of the Typst ecosystem
- Clean pipeline: SVG -> usvg tree -> PDF
- Used by `printpdf` (optional feature)

---

### 5.4 tiny-skia

| Property | Value |
|----------|-------|
| Crate | `tiny-skia` |
| Version | 0.12.0 |
| Downloads | 22,722,017 |
| Stars | 1,515 |
| License | BSD-3-Clause |
| Pure Rust | Yes |

**Capabilities**: 2D rendering library (subset of Skia ported to pure Rust).
Not SVG-specific but provides the rasterization backend for `resvg`.

---

## 6. Spreadsheet Processing

### 6.1 calamine (Reading)

| Property | Value |
|----------|-------|
| Crate | `calamine` |
| Version | 0.34.0 |
| Downloads | 6,171,012 |
| Stars | 2,229 |
| License | MIT |
| Pure Rust | Yes |
| Repo | https://github.com/tafia/calamine |

**Capabilities**: Read and deserialize spreadsheet files.

**Supported formats**: `xls`, `xlsx`, `xlsm`, `xlsb`, `xla`, `xlam`, `ods`

**Strengths**:
- The definitive Rust spreadsheet reader
- Serde deserialization support
- Pure Rust
- 6M+ downloads
- Active maintenance

**Code Example**:
```rust
use calamine::{open_workbook, Reader, Xlsx};

let mut workbook: Xlsx<_> = open_workbook("file.xlsx")?;
let range = workbook.worksheet_range("Sheet1")?;
for row in range.rows() {
    println!("{:?}", row);
}
```

---

### 6.2 rust_xlsxwriter (Writing)

| Property | Value |
|----------|-------|
| Crate | `rust_xlsxwriter` |
| Version | 0.94.0 |
| Downloads | 1,564,300 |
| Stars | 546 |
| License | MIT OR Apache-2.0 |
| Pure Rust | Yes |
| Repo | https://github.com/jmcnamara/rust_xlsxwriter |

**Capabilities**: Write Excel 2007+ `.xlsx` files.

**Strengths**:
- By the author of Python's `XlsxWriter` (very mature design)
- Comprehensive feature set: formatting, formulas, charts, images, sparklines,
  data validation, conditional formatting, tables, serde support
- Excellent documentation and examples
- Pure Rust
- Active development (v0.94!)

**Weaknesses**:
- Write-only (cannot read .xlsx)
- Cannot modify existing files

**Code Example**:
```rust
use rust_xlsxwriter::*;

let mut workbook = Workbook::new();
let worksheet = workbook.add_worksheet();

worksheet.write(0, 0, "Hello")?;
worksheet.write(1, 0, 42.0)?;
worksheet.write(2, 0, Formula::new("=SIN(PI()/4)"))?;

workbook.save("output.xlsx")?;
```

---

### 6.3 umya-spreadsheet (Read + Write)

| Property | Value |
|----------|-------|
| Crate | `umya-spreadsheet` |
| Version | 2.3.3 |
| Downloads | 490,690 |
| Stars | 437 |
| License | MIT |
| Pure Rust | Yes |
| Repo | https://github.com/MathNya/umya-spreadsheet |

**Capabilities**: Read AND write `.xlsx` files. The only pure-Rust crate that supports
both directions for Excel files.

**Strengths**:
- Read + write + modify cycle
- Preserves formatting when modifying existing files
- Pure Rust

**Weaknesses**:
- Less feature-complete than rust_xlsxwriter for writing
- Less battle-tested than calamine for reading

---

### 6.4 xlsxwriter

| Property | Value |
|----------|-------|
| Crate | `xlsxwriter` |
| Version | 0.6.1 |
| Downloads | 1,033,738 |
| License | MIT |
| Pure Rust | **No** -- FFI to libxlsxwriter (C) |

**Capabilities**: Write `.xlsx` files via C bindings.

**Note**: Superseded by `rust_xlsxwriter` (pure Rust, same author lineage).

---

## 7. DOCX Processing

### 7.1 docx-rs

| Property | Value |
|----------|-------|
| Crate | `docx-rs` |
| Version | 0.4.19 |
| Downloads | 1,411,508 |
| Stars | 513 |
| License | MIT |
| Pure Rust | Yes |
| Repo | https://github.com/bokuweb/docx-rs |

**Capabilities**: Write `.docx` files. WASM-compatible (available as `docx-wasm` on npm).

**Strengths**:
- Write-only but comprehensive: paragraphs, runs, tables, images, styles
- WASM support (browser and Node.js)
- Active maintenance
- Pure Rust

**Weaknesses**:
- Write-only (cannot read/parse existing .docx)
- Limited formatting compared to full Office spec

**Code Example**:
```rust
use docx_rs::*;

let path = std::path::Path::new("hello.docx");
let file = std::fs::File::create(path)?;
Docx::new()
    .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Hello")))
    .build()
    .pack(file)?;
```

---

## 8. Markdown Processing

### 8.1 pulldown-cmark

| Property | Value |
|----------|-------|
| Crate | `pulldown-cmark` |
| Version | 0.13.1 |
| Downloads | 75,863,447 |
| Stars | 2,504 |
| License | MIT |
| Pure Rust | Yes |
| Repo | https://github.com/pulldown-cmark/pulldown-cmark |

**Capabilities**: CommonMark-compliant pull parser for Markdown. AST-level access.

**Strengths**:
- 75M+ downloads -- THE Markdown parser in Rust
- Pull-based (streaming, low memory)
- CommonMark compliant
- Used by `rustdoc`, `mdBook`, and thousands of crates

---

### 8.2 comrak

| Property | Value |
|----------|-------|
| Crate | `comrak` |
| Version | 0.51.0 |
| Downloads | 3,732,862 |
| Stars | 1,563 |
| License | BSD-2-Clause |
| Pure Rust | Yes |
| Repo | https://github.com/kivikakk/comrak |

**Capabilities**: CommonMark + GitHub Flavored Markdown parser and renderer.
652/652 CommonMark compliance, 670/670 GFM compliance.

**Strengths**:
- 100% CommonMark and GFM compliant
- Both parsing and rendering (to HTML, CommonMark, etc.)
- AST manipulation
- CLI tool included
- Extensions: tables, task lists, strikethrough, autolinks, footnotes

---

### 8.3 markdown

| Property | Value |
|----------|-------|
| Crate | `markdown` |
| Version | 1.0.0 |
| Downloads | 5,527,878 |
| License | MIT |
| Pure Rust | Yes |
| Repo | https://github.com/wooorm/markdown-rs |

**Capabilities**: CommonMark parser with AST and extensions. From the author of
`unified`/`remark` (the JavaScript Markdown ecosystem).

---

## 9. Markdown to PDF

There is **no single dominant crate** for Markdown-to-PDF in pure Rust. Current approaches:

| Approach | Crates | Quality |
|----------|--------|---------|
| Markdown -> HTML -> Headless Chrome -> PDF | `comrak` + `html2pdf` | Good rendering, heavy |
| Markdown -> Typst markup -> PDF | `comrak` + `typst` | Excellent, heavy |
| Markdown -> lopdf primitives | `genpdf` (accepts text, not MD) | Basic |
| Markdown -> External tool | `pandoc` crate (wraps pandoc binary) | Excellent, requires pandoc |

**Recommendation for Nika**: Parse Markdown with `comrak` or `pulldown-cmark`, convert the
AST to `krilla` drawing calls, output PDF. This would be a lightweight, pure-Rust pipeline.

---

## 10. Comparative Matrix

### PDF Crates at a Glance

| Crate | Read | Write | Render | Text Extract | Pure Rust | License | Downloads |
|-------|------|-------|--------|--------------|-----------|---------|-----------|
| **lopdf** | Yes | Yes | No | No | Yes | MIT | 5.0M |
| **pdf** | Yes | Exp. | No | No | Yes | MIT | 488K |
| **pdf_oxide** | Yes | Yes | No | Yes | Yes | MIT/Apache | 7K |
| **pdf-extract** | Yes | No | No | Yes | Yes | - | 885K |
| **pdf-writer** | No | Yes | No | No | Yes | MIT/Apache | 1.1M |
| **krilla** | No | Yes | No | No | Yes | MIT/Apache | 224K |
| **printpdf** | Yes | Yes | No | No | Yes | MIT | 896K |
| **genpdf** | No | Yes | No | No | Yes | Apache | 307K |
| **pdfium-render** | Yes | Yes | **Yes** | Yes | **No** | MIT/Apache | 803K |
| **mupdf** | Yes | No | **Yes** | Yes | **No** | **AGPL** | 689K |
| **typst** | No | Yes | **Yes** | No | Yes | Apache | 778K |

### Non-PDF Document Crates

| Crate | Format | Direction | Pure Rust | Downloads |
|-------|--------|-----------|-----------|-----------|
| **calamine** | xlsx/xls/ods | Read | Yes | 6.2M |
| **rust_xlsxwriter** | xlsx | Write | Yes | 1.6M |
| **umya-spreadsheet** | xlsx | Read+Write | Yes | 491K |
| **docx-rs** | docx | Write | Yes | 1.4M |
| **resvg** | SVG | Render | Yes | 10.9M |
| **usvg** | SVG | Parse | Yes | 12.1M |
| **svg2pdf** | SVG->PDF | Convert | Yes | 779K |
| **pulldown-cmark** | Markdown | Parse | Yes | 75.9M |
| **comrak** | Markdown | Parse+Render | Yes | 3.7M |

---

## 11. Recommendations for Nika Media Pipeline

### Tier 1: Likely candidates (pure Rust, permissive license)

| Need | Recommended Crate | Rationale |
|------|-------------------|-----------|
| **PDF text extraction** | `pdf_oxide` | Fastest, highest reliability, MIT, has MCP server |
| **PDF generation** | `krilla` + `pdf-writer` | High-level API, Typst ecosystem, best testing |
| **SVG rendering** | `resvg` + `usvg` | Gold standard, pure Rust, pixel-reproducible |
| **SVG to PDF** | `svg2pdf` | Typst ecosystem, uses usvg + pdf-writer |
| **Spreadsheet reading** | `calamine` | Definitive, 6M downloads, serde support |
| **Spreadsheet writing** | `rust_xlsxwriter` | Most complete xlsx writer |
| **DOCX writing** | `docx-rs` | Only viable pure-Rust option |
| **Markdown parsing** | `comrak` | 100% GFM compliant, AST manipulation |

### Tier 2: Consider for specific features

| Need | Crate | When to use |
|------|-------|-------------|
| **PDF rendering to images** | `pdfium-render` | When you need PDF-to-image (requires external binary) |
| **PDF read/modify/write** | `lopdf` | When you need to manipulate PDF structure |
| **Full document typesetting** | `typst` | When you need LaTeX-quality output from markup |
| **Spreadsheet read+write** | `umya-spreadsheet` | When you need to modify existing xlsx |

### Binary Size Considerations

For Nika (which ships as a single binary), pure-Rust crates are strongly preferred:
- `krilla` + `pdf-writer` + `resvg` + `calamine` + `comrak` is a manageable stack
- `pdfium-render` would require shipping a ~25MB Pdfium binary alongside `nika`
- `mupdf` is AGPL -- incompatible with Nika's license model
- `typst` is powerful but adds significant binary size

---

## Sources

1. [crates.io](https://crates.io) -- Download counts, versions, feature flags (queried 2026-03-18)
2. [GitHub API](https://api.github.com) -- Star counts (queried 2026-03-18)
3. [lopdf README](https://github.com/J-F-Liu/lopdf) -- API examples
4. [pdf-writer README](https://github.com/typst/pdf-writer) -- API examples
5. [krilla README](https://github.com/LaurenzV/krilla) -- Feature list, testing methodology
6. [pdf_oxide README](https://github.com/yfedoseev/pdf_oxide) -- Benchmarks, API examples
7. [pdfium-render README](https://github.com/ajrcarey/pdfium-render) -- Feature list, API examples
8. [resvg README](https://github.com/linebender/resvg) -- Architecture, testing
9. [calamine README](https://github.com/tafia/calamine) -- API examples
10. [rust_xlsxwriter README](https://github.com/jmcnamara/rust_xlsxwriter) -- Feature list
11. [docx-rs README](https://github.com/bokuweb/docx-rs) -- API examples
12. [comrak README](https://github.com/kivikakk/comrak) -- Compliance stats
13. [typst README](https://github.com/typst/typst) -- Architecture overview

## Methodology

- Tools used: crates.io API, GitHub API, raw README fetches
- Crates analyzed: 40+
- Data points per crate: version, downloads, stars, license, dependencies, features, code examples
- Filtered for: active maintenance, >100 downloads, working code

## Confidence Level

**High** -- Data sourced directly from crates.io and GitHub APIs on 2026-03-18.
Download counts and star counts are factual. Performance claims for pdf_oxide are
self-reported by the author but include detailed methodology and reproducible benchmarks.
The krilla testing claims are verifiable in the repo's CI configuration.

## Further Research Suggestions

- Benchmark `pdf_oxide` vs `lopdf` + `pdf-extract` on Nika's specific PDF corpus
- Evaluate `krilla` for generating Nika workflow report PDFs
- Test `resvg` integration for SVG artifact rendering in the media pipeline
- Investigate `typst` as a "document backend" for Nika's `infer:` verb outputs
- Profile binary size impact of each crate combination
