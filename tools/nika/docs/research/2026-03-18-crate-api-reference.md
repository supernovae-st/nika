# Rust Crate API Reference for Media Pipeline

> Researched: 2026-03-18 | For: Nika media pipeline PR2/PR3

---

## Table of Contents

1. [c2pa (C2PA Provenance SDK)](#1-c2pa---c2pa-provenance-sdk)
2. [thumbhash (Image Placeholder Hash)](#2-thumbhash---image-placeholder-hash)
3. [charts-rs (Chart Generation)](#3-charts-rs---chart-generation)
4. [symphonia (Audio Decoding)](#4-symphonia---audio-decoding)
5. [nom-exif (EXIF/Metadata Parsing)](#5-nom-exif---exifmetadata-parsing)
6. [ocrs (OCR Engine)](#6-ocrs---ocr-engine)
7. [lofty (Audio Tags)](#7-lofty---audio-tags)

---

## 1. c2pa -- C2PA Provenance SDK

**Crate**: [c2pa](https://crates.io/crates/c2pa) | **Repo**: [contentauth/c2pa-rs](https://github.com/contentauth/c2pa-rs) | **Docs**: [docs.rs/c2pa](https://docs.rs/c2pa)
**License**: MIT / Apache-2.0 | **MSRV**: 1.88.0

### Purpose

Create, sign, embed, read, and validate C2PA content provenance manifests. Supports images (JPEG, PNG, WebP, AVIF, HEIF, TIFF, GIF), video (MP4), audio, and PDF.

### Cargo.toml

```toml
[dependencies]
c2pa = { version = "0.72", features = ["file_io", "add_thumbnails"] }
```

### Feature Flags

| Flag | Purpose | Default |
|------|---------|---------|
| `file_io` | Enable file system I/O APIs | No |
| `add_thumbnails` | Auto-generate thumbnails for JPEG/PNG | No |
| `openssl` | Vendored OpenSSL crypto | Yes |
| `rust_native_crypto` | Rust-native crypto (preferred if both enabled) | No |
| `fetch_remote_manifests` | Fetch remote manifests over network | No |
| `pdf` | Basic PDF read support | No |

### Core Types

```
Builder           -- Build and sign manifests
Reader            -- Read and validate manifests from files
Ingredient        -- Parent/component asset in a manifest
ClaimGeneratorInfo -- Info about the tool generating the claim
Relationship      -- ParentOf, ComponentOf, etc.
HashRange         -- Byte range for data hash exclusions
Context           -- Thread-safe configuration (Arc<Context>)
```

### Key Assertion Types

```
c2pa::assertions::Actions      -- List of actions (LABEL = "c2pa.actions")
c2pa::assertions::Action       -- Single action (c2pa_action::OPENED, EDITED, etc.)
c2pa::assertions::CreativeWork -- schema.org CreativeWork (LABEL = "stds.schema-org.CreativeWork")
c2pa::assertions::Exif         -- EXIF data (LABEL = "stds.exif")
c2pa::assertions::BmffHash     -- BMFF container hash assertion
```

### Builder API

```rust
// Construction
Builder::new() -> Builder

// Configuration
builder.definition.claim_version = Some(2);
builder.set_claim_generator_info(info: ClaimGeneratorInfo) -> &mut Self
builder.add_ingredient(ingredient: Ingredient) -> &mut Self
builder.add_assertion(label: &str, assertion: &impl Serialize) -> Result<&mut Self>

// Signing (file_io feature)
builder.sign_file(signer: &dyn Signer, source: &Path, dest: &Path) -> Result<Vec<u8>>
builder.sign(signer: &dyn Signer, format: &str, source: &mut R, dest: &mut W) -> Result<Vec<u8>>

// Embeddable API (advanced -- low-level control)
builder.needs_placeholder(format: &str) -> bool
builder.placeholder(format: &str) -> Result<Vec<u8>>
builder.set_data_hash_exclusions(ranges: Vec<HashRange>) -> Result<()>
builder.update_hash_from_stream(format: &str, stream: &mut S) -> Result<()>
builder.set_bmff_mdat_hashes(leaf_hashes: Vec<Vec<Vec<u8>>>) -> Result<()>
builder.sign_embeddable(format: &str) -> Result<Vec<u8>>

// Remote
builder.remote_url = Some(url)
builder.no_embed = true
```

### Reader API

```rust
// Construction
Reader::from_file(path: &Path) -> Result<Reader>
Reader::from_stream(format: &str, stream: &mut R) -> Result<Reader>

// Navigation
reader.active_label() -> Option<&str>
reader.get_manifest(label: &str) -> Option<&Manifest>
reader.validation_status() -> Option<&[ValidationStatus]>

// Manifest access
manifest.title() -> Option<&str>
manifest.format() -> Option<&str>
manifest.instance_id() -> &str
manifest.assertions() -> &[AssertionData]
manifest.ingredients() -> &[Ingredient]

// Assertion access
assertion.label() -> &str
assertion.label_with_instance() -> String
assertion.to_assertion::<T: DeserializeOwned>() -> Result<T>
```

### Ingredient API

```rust
Ingredient::from_file(path: &Path) -> Result<Ingredient>
ingredient.set_relationship(rel: Relationship)
ingredient.instance_id() -> &str
ingredient.title() -> Option<&str>
ingredient.active_manifest() -> Option<&str>
ingredient.validation_status() -> Option<&[ValidationStatus]>
```

### Signer Creation

```rust
use c2pa::create_signer;
use c2pa::crypto::raw_signature::SigningAlg;

let signer = create_signer::from_files(
    "cert.pub",     // X.509 certificate chain
    "key.pem",      // Private key
    SigningAlg::Es256,
    None,           // Optional TSA URL
)?;
```

### Complete Example: Sign and Read

```rust
use c2pa::{
    assertions::{c2pa_action, Action, Actions, Exif},
    create_signer, crypto::raw_signature::SigningAlg,
    Builder, ClaimGeneratorInfo, Ingredient, Reader, Relationship,
};

fn main() -> anyhow::Result<()> {
    // Create ingredient from parent file
    let mut parent = Ingredient::from_file("input.jpg")?;
    parent.set_relationship(Relationship::ParentOf);

    // Build manifest
    let mut builder = Builder::new();
    builder.definition.claim_version = Some(2);
    builder
        .set_claim_generator_info(ClaimGeneratorInfo::new("MyApp").set_version("1.0"))
        .add_ingredient(parent)
        .add_assertion(
            Actions::LABEL,
            &Actions::new().add_action(Action::new(c2pa_action::OPENED)),
        )?;

    // Sign
    let signer = create_signer::from_files(
        "certs/es256.pub", "certs/es256.pem", SigningAlg::Es256, None
    )?;
    builder.sign_file(&*signer, "input.jpg", "output.jpg")?;

    // Read back
    let reader = Reader::from_file("output.jpg")?;
    println!("{reader}"); // JSON output
    Ok(())
}
```

---

## 2. thumbhash -- Image Placeholder Hash

**Crate**: [thumbhash](https://crates.io/crates/thumbhash) | **Repo**: [evanw/thumbhash](https://github.com/evanw/thumbhash) (subdirectory `rust/`)
**License**: MIT | **No dependencies**

### Purpose

Encode a tiny image into ~25 bytes that can be decoded into a blurry placeholder. Better than BlurHash: encodes more detail, preserves aspect ratio, supports alpha.

### Cargo.toml

```toml
[dependencies]
thumbhash = "0.1"
```

### Complete Public API (4 functions)

```rust
/// Encode RGBA image to ThumbHash bytes (~25 bytes).
/// w, h must be <= 100px. rgba must have w*h*4 elements.
/// RGB should NOT be premultiplied by A.
pub fn rgba_to_thumb_hash(w: usize, h: usize, rgba: &[u8]) -> Vec<u8>

/// Decode ThumbHash to RGBA image.
/// Returns (width, height, rgba_pixels). Output is ~32px max dimension.
/// Err(()) if input too short.
pub fn thumb_hash_to_rgba(hash: &[u8]) -> Result<(usize, usize, Vec<u8>), ()>

/// Extract average RGBA color from ThumbHash.
/// Returns (r, g, b, a) each in 0.0..1.0 range.
/// Err(()) if input too short (< 5 bytes).
pub fn thumb_hash_to_average_rgba(hash: &[u8]) -> Result<(f32, f32, f32, f32), ()>

/// Extract approximate aspect ratio from ThumbHash.
/// Returns width/height ratio as f32.
/// Err(()) if input too short (< 5 bytes).
pub fn thumb_hash_to_approximate_aspect_ratio(hash: &[u8]) -> Result<f32, ()>
```

### Complete Example: Encode and Decode

```rust
use thumbhash::{rgba_to_thumb_hash, thumb_hash_to_rgba, thumb_hash_to_average_rgba};

fn main() {
    // Encode: need RGBA pixels, max 100x100
    let w = 32;
    let h = 32;
    let rgba: Vec<u8> = vec![128; w * h * 4]; // dummy gray image

    let hash = rgba_to_thumb_hash(w, h, &rgba);
    println!("Hash: {} bytes", hash.len()); // ~25 bytes

    // Store hash as base64 (~33 chars) alongside your data
    let b64 = base64::encode(&hash);

    // Decode: produces ~32px blurry placeholder
    let (pw, ph, pixels) = thumb_hash_to_rgba(&hash).unwrap();
    println!("Placeholder: {}x{}", pw, ph);

    // Extract average color
    let (r, g, b, a) = thumb_hash_to_average_rgba(&hash).unwrap();
    println!("Avg color: rgba({},{},{},{})", r, g, b, a);
}
```

### Integration Pattern for Nika

```rust
use image::GenericImageView;

fn generate_thumbhash(path: &str) -> Vec<u8> {
    let img = image::open(path).unwrap();
    // Resize to max 100x100 (required constraint)
    let thumb = img.resize(100, 100, image::imageops::FilterType::Triangle);
    let (w, h) = thumb.dimensions();
    let rgba = thumb.to_rgba8().into_raw();
    thumbhash::rgba_to_thumb_hash(w as usize, h as usize, &rgba)
}
```

---

## 3. charts-rs -- Chart Generation

**Crate**: [charts-rs](https://crates.io/crates/charts-rs) | **Repo**: [vicanso/charts-rs](https://github.com/vicanso/charts-rs)
**License**: Apache-2.0 | **Inspired by**: Apache ECharts

### Purpose

Generate charts as SVG/PNG/JPEG/WebP/AVIF. 10 chart types, 9 themes, JSON configuration support.

### Cargo.toml

```toml
[dependencies]
charts-rs = "0.4"
```

### Chart Types

| Type | Struct | JSON Support |
|------|--------|-------------|
| Bar | `BarChart` | Yes |
| Horizontal Bar | `HorizontalBarChart` | Yes |
| Line | `LineChart` | Yes |
| Pie | `PieChart` | Yes |
| Radar | `RadarChart` | Yes |
| Scatter | `ScatterChart` | Yes |
| Candlestick | `CandlestickChart` | Yes |
| Table | `TableChart` | Yes |
| Heatmap | `HeatmapChart` | Yes |
| Multi | `MultiChart` | Yes |

### Themes (Constants)

```rust
use charts_rs::{THEME_LIGHT, THEME_DARK, THEME_GRAFANA, THEME_ANT,
                THEME_VINTAGE, THEME_WALDEN, THEME_WESTEROS, THEME_CHALK, THEME_SHINE};
```

### Core Types

```rust
Series              -- Data series: name + data points + optional category
SeriesCategory      -- Line, Bar (for mixed charts)
Box                 -- Margin: { top, right, bottom, left }
YAxisConfig         -- Y-axis configuration
Color               -- Color type
Align               -- left, center, right
LegendCategory      -- round_rect, circle, rect
Symbol              -- Mark symbol type
```

### Output Methods (all chart types)

```rust
chart.svg() -> Result<String>        // SVG string
svg_to_png(svg: &str) -> Result<Vec<u8>>   // Convert SVG to PNG bytes
svg_to_jpeg(svg: &str) -> Result<Vec<u8>>  // Convert SVG to JPEG bytes
svg_to_webp(svg: &str) -> Result<Vec<u8>>  // Convert SVG to WebP bytes
svg_to_avif(svg: &str) -> Result<Vec<u8>>  // Convert SVG to AVIF bytes
```

### Construction Patterns

```rust
// Pattern 1: From JSON (all chart types)
let chart = BarChart::from_json(json_str)?;

// Pattern 2: With theme
let chart = BarChart::new_with_theme(series_list, x_axis_data, THEME_GRAFANA);

// Pattern 3: Basic constructor
let chart = BarChart::new(series_list, x_axis_data);
```

### Series Construction

```rust
// From tuple (shorthand)
let s: Series = ("Email", vec![120.0, 132.0, 101.0]).into();

// Explicit
let s = Series::new("Email".to_string(), vec![120.0, 132.0, 101.0]);

// Mixed chart (line on bar chart)
let mut s = Series::new("Temperature".to_string(), vec![2.0, 2.2, 3.3]);
s.category = Some(SeriesCategory::Line);
s.y_axis_index = 1;       // Second y-axis
s.label_show = true;
```

### Complete Example: Bar Chart from JSON

```rust
use charts_rs::{BarChart, svg_to_png};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let chart = BarChart::from_json(r#"{
        "width": 630,
        "height": 410,
        "title_text": "Weekly Report",
        "sub_title_text": "Generated by Nika",
        "legend_align": "left",
        "series_list": [
            {"name": "Tasks", "label_show": true, "data": [12, 19, 8, 15, 22, 30, 18]},
            {"name": "Errors", "data": [2, 3, 1, 4, 2, 5, 3]}
        ],
        "x_axis_data": ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
    }"#)?;

    let svg = chart.svg()?;
    let png_bytes = svg_to_png(&svg)?;
    std::fs::write("chart.png", png_bytes)?;
    Ok(())
}
```

### Complete Example: Line Chart Programmatic

```rust
use charts_rs::{LineChart, Series, Box as ChartBox, THEME_DARK};

let mut line = LineChart::new_with_theme(
    vec![
        ("CPU", vec![23.0, 45.0, 67.0, 34.0, 89.0]).into(),
        ("Memory", vec![55.0, 62.0, 78.0, 45.0, 91.0]).into(),
    ],
    vec!["10:00", "11:00", "12:00", "13:00", "14:00"]
        .into_iter().map(String::from).collect(),
    THEME_DARK,
);
line.title_text = "System Metrics".to_string();
line.series_smooth = true;
line.series_fill = true;
line.y_axis_configs[0].axis_formatter = Some("{c}%".to_string());

let svg = line.svg().unwrap();
```

### Custom Fonts

```rust
use charts_rs::get_or_try_init_fonts;
let font_data = std::fs::read("CustomFont.ttf")?;
get_or_try_init_fonts(Some(vec![&font_data]))?;
```

---

## 4. symphonia -- Audio Decoding and Demuxing

**Crate**: [symphonia](https://crates.io/crates/symphonia) | **Repo**: [pdeljanov/Symphonia](https://github.com/pdeljanov/Symphonia)
**License**: MPL v2.0 | **Pure Rust** | **MSRV**: 1.53+

### Purpose

Pure Rust audio decoding and media demuxing. Supports AAC, ADPCM, AIFF, ALAC, CAF, FLAC, MKV, MP1, MP2, MP3, MP4, OGG, Vorbis, WAV, WebM.

### Cargo.toml

```toml
[dependencies]
symphonia = { version = "0.5", features = ["all"] }
# Or selective:
# symphonia = { version = "0.5", features = ["mp3", "isomp4", "aac", "flac", "ogg", "vorbis", "wav"] }
```

### Feature Flags

| Flag | Content | Default |
|------|---------|---------|
| `all` | All formats + codecs | No |
| `all-formats` | All demuxers | No |
| `all-codecs` | All decoders | No |
| `mp3` / `mpa` | MP1/MP2/MP3 | No |
| `aac` | AAC-LC | No |
| `alac` | Apple Lossless | No |
| `flac` | FLAC | Yes |
| `vorbis` | Vorbis | Yes |
| `pcm` | PCM | Yes |
| `ogg` | OGG container | Yes |
| `mkv` | MKV/WebM container | Yes |
| `wav` | WAV container | Yes |
| `isomp4` | MP4/M4A container | No |
| `aiff` / `caf` | AIFF / CAF | No |
| `opt-simd` | All SIMD optimizations | No |

### Core Types

```
symphonia::core::io::MediaSourceStream  -- Buffered media source
symphonia::core::io::MediaSource        -- Trait: Read + Seek
symphonia::core::io::ReadOnlySource     -- Wrap non-seekable Read
symphonia::core::probe::Hint            -- File type hint
symphonia::core::probe::Probe           -- Format detection
symphonia::core::formats::FormatReader  -- Trait: demuxer
symphonia::core::formats::FormatOptions -- Demuxer options
symphonia::core::formats::Track         -- Track metadata
symphonia::core::codecs::Decoder        -- Trait: audio decoder
symphonia::core::codecs::DecoderOptions -- Decoder options
symphonia::core::codecs::CodecRegistry  -- Codec registry
symphonia::core::audio::AudioBuffer<S>  -- Typed sample buffer
symphonia::core::audio::AudioBufferRef  -- Enum of AudioBuffer variants
symphonia::core::audio::SampleBuffer<S> -- Interleaved output buffer
symphonia::core::audio::RawSampleBuffer -- Byte-oriented output
symphonia::core::audio::Signal          -- Trait for audio buffers
symphonia::core::meta::MetadataOptions  -- Metadata parsing options
symphonia::core::errors::Error          -- Error enum
```

### Key Enums

```rust
// AudioBufferRef variants
AudioBufferRef::U8(AudioBuffer<u8>)
AudioBufferRef::U16(AudioBuffer<u16>)
AudioBufferRef::U24(AudioBuffer<u24>)
AudioBufferRef::U32(AudioBuffer<u32>)
AudioBufferRef::S8(AudioBuffer<i8>)
AudioBufferRef::S16(AudioBuffer<i16>)
AudioBufferRef::S24(AudioBuffer<i24>)
AudioBufferRef::S32(AudioBuffer<i32>)
AudioBufferRef::F32(AudioBuffer<f32>)
AudioBufferRef::F64(AudioBuffer<f64>)

// Error variants
Error::IoError(_)
Error::DecodeError(_)
Error::SeekError(_)
Error::Unsupported(_)
Error::LimitError(_)
Error::ResetRequired
```

### Default Registries

```rust
symphonia::default::get_probe()  -> &'static Probe       // All enabled formats
symphonia::default::get_codecs() -> &'static CodecRegistry // All enabled codecs
```

### FormatReader API

```rust
reader.tracks() -> &[Track]                    // List tracks
reader.next_packet() -> Result<Packet>         // Get next packet
reader.metadata() -> &MetadataLog              // Metadata queue
reader.seek(mode, to) -> Result<SeekedTo>      // Seek
reader.default_track() -> Option<&Track>       // Default track
```

### Decoder API

```rust
decoder.decode(&packet) -> Result<AudioBufferRef>   // Decode one packet
decoder.last_decoded() -> AudioBufferRef             // Last decoded buffer
decoder.codec_params() -> &CodecParameters           // Codec info
decoder.reset()                                      // Reset decoder state
```

### AudioBuffer / Signal API

```rust
buf.chan(idx) -> &[S]           // Samples for channel
buf.planes() -> Planes          // View as slice-of-slices
buf.spec() -> &SignalSpec       // Sample rate, channels
buf.capacity() -> usize         // Max frames
buf.frames() -> usize           // Current frames
```

### Complete Example: Decode Audio File

```rust
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::audio::{SampleBuffer, Signal};

fn decode_audio(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let src = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path).extension() {
        hint.with_extension(ext.to_str().unwrap_or(""));
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let mut format = probed.format;

    let track = format.tracks().iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("no audio track")?;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

    let track_id = track.id;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut total_samples = 0u64;

    loop {
        let packet = match format.next_packet() {
            Ok(pkt) => pkt,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };

        if packet.track_id() != track_id { continue; }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                if sample_buf.is_none() {
                    sample_buf = Some(SampleBuffer::new(
                        decoded.capacity() as u64, *decoded.spec()
                    ));
                }
                if let Some(buf) = &mut sample_buf {
                    buf.copy_interleaved_ref(decoded);
                    total_samples += buf.samples().len() as u64;
                }
            }
            Err(Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }

    println!("Decoded {} samples", total_samples);
    Ok(())
}
```

### Metadata Access

```rust
// From probe result
if let Some(metadata) = probed.metadata.get() {
    if let Some(rev) = metadata.current() {
        for tag in rev.tags() {
            println!("{}: {}", tag.std_key.map(|k| format!("{:?}", k))
                .unwrap_or_else(|| tag.key.clone()), tag.value);
        }
    }
}

// During decode loop
while !format.metadata().is_latest() {
    format.metadata().pop();
    if let Some(rev) = format.metadata().current() {
        // Process metadata revision
    }
}
```

---

## 5. nom-exif -- EXIF/Metadata Parsing

**Crate**: [nom-exif](https://crates.io/crates/nom-exif) | **Repo**: [mindeng/nom-exif](https://github.com/mindeng/nom-exif)
**License**: MIT | **Pure Rust** (built on `nom` parser)

### Purpose

Parse EXIF and metadata from images (HEIC, JPEG, TIFF, RAF, CR3) and video/audio (MP4, MOV, WebM, MKV, MKA, 3GP). Unified API for all formats. Supports sync and async.

### Cargo.toml

```toml
[dependencies]
nom-exif = "2"
# For async:
# nom-exif = { version = "2", features = ["async"] }
```

### Core Types

```
MediaParser          -- Reusable parser (shares I/O buffer)
MediaSource          -- Abstraction over file/stream/reader
AsyncMediaParser     -- Async variant
AsyncMediaSource     -- Async variant
ExifIter             -- Lazy iterator over EXIF entries
Exif                 -- Random-access EXIF data (from ExifIter)
TrackInfo            -- Video/audio track metadata
ExifTag              -- Enum of standard EXIF tags
TrackInfoTag         -- Enum of track metadata tags
GpsInfo              -- Parsed GPS information
EntryValue           -- EXIF entry value (string, int, rational, etc.)
Result<T>            -- nom_exif::Result<T>
```

### MediaSource Construction

```rust
// From file path
MediaSource::file_path("photo.jpg")?

// From Read + Seek
MediaSource::seekable(reader)?

// From Read only (no seeking -- less efficient for some formats)
MediaSource::unseekable(reader)?

// From TCP stream
MediaSource::tcp_stream(stream)?
```

### MediaSource Query Methods

```rust
ms.has_exif() -> bool    // Can parse as EXIF image
ms.has_track() -> bool   // Can parse as video/audio track
```

### MediaParser API

```rust
let mut parser = MediaParser::new();

// Parse as EXIF (images)
let iter: ExifIter = parser.parse(ms)?;

// Parse as track info (video/audio)
let info: TrackInfo = parser.parse(ms)?;
```

### ExifIter API

```rust
// Iterate entries lazily
for entry in &mut iter {
    println!("{}: {:?}", entry.tag(), entry.value());
}

// Convert to random-access Exif
let exif: Exif = iter.into();

// Parse GPS info
let gps = iter.parse_gps_info()?;
```

### Exif API

```rust
exif.get(ExifTag::Make) -> Option<&EntryValue>
exif.get(ExifTag::Model) -> Option<&EntryValue>
exif.get(ExifTag::DateTimeOriginal) -> Option<&EntryValue>
exif.get(ExifTag::ImageWidth) -> Option<&EntryValue>
exif.get(ExifTag::ImageHeight) -> Option<&EntryValue>
exif.get_gps_info() -> Option<GpsInfo>
```

### EntryValue Methods

```rust
value.as_str() -> Option<&str>
value.as_u32() -> Option<u32>
value.as_time_components() -> Option<(NaiveDateTime, Option<FixedOffset>)>
```

### TrackInfo API

```rust
info.get(TrackInfoTag::Make) -> Option<&EntryValue>
info.get(TrackInfoTag::Model) -> Option<&EntryValue>
info.get(TrackInfoTag::Software) -> Option<&EntryValue>
info.get(TrackInfoTag::CreateDate) -> Option<&EntryValue>
info.get(TrackInfoTag::DurationMs) -> Option<&EntryValue>
info.get(TrackInfoTag::ImageWidth) -> Option<&EntryValue>
info.get(TrackInfoTag::ImageHeight) -> Option<&EntryValue>
info.get(TrackInfoTag::GpsIso6709) -> Option<&EntryValue>
info.get_gps_info() -> Option<GpsInfo>
```

### GpsInfo API

```rust
gps.latitude_ref -> char        // 'N' or 'S'
gps.longitude_ref -> char       // 'E' or 'W'
gps.latitude -> Rational        // Degrees, minutes, seconds
gps.longitude -> Rational
gps.altitude -> Option<f64>
gps.format_iso6709() -> String  // "+43.29013+084.22713+1595.950CRSWGS_84/"
```

### Common ExifTag Values

```rust
ExifTag::Make
ExifTag::Model
ExifTag::Software
ExifTag::DateTimeOriginal
ExifTag::ModifyDate
ExifTag::ImageWidth
ExifTag::ImageHeight
ExifTag::Orientation
ExifTag::FNumber
ExifTag::ExposureTime
ExifTag::ISOSpeedRatings
ExifTag::FocalLength
ExifTag::Flash
ExifTag::MeteringMode
ExifTag::LightSource
ExifTag::GPSLatitude
ExifTag::GPSLongitude
ExifTag::GPSAltitude
```

### Complete Example

```rust
use nom_exif::*;

fn extract_metadata(path: &str) -> Result<()> {
    let mut parser = MediaParser::new();
    let ms = MediaSource::file_path(path)?;

    if ms.has_exif() {
        let mut iter: ExifIter = parser.parse(ms)?;
        let exif: Exif = iter.into();

        if let Some(make) = exif.get(ExifTag::Make) {
            println!("Camera: {}", make.as_str().unwrap_or("unknown"));
        }
        if let Some(dt) = exif.get(ExifTag::DateTimeOriginal) {
            if let Some((ndt, offset)) = dt.as_time_components() {
                println!("Date: {} {:?}", ndt, offset);
            }
        }
        if let Some(gps) = exif.get_gps_info() {
            println!("GPS: {}", gps.format_iso6709());
        }
    } else if ms.has_track() {
        let info: TrackInfo = parser.parse(ms)?;
        println!("Duration: {:?}ms", info.get(TrackInfoTag::DurationMs));
        println!("Size: {:?}x{:?}",
            info.get(TrackInfoTag::ImageWidth),
            info.get(TrackInfoTag::ImageHeight));
    }
    Ok(())
}
```

---

## 6. ocrs -- OCR Engine

**Crate**: [ocrs](https://crates.io/crates/ocrs) | **Repo**: [robertknight/ocrs](https://github.com/robertknight/ocrs)
**License**: MIT (Apache-2.0?) | **Depends on**: `rten` (ONNX runtime), `rten-imageproc`

### Purpose

Extract text from images using neural network models. Supports text detection, layout analysis, and recognition. Latin alphabet only (currently). Uses ONNX models via the RTen inference engine.

### Cargo.toml

```toml
[dependencies]
ocrs = "0.8"
rten = "0.14"
image = "0.25"
```

### Required Models (download separately)

```
text-detection.rten     -- Text word detection model
text-recognition.rten   -- Text line recognition model
```

Models available from: https://github.com/robertknight/ocrs-models

### Core Types

```
OcrEngine            -- Main engine (holds detector + recognizer)
OcrEngineParams      -- Engine configuration
OcrInput             -- Preprocessed image for OCR
ImageSource          -- Input image wrapper
ImageSourceError     -- Image loading error
DecodeMethod         -- CTC decode method
TextLine             -- Recognized text line
TextWord             -- Recognized text word
TextChar             -- Individual character with position
RotatedRect          -- Oriented bounding rectangle (from rten_imageproc)
DimOrder             -- Channel dimension order (Chw, Hwc)
```

### OcrEngineParams

```rust
#[derive(Default)]
pub struct OcrEngineParams {
    pub detection_model: Option<rten::Model>,     // Text detection
    pub recognition_model: Option<rten::Model>,   // Text recognition
    pub debug: bool,                              // Debug logging
    pub decode_method: DecodeMethod,              // CTC decode strategy
    pub alphabet: Option<String>,                 // Custom alphabet
    pub allowed_chars: Option<String>,            // Filter to specific chars
}
```

### OcrEngine Methods

```rust
// Construction
OcrEngine::new(params: OcrEngineParams) -> anyhow::Result<OcrEngine>

// Preprocessing
engine.prepare_input(image: ImageSource) -> anyhow::Result<OcrInput>

// Detection
engine.detect_words(input: &OcrInput) -> anyhow::Result<Vec<RotatedRect>>
engine.detect_text_pixels(input: &OcrInput) -> anyhow::Result<NdTensor<f32, 2>>

// Layout analysis
engine.find_text_lines(input: &OcrInput, words: &[RotatedRect]) -> Vec<Vec<RotatedRect>>

// Recognition
engine.recognize_text(input: &OcrInput, lines: &[Vec<RotatedRect>]) -> anyhow::Result<Vec<Option<TextLine>>>

// High-level convenience
engine.get_text(input: &OcrInput) -> anyhow::Result<String>

// Configuration query
engine.detection_threshold() -> f32
engine.prepare_recognition_input(input: &OcrInput, lines: &[Vec<RotatedRect>]) -> ...
```

### ImageSource Construction

```rust
// From raw bytes + dimensions
ImageSource::from_bytes(data: &[u8], (width, height): (u32, u32)) -> Result<ImageSource>

// From tensor
ImageSource::from_tensor(tensor: NdTensorView<f32, 3>, order: DimOrder) -> Result<ImageSource>
```

### TextLine / TextWord / TextChar

```rust
line.to_string() -> String              // Full line text
line.words() -> &[TextWord]             // Words in line

word.to_string() -> String              // Word text
word.chars() -> &[TextChar]             // Characters

char.char() -> char                     // The character
char.rect() -> RotatedRect              // Bounding box
```

### Complete Example

```rust
use ocrs::{ImageSource, OcrEngine, OcrEngineParams};
use rten::Model;

fn ocr_image(image_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let detection_model = Model::load_file("text-detection.rten")?;
    let recognition_model = Model::load_file("text-recognition.rten")?;

    let engine = OcrEngine::new(OcrEngineParams {
        detection_model: Some(detection_model),
        recognition_model: Some(recognition_model),
        ..Default::default()
    })?;

    // Load and convert image
    let img = image::open(image_path)?.into_rgb8();
    let img_source = ImageSource::from_bytes(img.as_raw(), img.dimensions())?;
    let ocr_input = engine.prepare_input(img_source)?;

    // Simple: get all text at once
    let text = engine.get_text(&ocr_input)?;
    Ok(text)
}

fn ocr_with_layout(image_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    // ... (engine setup as above) ...

    let ocr_input = engine.prepare_input(img_source)?;

    // Step-by-step for layout info
    let word_rects = engine.detect_words(&ocr_input)?;
    let line_rects = engine.find_text_lines(&ocr_input, &word_rects);
    let line_texts = engine.recognize_text(&ocr_input, &line_rects)?;

    for line in line_texts.iter().flatten() {
        println!("{}", line);
        for word in line.words() {
            println!("  word: {} at {:?}", word, word.chars()[0].rect());
        }
    }
    Ok(())
}
```

### Filtering Characters

```rust
// Only recognize digits
let engine = OcrEngine::new(OcrEngineParams {
    detection_model: Some(det_model),
    recognition_model: Some(rec_model),
    allowed_chars: Some("0123456789".to_string()),
    ..Default::default()
})?;
```

### Performance Note

Always build in **release mode**. Debug builds of ocrs and rten are extremely slow.

---

## 7. lofty -- Audio Tags

**Crate**: [lofty](https://crates.io/crates/lofty) | **Repo**: [Serial-ATA/lofty-rs](https://github.com/Serial-ATA/lofty-rs)
**License**: MIT / Apache-2.0

### Purpose

Parse, convert, and write metadata tags to audio files. Supports 12 audio formats with multiple tag types per format.

### Cargo.toml

```toml
[dependencies]
lofty = "0.21"
```

### Supported Formats

| Format | Tag Types |
|--------|-----------|
| AAC (ADTS) | ID3v2, ID3v1 |
| APE | APE, ID3v2*, ID3v1 |
| AIFF | ID3v2, Text Chunks |
| FLAC | Vorbis Comments, ID3v2* |
| MP3 | ID3v2, ID3v1, APE |
| MP4 | iTunes-style ilst |
| MPC | APE, ID3v2*, ID3v1* |
| Opus | Vorbis Comments |
| Ogg Vorbis | Vorbis Comments |
| Speex | Vorbis Comments |
| WAV | ID3v2, RIFF INFO |
| WavPack | APE, ID3v1 |

\* Read only

### Core Types

```
lofty::probe::Probe        -- File format detection + reading
lofty::file::TaggedFile    -- File with tags and properties
lofty::tag::Tag            -- Mutable tag
lofty::tag::TagType        -- Id3v2, Id3v1, Ape, VorbisComments, Mp4Ilst, RiffInfo, AiffText
lofty::tag::ItemKey        -- Standard tag keys (Title, Artist, Album, etc.)
lofty::properties::FileProperties -- Duration, bitrate, sample rate, etc.
lofty::picture::Picture    -- Embedded picture
lofty::picture::PictureType -- Front cover, back cover, etc.
lofty::config::ParseOptions -- Parsing configuration
lofty::config::WriteOptions -- Writing configuration
```

### Prelude (common traits)

```rust
use lofty::prelude::*;
// Imports: AudioFile, TaggedFileExt, Accessor, ItemKey, MergeTag, SplitTag, TagExt
```

### Reading Tags

```rust
use lofty::prelude::*;
use lofty::probe::Probe;

// Pattern 1: From path (guess format from extension)
let tagged_file = lofty::read_from_path("song.mp3")?;

// Pattern 2: From path with content-based detection
let tagged_file = Probe::open("song.mp3")?.guess_file_type()?.read()?;

// Pattern 3: From reader
let mut file = std::fs::File::open("song.mp3")?;
let tagged_file = lofty::read_from(&mut file)?;

// Pattern 4: Concrete file type
use lofty::mpeg::MpegFile;
use lofty::file::AudioFile;
let mp3 = MpegFile::read_from(&mut file, ParseOptions::new())?;
```

### TaggedFile API

```rust
// Tag access (TaggedFileExt trait)
file.primary_tag() -> Option<&Tag>           // Primary tag for format
file.primary_tag_mut() -> Option<&mut Tag>
file.first_tag() -> Option<&Tag>             // First available tag
file.first_tag_mut() -> Option<&mut Tag>
file.tag(tag_type: TagType) -> Option<&Tag>
file.tag_mut(tag_type: TagType) -> Option<&mut Tag>
file.tags() -> &[Tag]
file.insert_tag(tag: Tag)
file.remove(tag_type: TagType) -> Option<Tag>
file.contains_tag_type(tag_type: TagType) -> bool
file.primary_tag_type() -> TagType

// Properties (AudioFile trait)
file.properties() -> &FileProperties
```

### FileProperties API

```rust
props.duration() -> Duration
props.audio_bitrate() -> Option<u32>       // kbps
props.overall_bitrate() -> Option<u32>     // kbps
props.sample_rate() -> Option<u32>         // Hz
props.bit_depth() -> Option<u8>
props.channels() -> Option<u8>
```

### Tag API (Accessor trait)

```rust
// Common fields
tag.title() -> Option<Cow<str>>
tag.artist() -> Option<Cow<str>>
tag.album() -> Option<Cow<str>>
tag.genre() -> Option<Cow<str>>
tag.track() -> Option<u32>
tag.track_total() -> Option<u32>
tag.disk() -> Option<u32>
tag.disk_total() -> Option<u32>
tag.year() -> Option<u32>
tag.comment() -> Option<Cow<str>>

// Set fields
tag.set_title(value: String)
tag.set_artist(value: String)
tag.set_album(value: String)
tag.set_genre(value: String)
tag.set_track(value: u32)
tag.set_year(value: u32)
tag.set_comment(value: String)

// Generic key access
tag.get_string(key: ItemKey) -> Option<&str>
tag.get(key: &ItemKey) -> Option<&TagItem>

// Pictures
tag.pictures() -> &[Picture]
tag.push_picture(picture: Picture)

// Save
tag.save_to_path(path: &Path, options: WriteOptions) -> Result<()>
tag.save_to(&mut writer, options: WriteOptions) -> Result<()>

// Tag type
tag.tag_type() -> TagType
tag.len() -> usize
tag.is_empty() -> bool
```

### ItemKey (Standard Keys)

```rust
ItemKey::TrackTitle
ItemKey::TrackArtist
ItemKey::AlbumTitle
ItemKey::AlbumArtist
ItemKey::Genre
ItemKey::TrackNumber
ItemKey::TrackTotal
ItemKey::DiscNumber
ItemKey::DiscTotal
ItemKey::Year
ItemKey::RecordingDate
ItemKey::Comment
ItemKey::Composer
ItemKey::Conductor
ItemKey::Lyrics
ItemKey::Encoder
ItemKey::EncodedBy
ItemKey::Bpm
ItemKey::ReplayGainTrackGain
ItemKey::ReplayGainTrackPeak
ItemKey::ReplayGainAlbumGain
ItemKey::ReplayGainAlbumPeak
ItemKey::MusicBrainzTrackId
ItemKey::MusicBrainzReleaseId
ItemKey::MusicBrainzArtistId
```

### Complete Example: Read Tags

```rust
use lofty::prelude::*;
use lofty::probe::Probe;

fn read_audio_metadata(path: &str) -> Result<(), lofty::error::LoftyError> {
    let tagged_file = Probe::open(path)?.read()?;

    let tag = tagged_file.primary_tag()
        .or_else(|| tagged_file.first_tag())
        .ok_or_else(|| lofty::error::LoftyError::new(lofty::error::ErrorKind::NotFound))?;

    println!("Title:  {}", tag.title().as_deref().unwrap_or("Unknown"));
    println!("Artist: {}", tag.artist().as_deref().unwrap_or("Unknown"));
    println!("Album:  {}", tag.album().as_deref().unwrap_or("Unknown"));
    println!("Genre:  {}", tag.genre().as_deref().unwrap_or("Unknown"));
    println!("Album Artist: {}",
        tag.get_string(lofty::tag::ItemKey::AlbumArtist).unwrap_or("Unknown"));

    let props = tagged_file.properties();
    let dur = props.duration();
    println!("Duration: {:02}:{:02}",
        dur.as_secs() / 60, dur.as_secs() % 60);
    println!("Bitrate:     {} kbps", props.audio_bitrate().unwrap_or(0));
    println!("Sample rate: {} Hz", props.sample_rate().unwrap_or(0));
    println!("Channels:    {}", props.channels().unwrap_or(0));
    println!("Bit depth:   {}", props.bit_depth().unwrap_or(0));

    Ok(())
}
```

### Complete Example: Write Tags

```rust
use lofty::prelude::*;
use lofty::config::WriteOptions;
use lofty::probe::Probe;
use lofty::tag::Tag;

fn write_tags(path: &str) -> Result<(), lofty::error::LoftyError> {
    let mut tagged_file = Probe::open(path)?.read()?;

    let tag = match tagged_file.primary_tag_mut() {
        Some(t) => t,
        None => {
            let tag_type = tagged_file.primary_tag_type();
            tagged_file.insert_tag(Tag::new(tag_type));
            tagged_file.primary_tag_mut().unwrap()
        }
    };

    tag.set_title("My Song".to_string());
    tag.set_artist("My Artist".to_string());
    tag.set_album("My Album".to_string());
    tag.set_genre("Electronic".to_string());
    tag.set_track(1);
    tag.set_year(2026);

    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}
```

---

## Summary Table

| Crate | Version | Purpose | Key API Entry |
|-------|---------|---------|---------------|
| `c2pa` | 0.72 | Content provenance | `Builder::new()` / `Reader::from_file()` |
| `thumbhash` | 0.1 | Placeholder hashes | `rgba_to_thumb_hash()` / `thumb_hash_to_rgba()` |
| `charts-rs` | 0.4 | Chart generation | `BarChart::from_json()` / `.svg()` |
| `symphonia` | 0.5 | Audio decode/demux | `get_probe().format()` / `decoder.decode()` |
| `nom-exif` | 2.x | EXIF/metadata | `MediaParser::new()` / `parser.parse()` |
| `ocrs` | 0.8 | OCR text extraction | `OcrEngine::new()` / `engine.get_text()` |
| `lofty` | 0.21 | Audio tags read/write | `Probe::open().read()` / `tag.save_to_path()` |

---

## Sources

1. [c2pa-rs README](https://github.com/contentauth/c2pa-rs) -- Project overview, features
2. [c2pa usage docs](https://github.com/contentauth/c2pa-rs/blob/main/docs/usage.md) -- API features, resource refs
3. [c2pa embeddable API](https://github.com/contentauth/c2pa-rs/blob/main/docs/embeddable-api.md) -- Low-level signing
4. [c2pa client example](https://github.com/contentauth/c2pa-rs/blob/main/sdk/examples/client/client.rs) -- Full Builder+Reader usage
5. [thumbhash rust/src/lib.rs](https://github.com/evanw/thumbhash/blob/main/rust/src/lib.rs) -- Complete source (4 functions)
6. [charts-rs README](https://github.com/vicanso/charts-rs) -- Chart types, themes, examples
7. [charts-rs lib.rs](https://github.com/vicanso/charts-rs/blob/main/src/lib.rs) -- JSON and programmatic examples
8. [Symphonia README](https://github.com/pdeljanov/Symphonia) -- Format/codec support tables
9. [Symphonia GETTING_STARTED.md](https://github.com/pdeljanov/Symphonia/blob/master/GETTING_STARTED.md) -- Full decode loop tutorial
10. [nom-exif README](https://github.com/mindeng/nom-exif) -- Unified API, GPS, ExifIter
11. [ocrs README + lib.rs](https://github.com/robertknight/ocrs) -- OcrEngine API surface
12. [ocrs hello_ocr.rs](https://github.com/robertknight/ocrs/blob/main/ocrs/examples/hello_ocr.rs) -- Complete example
13. [lofty README + lib.rs](https://github.com/Serial-ATA/lofty-rs) -- Supported formats, module structure
14. [lofty tag_reader.rs](https://github.com/Serial-ATA/lofty-rs/blob/main/examples/tag_reader.rs) -- Read example
15. [lofty tag_writer.rs](https://github.com/Serial-ATA/lofty-rs/blob/main/examples/tag_writer.rs) -- Write example

## Methodology

- Tools used: curl (raw GitHub URLs), source code inspection
- Pages analyzed: 25+
- All API surfaces extracted from actual source code (lib.rs, examples, docs)
- Version numbers are approximate; check crates.io for latest

## Confidence Level

**High** -- All data sourced directly from official GitHub repositories and source code. API signatures verified against actual Rust source files.
