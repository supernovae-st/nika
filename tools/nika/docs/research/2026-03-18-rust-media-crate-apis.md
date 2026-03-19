# Rust Media Crate API Reference

Research date: 2026-03-18
Sources: docs.rs, GitHub source, crates.io

---

## 1. color-thief (v0.2.2)

**Purpose**: Extract dominant color palette from images using the Modified Median Cut Quantization algorithm.
**License**: MIT | **Repo**: https://github.com/RazrFalcon/color-thief-rs
**Dependencies**: `rgb ^0.8`

### Public API Surface

```rust
// Re-export
pub use rgb::RGB8 as Color;  // {r: u8, g: u8, b: u8}

/// Color format of underlying image data
pub enum ColorFormat {
    Rgb,
    Rgba,
    Argb,
    Bgr,
    Bgra,
}

/// Errors
pub enum Error {
    InvalidVBox,
    VBoxCutFailed,
}

/// Extract a representative color palette from raw pixel data.
///
/// - pixels:       Raw image bytes (&[u8])
/// - color_format: Pixel layout (Rgb, Rgba, etc.)
/// - quality:      Step in pixels (1..=10). 1 = every pixel, 10 = every 10th
/// - max_colors:   Number of palette colors (2..=255)
pub fn get_palette(
    pixels: &[u8],
    color_format: ColorFormat,
    quality: u8,
    max_colors: u8,
) -> Result<Vec<Color>, Error>;
```

That is the **entire** public API -- one function, one enum, one re-exported type.

### Complete Example

```rust
use color_thief::{get_palette, ColorFormat, Color};
use image::GenericImageView;

fn extract_palette(path: &str) -> Result<Vec<Color>, Box<dyn std::error::Error>> {
    let img = image::open(path)?;
    let (width, height) = img.dimensions();
    let rgba_pixels = img.to_rgba8().into_raw();

    // quality=5 (medium speed), max_colors=8
    let palette = get_palette(&rgba_pixels, ColorFormat::Rgba, 5, 8)?;

    for (i, color) in palette.iter().enumerate() {
        println!("Color {i}: #{:02x}{:02x}{:02x}", color.r, color.g, color.b);
    }

    // Dominant color is always first
    let dominant = &palette[0];
    println!("Dominant: rgb({}, {}, {})", dominant.r, dominant.g, dominant.b);

    Ok(palette)
}
```

### Key Notes

- `Color` is `rgb::RGB8` -- a POD struct with `r`, `g`, `b` fields (all `u8`).
- The first color in the returned `Vec` is the most dominant.
- `quality=1` examines every pixel (slowest, most accurate). `quality=10` samples every 10th pixel.
- `max_colors` is a *maximum* -- the actual palette may be smaller for images with few distinct colors.
- No `image` crate dependency in the library itself; feed it raw `&[u8]` in any supported format.

---

## 2. calamine (v0.34.0)

**Purpose**: Read Excel (.xlsx, .xlsm, .xlsb, .xls) and OpenDocument (.ods) spreadsheets. Pure Rust.
**License**: MIT | **Repo**: https://github.com/tafia/calamine
**Key deps**: `quick-xml`, `zip`, `serde`, `encoding_rs`

### Public Types

```rust
// Format-specific readers
pub struct Xlsx<RS: Read + Seek>;   // .xlsx / .xlsm
pub struct Xlsb<RS: Read + Seek>;   // .xlsb
pub struct Xls<RS: Read + Seek>;    // .xls (legacy)
pub struct Ods<RS: Read + Seek>;    // .ods (OpenDocument)
pub struct Sheets;                   // Auto-detected format

// Cell data types
pub enum Data {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    DateTime(ExcelDateTime),
    DateTimeIso(String),
    DurationIso(String),
    Error(CellErrorType),
    Empty,
}

// Also: DataRef<'a> (borrowed variant), DataType (trait)

pub struct ExcelDateTime { /* opaque */ }
pub enum ExcelDateTimeType { DateTime, TimeDelta }

pub enum CellErrorType { Div0, NA, Name, Null, Num, Ref, Value, GettingData }

// Range = a 2D grid of cells
pub struct Range<T> { /* ... */ }
pub struct Dimensions { pub start: (u32, u32), pub end: (u32, u32) }

// Metadata
pub struct Metadata { /* sheets, names */ }
pub struct Sheet { pub name: String, pub typ: SheetType, pub visible: SheetVisible }
pub enum SheetType { WorkSheet, DialogSheet, MacroSheet, ChartSheet, Vba }
pub enum SheetVisible { Visible, Hidden, VeryHidden }

// Deserialization
pub struct RangeDeserializerBuilder<'h, H>;
pub struct RangeDeserializer<'h, T, H>;
pub enum DeError { CellOutOfRange{..}, CellError{..}, UnexpectedEndOfRow{..}, HeaderNotFound(String), Custom(String) }
pub enum HeaderRow { FirstNonEmptyRow, Row(u32) }
```

### Reader Trait

```rust
pub trait Reader<RS: Read + Seek>: Sized {
    type Error: Debug + From<io::Error>;

    fn new(reader: RS) -> Result<Self, Self::Error>;
    fn with_header_row(&mut self, header_row: HeaderRow) -> &mut Self;
    fn vba_project(&mut self) -> Result<Option<VbaProject>, Self::Error>;
    fn metadata(&self) -> &Metadata;
    fn worksheet_range(&mut self, name: &str) -> Result<Range<Data>, Self::Error>;
    fn worksheets(&mut self) -> Vec<(String, Range<Data>)>;
    fn worksheet_formula(&mut self, name: &str) -> Result<Range<String>, Self::Error>;
    fn sheet_names(&self) -> &[String];
    fn defined_names(&self) -> &[(String, String)];
}
```

### Range<T> Methods

```rust
impl<T> Range<T> {
    pub fn get_size(&self) -> (usize, usize);  // (rows, cols)
    pub fn get((row, col)) -> Option<&T>;
    pub fn rows(&self) -> Rows<'_, T>;         // Iterator over &[T] rows
    pub fn used_cells(&self) -> UsedCells<'_, T>;
    pub fn start(&self) -> Option<(u32, u32)>;
    pub fn end(&self) -> Option<(u32, u32)>;
    pub fn is_empty(&self) -> bool;
    pub fn range(&self) -> &Dimensions;
    // + Index<(usize, usize)>
}
```

### Convenience Functions

```rust
/// Open a workbook with automatic type inference from extension
pub fn open_workbook<R, P>(path: P) -> Result<R, R::Error>
where
    R: Reader<BufReader<File>>,
    P: AsRef<Path>;

/// Auto-detect format from file content
pub fn open_workbook_auto<P: AsRef<Path>>(path: P) -> Result<Sheets, Error>;
pub fn open_workbook_auto_from_rs<RS: Read + Seek>(reader: RS) -> Result<Sheets, Error>;
```

### Complete Example: Read + Deserialize

```rust
use calamine::{Reader, open_workbook, Xlsx, Data, RangeDeserializerBuilder};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Record {
    label: String,
    value: f64,
}

fn read_spreadsheet() -> Result<(), calamine::Error> {
    // Open typed workbook
    let mut workbook: Xlsx<_> = open_workbook("data.xlsx")?;

    // List sheets
    for sheet in workbook.sheet_names() {
        println!("Sheet: {sheet}");
    }

    // Read raw cell data
    let range = workbook.worksheet_range("Sheet1")?;
    let (rows, cols) = range.get_size();
    println!("{rows} rows x {cols} cols");

    for row in range.rows() {
        for cell in row.iter() {
            match cell {
                Data::Float(f)  => print!("{f:.2}\t"),
                Data::String(s) => print!("{s}\t"),
                Data::Int(i)    => print!("{i}\t"),
                Data::Bool(b)   => print!("{b}\t"),
                Data::Empty     => print!("\t"),
                _ => print!("?\t"),
            }
        }
        println!();
    }

    // Deserialize into structs (with header row)
    let range = workbook.worksheet_range("Sheet1")?;
    let iter = RangeDeserializerBuilder::new()
        .has_headers(true)
        .from_range::<_, Record>(&range)?;

    for result in iter {
        let record: Record = result?;
        println!("{:?}", record);
    }

    Ok(())
}

// Auto-detect format
fn read_any_spreadsheet(path: &str) -> Result<(), calamine::Error> {
    let mut workbook = open_workbook_auto(path)?;
    let sheets = workbook.sheet_names().to_owned();
    for name in &sheets {
        if let Ok(range) = workbook.worksheet_range(name) {
            println!("{name}: {} cells", range.get_size().0 * range.get_size().1);
        }
    }
    Ok(())
}
```

### Feature Flags

- `chrono` -- adds Chrono date/time type support
- `picture` -- enables reading embedded pictures

---

## 3. rust_xlsxwriter (v0.94.0)

**Purpose**: Write Excel 2007+ .xlsx files. Write-only, no read/modify. Extremely feature-rich.
**License**: MIT OR Apache-2.0 | **Repo**: https://github.com/jmcnamara/rust_xlsxwriter
**Key deps**: `zip` (only default dep)

### Core Types

```rust
pub struct Workbook;
pub struct Worksheet;
pub struct Format;
pub struct Chart;
pub struct Image;
pub struct Table;
pub struct Formula;
pub struct Url;
pub struct Note;
pub struct Shape;         // Textboxes
pub struct ExcelDateTime;
pub struct DocProperties;
pub struct DataValidation;
pub enum XlsxError { /* all error variants */ }
pub type XlsxResult<T> = Result<T, XlsxError>;
```

### Workbook Methods

```rust
impl Workbook {
    pub fn new() -> Workbook;
    pub fn add_worksheet(&mut self) -> &mut Worksheet;
    pub fn worksheet_from_index(&mut self, index: usize) -> Result<&mut Worksheet, XlsxError>;
    pub fn worksheet_from_name(&mut self, name: &str) -> Result<&mut Worksheet, XlsxError>;
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), XlsxError>;
    pub fn save_to_writer<W: Write + Seek>(&self, writer: W) -> Result<(), XlsxError>;
    pub fn save_to_buffer(&self) -> Result<Vec<u8>, XlsxError>;
    pub fn set_properties(&mut self, properties: &DocProperties) -> &mut Workbook;
    pub fn push_worksheet(&mut self, worksheet: Worksheet) -> &mut Workbook;
    pub fn add_vba_project(&mut self, vba_data: &[u8]) -> Result<&mut Workbook, XlsxError>;
}
```

### Worksheet Core Methods

```rust
impl Worksheet {
    // Generic write (auto-dispatches by type)
    pub fn write(&mut self, row: u32, col: u16, value: impl IntoExcelData) -> Result<&mut Self, XlsxError>;
    pub fn write_with_format(&mut self, row: u32, col: u16, value: impl IntoExcelData, format: &Format) -> Result<&mut Self, XlsxError>;

    // Specific write methods
    pub fn write_string(&mut self, row: u32, col: u16, value: &str) -> Result<&mut Self, XlsxError>;
    pub fn write_number(&mut self, row: u32, col: u16, value: f64) -> Result<&mut Self, XlsxError>;
    pub fn write_boolean(&mut self, row: u32, col: u16, value: bool) -> Result<&mut Self, XlsxError>;
    pub fn write_formula(&mut self, row: u32, col: u16, formula: impl Into<Formula>) -> Result<&mut Self, XlsxError>;
    pub fn write_url(&mut self, row: u32, col: u16, url: impl Into<Url>) -> Result<&mut Self, XlsxError>;
    pub fn write_datetime(&mut self, row: u32, col: u16, datetime: &ExcelDateTime) -> Result<&mut Self, XlsxError>;
    pub fn write_blank(&mut self, row: u32, col: u16, format: &Format) -> Result<&mut Self, XlsxError>;
    pub fn write_row(&mut self, row: u32, col: u16, data: impl IntoIterator<Item = impl IntoExcelData>) -> Result<&mut Self, XlsxError>;
    pub fn write_column(&mut self, row: u32, col: u16, data: impl IntoIterator<Item = impl IntoExcelData>) -> Result<&mut Self, XlsxError>;
    pub fn write_row_matrix(&mut self, row: u32, col: u16, data: impl IntoIterator<Item = impl IntoIterator<Item = impl IntoExcelData>>) -> Result<&mut Self, XlsxError>;

    // Layout
    pub fn set_column_width(&mut self, col: u16, width: impl Into<f64>) -> Result<&mut Self, XlsxError>;
    pub fn set_row_height(&mut self, row: u32, height: impl Into<f64>) -> Result<&mut Self, XlsxError>;
    pub fn set_column_format(&mut self, col: u16, format: &Format) -> Result<&mut Self, XlsxError>;
    pub fn autofit(&mut self) -> &mut Self;
    pub fn set_name(&mut self, name: &str) -> Result<&mut Self, XlsxError>;

    // Merging
    pub fn merge_range(&mut self, first_row: u32, first_col: u16, last_row: u32, last_col: u16, value: impl IntoExcelData, format: &Format) -> Result<&mut Self, XlsxError>;

    // Images
    pub fn insert_image(&mut self, row: u32, col: u16, image: &Image) -> Result<&mut Self, XlsxError>;

    // Charts
    pub fn insert_chart(&mut self, row: u32, col: u16, chart: &Chart) -> Result<&mut Self, XlsxError>;

    // Tables
    pub fn add_table(&mut self, first_row: u32, first_col: u16, last_row: u32, last_col: u16, table: &Table) -> Result<&mut Self, XlsxError>;

    // Serde (feature = "serde")
    pub fn serialize_headers(&mut self, row: u32, col: u16, data: &impl Serialize) -> Result<&mut Self, XlsxError>;
    pub fn serialize(&mut self, data: &impl Serialize) -> Result<&mut Self, XlsxError>;
}
```

### Format Builder

```rust
impl Format {
    pub fn new() -> Format;
    pub fn set_bold(self) -> Format;
    pub fn set_italic(self) -> Format;
    pub fn set_font_size(self, size: f64) -> Format;
    pub fn set_font_name(self, name: &str) -> Format;
    pub fn set_font_color(self, color: impl IntoColor) -> Format;
    pub fn set_num_format(self, format: &str) -> Format;
    pub fn set_align(self, align: FormatAlign) -> Format;
    pub fn set_border(self, border: FormatBorder) -> Format;
    pub fn set_background_color(self, color: impl IntoColor) -> Format;
    pub fn set_text_wrap(self) -> Format;
    pub fn set_indent(self, level: u8) -> Format;
    pub fn set_underline(self, underline: FormatUnderline) -> Format;
    pub fn set_font_strikethrough(self) -> Format;
    // ... many more
}
```

### Complete Example

```rust
use rust_xlsxwriter::*;

fn write_report() -> Result<(), XlsxError> {
    let mut workbook = Workbook::new();

    // Formats
    let bold = Format::new().set_bold();
    let money_fmt = Format::new().set_num_format("$#,##0.00");
    let date_fmt = Format::new().set_num_format("yyyy-mm-dd");
    let header_fmt = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x4472C4))
        .set_font_color(Color::White)
        .set_align(FormatAlign::Center);

    // Worksheet 1: Data table
    let ws = workbook.add_worksheet();
    ws.set_name("Sales Report")?;
    ws.set_column_width(0, 15)?;
    ws.set_column_width(1, 12)?;
    ws.set_column_width(2, 12)?;

    // Headers
    ws.write_with_format(0, 0, "Product", &header_fmt)?;
    ws.write_with_format(0, 1, "Date", &header_fmt)?;
    ws.write_with_format(0, 2, "Revenue", &header_fmt)?;

    // Data rows
    ws.write(1, 0, "Widget A")?;
    ws.write_with_format(1, 1, &ExcelDateTime::from_ymd(2026, 1, 15)?, &date_fmt)?;
    ws.write_with_format(1, 2, 1250.50, &money_fmt)?;

    ws.write(2, 0, "Widget B")?;
    ws.write_with_format(2, 1, &ExcelDateTime::from_ymd(2026, 2, 20)?, &date_fmt)?;
    ws.write_with_format(2, 2, 3420.00, &money_fmt)?;

    // Formula
    ws.write_with_format(3, 2, Formula::new("=SUM(C2:C3)"), &money_fmt)?;
    ws.write_with_format(3, 0, "Total", &bold)?;

    // Hyperlink
    ws.write(5, 0, Url::new("https://example.com").set_text("Report Link"))?;

    // Image
    let image = Image::new("logo.png")?;
    ws.insert_image(7, 0, &image)?;

    // Save
    workbook.save("report.xlsx")?;

    // Or save to buffer (for HTTP responses, etc.)
    let buffer: Vec<u8> = workbook.save_to_buffer()?;

    Ok(())
}
```

### Feature Flags

- `constant_memory` -- stream-write large files with constant memory
- `serde` -- serialize structs directly to worksheets
- `chrono` -- Chrono date/time integration
- `jiff` -- Jiff date/time integration
- `ryu` -- faster numeric formatting for large datasets
- `zlib` -- faster compression (requires C toolchain)
- `polars` -- Polars DataFrame integration
- `wasm` -- WebAssembly target support

---

## 4. cosmic-text (v0.18.2)

**Purpose**: Pure Rust multi-line text shaping, layout, font discovery, font fallback, and optional rasterization. Used by the COSMIC desktop environment.
**License**: MIT OR Apache-2.0 | **Repo**: https://github.com/pop-os/cosmic-text
**Key deps**: `fontdb`, `harfrust`, `skrifa`, `unicode-bidi`, `unicode-segmentation`

### Core Types

```rust
/// Font database and shaping system -- create ONE per application
pub struct FontSystem { /* fontdb::Database + locale + shape caches */ }

/// Rasterized glyph cache -- create ONE per application
pub struct SwashCache;  // requires feature "swash"

/// Text metrics: font size and line height
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    pub font_size: f32,    // in pixels
    pub line_height: f32,  // in pixels
}

/// Text buffer -- create ONE per text widget
pub struct Buffer {
    pub lines: Vec<BufferLine>,
    pub metrics: Metrics,
    pub scroll: Scroll,
    // ...
}

/// A convenience wrapper that borrows Buffer + FontSystem together
pub struct BorrowedWithFontSystem<'a>;

/// Font/style attributes for a range of text
pub struct Attrs<'a> {
    // family, weight, stretch, style, etc.
}
pub struct AttrsList;

/// A single line in a Buffer
pub struct BufferLine;

/// Output: a visible run of laid-out text
pub struct LayoutRun<'a> {
    pub line_i: usize,
    pub text: &'a str,
    pub rtl: bool,
    pub glyphs: &'a [LayoutGlyph],
    pub decorations: &'a [DecorationSpan],
    pub line_y: f32,
    pub line_top: f32,
    pub line_height: f32,
    pub line_w: f32,
}

/// A positioned glyph in a layout
pub struct LayoutGlyph {
    pub start: usize,        // byte offset in source text
    pub end: usize,
    pub x: f32,
    pub w: f32,
    pub level: unicode_bidi::Level,
    pub cache_key: CacheKey,
    pub x_offset: f32,
    pub y_offset: f32,
    pub color_opt: Option<Color>,
    // ...
}

/// Shaping strategy
pub enum Shaping {
    /// Basic shaping (no HarfBuzz, faster)
    Basic,
    /// Advanced shaping with HarfBuzz (ligatures, kerning, etc.)
    Advanced,
}

/// Text alignment
pub enum Align { Left, Right, Center, Justified, End }

/// Line wrapping mode
pub enum Wrap { None, Word, Glyph, WordOrGlyph }

/// Color type
pub struct Color(pub u32);  // ARGB packed
impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Color;
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color;
    pub fn r(&self) -> u8;
    pub fn g(&self) -> u8;
    pub fn b(&self) -> u8;
    pub fn a(&self) -> u8;
}

/// Cursor for text editing
pub struct Cursor { pub line: usize, pub index: usize, pub affinity: Affinity }

/// Font weight
pub struct Weight(pub u16);  // 100-900
/// Font stretch
pub struct Stretch(pub u8);  // UltraCondensed..UltraExpanded
/// Font style
pub enum Style { Normal, Italic, Oblique }
/// Font family
pub enum Family<'a> { Name(&'a str), Serif, SansSerif, Cursive, Fantasy, Monospace }
```

### FontSystem API

```rust
impl FontSystem {
    /// Create a new FontSystem, loading all system fonts
    pub fn new() -> Self;

    /// Create with a custom locale string
    pub fn new_with_locale_and_db(locale: String, db: fontdb::Database) -> Self;

    /// Access the underlying fontdb::Database
    pub fn db(&self) -> &fontdb::Database;
    pub fn db_mut(&mut self) -> &mut fontdb::Database;

    /// Get the locale
    pub fn locale(&self) -> &str;
}
```

### Metrics API

```rust
impl Metrics {
    pub const fn new(font_size: f32, line_height: f32) -> Self;
}
```

### Buffer API

```rust
impl Buffer {
    /// Create a new Buffer with given metrics
    pub fn new(font_system: &mut FontSystem, metrics: Metrics) -> Self;

    /// Create a "borrow" wrapper for convenience
    pub fn borrow_with<'a>(&'a mut self, font_system: &'a mut FontSystem)
        -> BorrowedWithFontSystem<'a>;

    /// Set text content, replacing all existing lines
    pub fn set_text(
        &mut self,
        font_system: &mut FontSystem,
        text: &str,
        attrs: &Attrs<'_>,
        shaping: Shaping,
        tab_width: Option<u16>,
    );

    /// Set the buffer dimensions (for wrapping)
    pub fn set_size(
        &mut self,
        font_system: &mut FontSystem,
        width: Option<f32>,
        height: Option<f32>,
    );

    /// Update metrics (font size, line height)
    pub fn set_metrics(&mut self, font_system: &mut FontSystem, metrics: Metrics);

    /// Set line wrapping mode
    pub fn set_wrap(&mut self, font_system: &mut FontSystem, wrap: Wrap);

    /// Perform shaping until content is ready for the visible scroll area
    pub fn shape_until_scroll(&mut self, font_system: &mut FontSystem, prune: bool);

    /// Iterate visible layout runs
    pub fn layout_runs(&self) -> LayoutRunIter<'_>;

    /// Access buffer lines
    pub fn lines(&self) -> &[BufferLine];

    /// Draw glyphs via callback (convenience, uses SwashCache internally)
    /// The callback receives: x, y, w, h, color for each rasterized glyph rectangle
    pub fn draw<F>(
        &self,
        font_system: &mut FontSystem,
        cache: &mut SwashCache,
        text_color: Color,
        f: F,
    ) where F: FnMut(i32, i32, u32, u32, Color);
}
```

### Attrs Builder

```rust
impl<'a> Attrs<'a> {
    pub fn new() -> Self;
    pub fn family(mut self, family: Family<'a>) -> Self;
    pub fn weight(mut self, weight: Weight) -> Self;
    pub fn stretch(mut self, stretch: Stretch) -> Self;
    pub fn style(mut self, style: Style) -> Self;
    pub fn color(mut self, color: Color) -> Self;
    pub fn metadata(mut self, metadata: usize) -> Self;
    pub fn cache_key_flags(mut self, flags: CacheKeyFlags) -> Self;
}
```

### Complete Example

```rust
use cosmic_text::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache, Weight,
};

fn render_text_to_pixels() {
    // 1. One-time setup (per application)
    let mut font_system = FontSystem::new();
    let mut swash_cache = SwashCache::new();

    // 2. Configure text metrics
    let metrics = Metrics::new(24.0, 30.0);  // 24px font, 30px line height

    // 3. Create buffer (per text widget)
    let mut buffer = Buffer::new(&mut font_system, metrics);

    // 4. Set dimensions for wrapping
    buffer.set_size(&mut font_system, Some(400.0), Some(300.0));

    // 5. Set text with attributes
    let attrs = Attrs::new()
        .family(Family::SansSerif)
        .weight(Weight::BOLD);
    buffer.set_text(&mut font_system, "Hello, cosmic-text!\nLine two.", &attrs, Shaping::Advanced, None);

    // 6. Shape (must call before layout)
    buffer.shape_until_scroll(&mut font_system, true);

    // 7. Iterate layout runs for custom rendering
    for run in buffer.layout_runs() {
        println!("Line {}: y={:.1}, {} glyphs", run.line_i, run.line_y, run.glyphs.len());
        for glyph in run.glyphs {
            println!("  glyph at ({:.1}, {:.1}) w={:.1}", glyph.x, glyph.y_offset, glyph.w);
        }
    }

    // 8. Or use the draw callback for pixel-level rendering
    let text_color = Color::rgb(0xFF, 0xFF, 0xFF);
    let mut pixel_buffer: Vec<u32> = vec![0; 400 * 300];
    buffer.draw(&mut font_system, &mut swash_cache, text_color, |x, y, w, h, color| {
        // Fill rectangle at (x, y) with size (w, h) using `color`
        // This is where you blit into your framebuffer/texture
        for py in y..(y + h as i32) {
            for px in x..(x + w as i32) {
                if px >= 0 && px < 400 && py >= 0 && py < 300 {
                    let idx = (py * 400 + px) as usize;
                    pixel_buffer[idx] = color.0;
                }
            }
        }
    });
}
```

### BorrowedWithFontSystem Convenience

```rust
// Instead of passing font_system to every call:
let mut buffer = buffer.borrow_with(&mut font_system);
buffer.set_size(Some(80.0), Some(25.0));
buffer.set_text("Hello!", &Attrs::new(), Shaping::Advanced, None);
buffer.shape_until_scroll(true);
```

### Feature Flags

- `std` (default) -- standard library support
- `no_std` -- no_std compatible (must pick one of std/no_std)
- `swash` -- enables SwashCache for glyph rasterization
- `syntect` -- syntax highlighting support
- `vi` -- vi-mode editing

---

## 5. gifski (v1.34.0)

**Purpose**: High-quality GIF animation encoder using pngquant. Produces smooth, well-dithered animated GIFs from arbitrary RGBA pixel data or PNG files.
**License**: AGPL-3.0-or-later | **Repo**: https://github.com/ImageOptim/gifski
**Key deps**: `imagequant`, `gif`, `crossbeam-channel`, `rgb`, `imgref`, `resize`

**IMPORTANT LICENSE NOTE**: AGPL-3.0 -- any network service using this must release source code.

### Public Types

```rust
/// Encoding settings
#[derive(Copy, Clone)]
pub struct Settings {
    pub width: Option<u32>,     // Resize to max this width (None = auto)
    pub height: Option<u32>,    // Resize to max this height (None = auto)
    pub quality: u8,            // 1-100, recommended: 80-100
    pub fast: bool,             // Lower quality but faster
    pub repeat: Repeat,         // gif::Repeat (Infinite, Finite(u16))
}

/// Frame collector -- feed frames into this (on one thread)
pub struct Collector {
    // Send frames via crossbeam channel
}

/// GIF writer -- writes output (on another thread)
pub struct Writer {
    // Receives processed frames, writes GIF
}

/// Error types
pub enum Error {
    Aborted,
    NoArg,
    WrongSize(String),
    Quant(imagequant::Error),
    Pal(gif_dispose::Error),
    ThreadSend,
    IO(io::Error),
    PNG(lodepng::Error),
    GIF(gif::EncodingError),
    Gifsicle,
}
pub type GifResult<T> = Result<T, Error>;

/// Number of repetitions
pub type Repeat = gif::Repeat;  // Infinite | Finite(u16)
```

### Top-Level Function

```rust
/// Create a new (Collector, Writer) pair for multi-threaded encoding.
/// Collector feeds frames; Writer writes the GIF output.
pub fn new(settings: Settings) -> GifResult<(Collector, Writer)>;
```

### Collector Methods

```rust
impl Collector {
    /// Add a frame from raw RGBA pixel data.
    /// frame_index: 0-based, set each index once (can be out of order)
    /// presentation_timestamp: seconds since start (frame 0 should be 0.0)
    pub fn add_frame_rgba(
        &self,
        frame_index: usize,
        frame: ImgVec<RGBA8>,
        presentation_timestamp: f64,
    ) -> GifResult<()>;

    /// Add a frame from in-memory PNG data (feature = "png")
    pub fn add_frame_png_data(
        &self,
        frame_index: usize,
        png_data: Vec<u8>,
        presentation_timestamp: f64,
    ) -> GifResult<()>;

    /// Add a frame from a PNG file path (feature = "png")
    pub fn add_frame_png_file(
        &self,
        frame_index: usize,
        path: PathBuf,
        presentation_timestamp: f64,
    ) -> GifResult<()>;
}
// Drop(Collector) signals end of input -- MUST drop before Writer::write() finishes
```

### Writer Methods

```rust
impl Writer {
    /// Write the GIF to any `Write` destination.
    /// Blocks until all frames are processed (Collector must be dropped).
    pub fn write<W: Write>(
        self,
        writer: W,
        progress: &mut dyn ProgressReporter,
    ) -> GifResult<()>;
}
```

### Progress Reporting

```rust
pub mod progress {
    pub trait ProgressReporter {
        fn increase(&mut self) -> bool;  // return false to abort
        fn done(&mut self, msg: &str);
    }
    pub struct NoProgress;  // no-op implementation
}
```

### Complete Example

```rust
use gifski::*;
use imgref::ImgVec;
use rgb::RGBA8;
use std::fs::File;

fn create_gif() -> GifResult<()> {
    let settings = Settings {
        width: Some(320),
        height: Some(240),
        quality: 90,
        fast: false,
        repeat: gif::Repeat::Infinite,
    };

    let (collector, writer) = gifski::new(settings)?;

    // Use scoped threads: collector on one, writer on another
    std::thread::scope(|scope| -> GifResult<()> {
        // Frame producer thread
        let frame_thread = scope.spawn(move || -> GifResult<()> {
            for i in 0..30 {
                let width = 320;
                let height = 240;
                let mut pixels = Vec::with_capacity(width * height);

                // Generate a frame (e.g., shifting gradient)
                for y in 0..height {
                    for x in 0..width {
                        pixels.push(RGBA8 {
                            r: ((x + i * 10) % 256) as u8,
                            g: ((y + i * 5) % 256) as u8,
                            b: ((x + y + i * 3) % 256) as u8,
                            a: 255,
                        });
                    }
                }

                let frame = ImgVec::new(pixels, width, height);
                let timestamp = i as f64 / 30.0;  // 30 fps
                collector.add_frame_rgba(i, frame, timestamp)?;
            }
            // MUST drop collector to signal end of input
            drop(collector);
            Ok(())
        });

        // Writer thread (current thread)
        let output = File::create("animation.gif")?;
        writer.write(output, &mut progress::NoProgress {})?;

        frame_thread.join().unwrap()?;
        Ok(())
    })?;

    Ok(())
}

// Simpler: from PNG files
fn gif_from_pngs() -> GifResult<()> {
    let (collector, writer) = gifski::new(Settings::default())?;

    std::thread::scope(|scope| -> GifResult<()> {
        let producer = scope.spawn(move || -> GifResult<()> {
            for i in 0..10 {
                let path = format!("frames/frame_{:04}.png", i).into();
                collector.add_frame_png_file(i, path, i as f64 * 0.1)?;
            }
            drop(collector);
            Ok(())
        });

        writer.write(File::create("output.gif")?, &mut progress::NoProgress {})?;
        producer.join().unwrap()
    })
}
```

### Key Notes

- **Multi-threaded by design**: `Collector` and `Writer` MUST run on separate threads.
- `drop(collector)` signals end-of-input; without it, `writer.write()` blocks forever.
- Frame indices can be submitted out of order (buffered in RAM).
- `ImgVec<RGBA8>` is from the `imgref` crate -- a simple owned image buffer.
- `RGBA8` is from the `rgb` crate -- `{r, g, b, a}` all `u8`.

---

## 6. webp-animation (v0.9.0)

**Purpose**: High-level Rust wrapper for encoding/decoding animated WebP images. Wraps Google's libwebp via `libwebp-sys2`.
**License**: MIT OR Apache-2.0 | **Repo**: https://github.com/blaind/webp-animation
**Key deps**: `libwebp-sys2` (C library FFI)

### Public Types

```rust
// ---- Encoder ----
pub struct Encoder;
pub struct EncoderOptions {
    pub anim_params: AnimParams,
    pub minimize_size: bool,        // default: false
    pub kmin: isize,                // min keyframe distance (0 = auto)
    pub kmax: isize,                // max keyframe distance (0 = auto)
    pub allow_mixed: bool,          // lossy+lossless per frame
    pub verbose: bool,
    pub color_mode: ColorMode,      // default: Rgba
    pub encoding_config: Option<EncodingConfig>,
}

pub struct AnimParams {
    pub loop_count: i32,            // 0 = infinite
}

pub struct EncodingConfig {
    pub encoding_type: EncodingType,
    pub quality: f32,               // 0-100
    pub method: usize,              // 0=fast, 6=slow+better
}

pub enum EncodingType {
    Lossy(LossyEncodingConfig),
    Lossless,
}

pub struct LossyEncodingConfig {
    pub target_size: usize,
    pub target_psnr: f32,
    pub segments: usize,            // 1..4
    pub sns_strength: usize,        // 0..100
    pub filter_strength: usize,     // 0..100
    pub filter_sharpness: usize,    // 0..7
    pub filter_type: usize,         // 0=simple, 1=strong
    pub autofilter: bool,
    pub alpha_compression: bool,
    pub alpha_filtering: usize,     // 0=none, 1=fast, 2=best
    pub alpha_quality: usize,       // 0..100
    pub pass: usize,                // 1..10
    pub show_compressed: bool,
    pub preprocessing: bool,
    pub partitions: usize,          // 0..3
    pub partition_limit: isize,
    pub use_sharp_yuv: bool,
}

// ---- Decoder ----
pub struct Decoder;
pub struct DecoderOptions {
    pub color_mode: ColorMode,
}
pub struct DecoderIterator;

// ---- Frame/Data ----
pub struct Frame {
    // decoded frame data + metadata
}
pub struct WebPData;   // wraps &[u8], Deref<Target=[u8]>

// ---- Color ----
pub enum ColorMode { Rgb, Rgba, Bgra, Bgr }
impl ColorMode {
    pub fn size(&self) -> usize;  // 3 or 4
}

// ---- Errors ----
pub enum Error {
    OptionsInitFailed,
    DecodeFailed,
    DecoderGetInfoFailed,
    TooLargeCanvas(u32, u32, usize),
    EncoderCreateFailed,
    BufferSizeFailed(usize, usize),
    PictureImportFailed,
    EncoderAddFailed,
    WrongColorMode(ColorMode, ColorMode),
    TimestampMustBeHigherThanPrevious(i32, i32),
    TimestampMustBeEqualOrHigherThanPrevious(i32, i32),
    EncoderAssmebleFailed,
    DimensionsMustbePositive,
    NoFramesAdded,
    ZeroSizeBuffer,
    InvalidEncodingConfig,
}
```

### Encoder API

```rust
impl Encoder {
    /// New encoder with default options
    pub fn new(dimensions: (u32, u32)) -> Result<Self, Error>;

    /// New encoder with custom options
    pub fn new_with_options(dimensions: (u32, u32), options: EncoderOptions) -> Result<Self, Error>;

    /// Add frame with RGBA (or configured ColorMode) pixel data.
    /// timestamp_ms must be strictly increasing between frames.
    pub fn add_frame(&mut self, data: &[u8], timestamp_ms: i32) -> Result<(), Error>;

    /// Add frame with per-frame encoding config
    pub fn add_frame_with_config(
        &mut self,
        data: &[u8],
        timestamp_ms: i32,
        config: &EncodingConfig,
    ) -> Result<(), Error>;

    /// Set default encoding config
    pub fn set_default_encoding_config(&mut self, config: EncodingConfig) -> Result<(), Error>;

    /// Finalize and get encoded WebP data.
    /// timestamp_ms is the end timestamp (determines last frame duration).
    pub fn finalize(self, timestamp_ms: i32) -> Result<WebPData, Error>;
}
```

### Decoder API

```rust
impl Decoder {
    /// Create decoder from WebP data bytes
    pub fn new(data: &[u8]) -> Result<Self, Error>;

    /// Create decoder with custom options
    pub fn new_with_options(data: &[u8], options: DecoderOptions) -> Result<Self, Error>;

    /// Iterate decoded frames
    pub fn into_iter(self) -> DecoderIterator;
}

impl Iterator for DecoderIterator {
    type Item = Frame;
}

impl Frame {
    pub fn data(&self) -> &[u8];          // raw pixel data
    pub fn dimensions(&self) -> (u32, u32);
    pub fn timestamp(&self) -> i32;       // ms
    pub fn color_mode(&self) -> ColorMode;
}

impl WebPData {
    // Deref<Target=[u8]> -- use as &[u8]
    pub fn len(&self) -> usize;
}
```

### Complete Example: Encode

```rust
use webp_animation::prelude::*;

fn create_animated_webp() -> Result<Vec<u8>, webp_animation::Error> {
    let (width, height) = (256, 256);

    // Simple encoder
    let mut encoder = Encoder::new((width, height))?;

    // Generate 10 frames
    for i in 0..10 {
        let mut frame_data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                frame_data.push(((x + i * 25) % 256) as u8); // R
                frame_data.push(((y + i * 10) % 256) as u8); // G
                frame_data.push(((x + y) % 256) as u8);       // B
                frame_data.push(255);                          // A
            }
        }
        encoder.add_frame(&frame_data, i as i32 * 100)?; // 100ms per frame
    }

    let webp_data = encoder.finalize(1000)?; // end at 1000ms
    std::fs::write("animation.webp", &*webp_data)?;

    Ok(webp_data.to_vec())
}

// With advanced options
fn create_lossy_webp() -> Result<(), webp_animation::Error> {
    let mut encoder = Encoder::new_with_options(
        (640, 480),
        EncoderOptions {
            kmin: 3,
            kmax: 5,
            encoding_config: Some(EncodingConfig {
                quality: 75.0,
                encoding_type: EncodingType::Lossy(LossyEncodingConfig {
                    segments: 2,
                    alpha_compression: true,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        },
    )?;

    // ... add frames ...
    Ok(())
}
```

### Complete Example: Decode

```rust
use webp_animation::prelude::*;

fn decode_webp(data: &[u8]) -> Result<(), webp_animation::Error> {
    let decoder = Decoder::new(data)?;

    for frame in decoder.into_iter() {
        println!(
            "Frame at {}ms: {}x{}, {} bytes",
            frame.timestamp(),
            frame.dimensions().0,
            frame.dimensions().1,
            frame.data().len(),
        );
        // frame.data() contains RGBA pixels by default
    }
    Ok(())
}
```

### Key Notes

- **Single-threaded** -- unlike gifski, encoding is synchronous on one thread.
- Timestamps must be **strictly increasing** for `add_frame()`.
- `WebPData` implements `Deref<Target=[u8]>` so it can be used as `&[u8]` directly.
- Requires C toolchain for `libwebp-sys2` build.

---

## 7. xcap (v0.9.2)

**Purpose**: Cross-platform screen capture library. Supports Linux (X11/Wayland), macOS, Windows. Screenshot + video recording (WIP).
**License**: Apache-2.0 | **Repo**: https://github.com/nashaofu/xcap
**Key deps**: platform-specific (CoreGraphics on macOS, XCB/pipewire on Linux, windows crate on Windows)

### Public Types

```rust
// Re-export
pub use image;  // The `image` crate -- RgbaImage, etc.

/// Monitor (display/screen)
#[derive(Debug, Clone)]
pub struct Monitor;

/// Application window
#[derive(Debug, Clone)]
pub struct Window;

/// Video frame (raw data)
#[derive(Debug, Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub raw: Vec<u8>,
}

/// Video recorder handle
#[derive(Debug, Clone)]
pub struct VideoRecorder;

/// Error type
pub enum XCapError {
    NotSupported,
    Error(String),
    StdSyncPoisonError(String),
    InvalidCaptureRegion(String),
    // + platform-specific variants (CGError, windows::core::Error, xcb::Error, etc.)
}
pub type XCapResult<T> = Result<T, XCapError>;
```

### Monitor API

```rust
impl Monitor {
    /// List all monitors
    pub fn all() -> XCapResult<Vec<Monitor>>;

    /// Get the monitor at a specific screen coordinate
    pub fn from_point(x: i32, y: i32) -> XCapResult<Monitor>;

    // --- Properties ---
    pub fn id(&self) -> XCapResult<u32>;
    pub fn name(&self) -> XCapResult<String>;
    pub fn friendly_name(&self) -> XCapResult<String>;
    pub fn x(&self) -> XCapResult<i32>;
    pub fn y(&self) -> XCapResult<i32>;
    pub fn width(&self) -> XCapResult<u32>;
    pub fn height(&self) -> XCapResult<u32>;
    pub fn rotation(&self) -> XCapResult<f32>;      // 0, 90, 180, 270
    pub fn scale_factor(&self) -> XCapResult<f32>;
    pub fn frequency(&self) -> XCapResult<f32>;     // refresh rate
    pub fn is_primary(&self) -> XCapResult<bool>;
    pub fn is_builtin(&self) -> XCapResult<bool>;

    // --- Capture ---
    /// Capture the entire monitor as an RGBA image
    pub fn capture_image(&self) -> XCapResult<RgbaImage>;

    /// Capture a sub-region of the monitor
    pub fn capture_region(&self, x: u32, y: u32, width: u32, height: u32) -> XCapResult<RgbaImage>;

    /// Start video recording (returns recorder handle + frame receiver)
    pub fn video_recorder(&self) -> XCapResult<(VideoRecorder, Receiver<Frame>)>;
}
```

### Window API

```rust
impl Window {
    /// List all windows, sorted by z-order
    pub fn all() -> XCapResult<Vec<Window>>;

    // --- Properties ---
    pub fn id(&self) -> XCapResult<u32>;
    pub fn pid(&self) -> XCapResult<u32>;           // process ID
    pub fn app_name(&self) -> XCapResult<String>;
    pub fn title(&self) -> XCapResult<String>;
    pub fn current_monitor(&self) -> XCapResult<Monitor>;
    pub fn x(&self) -> XCapResult<i32>;
    pub fn y(&self) -> XCapResult<i32>;
    pub fn z(&self) -> XCapResult<i32>;
    pub fn width(&self) -> XCapResult<u32>;
    pub fn height(&self) -> XCapResult<u32>;
    pub fn is_minimized(&self) -> XCapResult<bool>;
    pub fn is_maximized(&self) -> XCapResult<bool>;
    pub fn is_focused(&self) -> XCapResult<bool>;

    // --- Capture ---
    /// Capture the window as an RGBA image
    pub fn capture_image(&self) -> XCapResult<RgbaImage>;
}
```

### VideoRecorder API

```rust
impl VideoRecorder {
    pub fn start(&self) -> XCapResult<()>;
    pub fn stop(&self) -> XCapResult<()>;
}

impl Frame {
    pub fn new(width: u32, height: u32, raw: Vec<u8>) -> Self;
}
```

### Complete Example: Screenshot

```rust
use xcap::{Monitor, Window};

fn screenshot_all_monitors() -> xcap::XCapResult<()> {
    let monitors = Monitor::all()?;

    for (i, monitor) in monitors.iter().enumerate() {
        let name = monitor.name()?;
        let is_primary = monitor.is_primary()?;
        println!("Monitor {i}: {name} (primary={is_primary})");
        println!("  Resolution: {}x{}", monitor.width()?, monitor.height()?);
        println!("  Position: ({}, {})", monitor.x()?, monitor.y()?);
        println!("  Scale: {}x", monitor.scale_factor()?);

        // Capture full screen
        let image = monitor.capture_image()?;
        image.save(format!("monitor_{i}.png")).unwrap();

        // Capture a region (top-left 500x500)
        let region = monitor.capture_region(0, 0, 500, 500)?;
        region.save(format!("monitor_{i}_region.png")).unwrap();
    }

    Ok(())
}

fn screenshot_window() -> xcap::XCapResult<()> {
    let windows = Window::all()?;

    for window in &windows {
        let title = window.title()?;
        let app = window.app_name()?;
        println!("Window: '{title}' (app: {app}, {}x{})",
            window.width()?, window.height()?);

        if !window.is_minimized()? {
            let image = window.capture_image()?;
            // RgbaImage from the `image` crate
            println!("  Captured: {}x{} pixels", image.width(), image.height());
            // image.save("window.png").unwrap();
        }
    }

    Ok(())
}
```

### Complete Example: Video Recording

```rust
use xcap::Monitor;
use std::time::Duration;

fn record_screen() -> xcap::XCapResult<()> {
    let monitor = Monitor::all()?.into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .expect("No primary monitor");

    let (recorder, frame_rx) = monitor.video_recorder()?;

    // Start recording
    recorder.start()?;

    // Collect frames in a separate thread
    let handle = std::thread::spawn(move || {
        let mut frame_count = 0;
        while let Ok(frame) = frame_rx.recv() {
            println!("Frame {}: {}x{}, {} bytes",
                frame_count, frame.width, frame.height, frame.raw.len());
            frame_count += 1;
        }
        frame_count
    });

    // Record for 5 seconds
    std::thread::sleep(Duration::from_secs(5));

    // Stop recording
    recorder.stop()?;

    let total = handle.join().unwrap();
    println!("Recorded {total} frames");

    Ok(())
}
```

### Key Notes

- `capture_image()` returns `image::RgbaImage` (from the `image` crate, re-exported).
- All property accessors return `XCapResult<T>` (can fail on platform-specific issues).
- Video recording is WIP but functional with `VideoRecorder` + `mpsc::Receiver<Frame>`.
- macOS requires screen recording permission.
- Linux supports both X11 (via xcb) and Wayland (via pipewire/portals).
- `Window::all()` returns windows sorted by z-order (front to back).

---

## Cross-Reference: Cargo.toml Lines

```toml
[dependencies]
# Palette extraction
color-thief = "0.2"
rgb = "0.8"               # for Color type

# Spreadsheet reading
calamine = "0.34"

# Spreadsheet writing
rust_xlsxwriter = "0.94"

# Text layout + rendering
cosmic-text = { version = "0.18", features = ["swash"] }

# GIF creation (WARNING: AGPL-3.0)
gifski = "1.34"
imgref = "1.11"

# Animated WebP (requires C toolchain for libwebp)
webp-animation = "0.9"

# Screenshot
xcap = "0.9"
```

---

## Comparison Matrix

| Crate | Latest | License | Pure Rust | API Complexity | Thread Safety |
|-------|--------|---------|-----------|---------------|---------------|
| color-thief | 0.2.2 | MIT | Yes | Minimal (1 fn) | Send+Sync (stateless) |
| calamine | 0.34.0 | MIT | Yes | Medium (trait-based) | !Sync (Reader borrows) |
| rust_xlsxwriter | 0.94.0 | MIT/Apache-2.0 | Yes | Rich (builder pattern) | !Send (Workbook) |
| cosmic-text | 0.18.2 | MIT/Apache-2.0 | Yes | Complex (multi-phase) | !Send (FontSystem) |
| gifski | 1.34.0 | **AGPL-3.0** | Yes | Medium (2-thread) | Collector: Send, Writer: !Send |
| webp-animation | 0.9.0 | MIT/Apache-2.0 | No (libwebp FFI) | Simple (sequential) | !Send (raw pointers) |
| xcap | 0.9.2 | Apache-2.0 | No (platform FFI) | Simple (query+capture) | Monitor: Clone+Send |
