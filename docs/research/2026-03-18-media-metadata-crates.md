# Research Report: Rust Crates for Universal Media Metadata Extraction

**Date**: 2026-03-18
**Purpose**: Identify the best Rust crates for building a universal media metadata extractor in Nika's media pipeline.

## Summary

The Rust ecosystem has mature, mostly pure-Rust solutions for every media metadata category. The recommended stack is: `infer` (type detection) + `imagesize` (dimensions) + `kamadak-exif` (EXIF) + `nom-exif` (unified image+video metadata) + `lofty` (audio tags) + `symphonia` (audio containers) + `mp4` (video containers). All are pure Rust except where noted.

---

## 1. File Type Detection (Magic Bytes)

### infer -- RECOMMENDED
| Field | Value |
|-------|-------|
| Crate | `infer` |
| Version | 0.19.0 |
| Downloads | 77.3M total / 13M recent |
| Pure Rust | Yes (zero dependencies by default) |
| Repository | https://github.com/bojand/infer |

**What it detects**: MIME type and extension from magic bytes. Supports 100+ file types: images (JPEG, PNG, GIF, WebP, AVIF, HEIF, TIFF, BMP, PSD, ICO, JXL), video (MP4, MKV, WebM, AVI, MOV, FLV), audio (MP3, FLAC, WAV, OGG, AAC, AIFF, M4A), documents (PDF, DOCX, XLSX, PPTX), archives, fonts. Only needs the first few bytes of a file.

**Why best**: Massively adopted (77M downloads), zero deps, minimal API (`infer::get(&buf)` returns `Type { mime, ext }`). Ideal as the first step in a pipeline to route to specialized extractors.

### file-format
| Field | Value |
|-------|-------|
| Crate | `file-format` |
| Version | 0.28.0 |
| Downloads | 1M total / 212K recent |
| Pure Rust | Yes (zero dependencies) |

**What it detects**: File format from magic bytes, similar to `infer` but with a richer `FileFormat` enum that includes format name, short name, media type, and known extensions. Covers 400+ formats including niche ones (DjVu, OpenRaster, Radiance HDR, etc.).

**When to prefer**: If you need the format enum with detailed metadata about the format itself (not just MIME string).

### tree_magic_mini
| Field | Value |
|-------|-------|
| Crate | `tree_magic_mini` |
| Version | 3.2.2 |
| Downloads | 6.8M total / 2.1M recent |
| Pure Rust | Yes |

**What it detects**: Uses the FreeDesktop shared MIME database. Hierarchical type matching (e.g., `image/svg+xml` is also `application/xml` is also `text/plain`). More accurate for edge cases but heavier.

**When to prefer**: When you need inheritance-aware MIME type checking (e.g., "is this file a subtype of text?").

---

## 2. Image Dimensions (Without Full Decode)

### imagesize -- RECOMMENDED
| Field | Value |
|-------|-------|
| Crate | `imagesize` |
| Version | 0.14.0 |
| Downloads | 12.4M total / 4.1M recent |
| Pure Rust | Yes (ZERO dependencies) |
| Repository | https://github.com/Roughsketch/imagesize |

**What it extracts**: Width and height from image headers. Supports: AVIF, BMP, DDS, Farbfeld, GIF, HDR, HEIF/HEIC, ICO, JPEG, JPEG XL, KTX2, PNG, PNM (PBM/PGM/PPM/PAM), PSD, QOI, TGA, TIFF, VTF, WebP.

**Why best**: Zero dependencies, reads only enough bytes to determine dimensions (typically < 1KB), extremely fast. The `size()` function returns `ImageSize { width, height }` from a path or reader.

### image-meta
| Field | Value |
|-------|-------|
| Crate | `image-meta` |
| Version | 0.1.4 |
| Downloads | 26K |
| Pure Rust | Yes |

**What it extracts**: Width, height, and animation info. Fewer format support than `imagesize`. Not recommended over `imagesize`.

---

## 3. EXIF Metadata (Images)

### kamadak-exif -- RECOMMENDED (read-only, battle-tested)
| Field | Value |
|-------|-------|
| Crate | `kamadak-exif` |
| Version | 0.6.1 |
| Downloads | 8.4M total / 1.5M recent |
| Pure Rust | Yes (1 dependency: `mutate_once`) |
| Repository | https://github.com/kamadak/exif-rs |

**What it extracts**: Full EXIF IFD fields from JPEG and TIFF. Camera make/model, exposure time, f-number, ISO, date/time, GPS coordinates, lens info, orientation, flash, white balance, color space, focal length, metering mode, scene type, sharpness, saturation, contrast, and all standard EXIF 2.32+ tags. Read-only.

**Why best**: Most downloaded EXIF crate with strong recent momentum (1.5M/quarter). Minimal dependency tree. Used by the `image` crate ecosystem.

### rexif
| Field | Value |
|-------|-------|
| Crate | `rexif` |
| Version | 0.7.5 |
| Downloads | 8.9M total / 414K recent |
| Pure Rust | Yes |

**What it extracts**: Same EXIF fields as kamadak-exif. JPEG and TIFF only. Includes GPS parsing with lat/lon helpers.

**Trade-off**: Higher total downloads but declining trajectory (414K vs kamadak's 1.5M recent). API is slightly less ergonomic.

### nom-exif -- RECOMMENDED (multi-format, image + video)
| Field | Value |
|-------|-------|
| Crate | `nom-exif` |
| Version | 2.7.0 |
| Downloads | 104K total / 23K recent |
| Pure Rust | Yes |
| Repository | https://github.com/mindeng/nom-exif |

**What it extracts**: EXIF from images (JPEG, HEIF/HEIC, TIFF) AND metadata from video/audio (MOV, MP4, 3GP, WebM, MKV, MKA). Extracts: GPS, date/time, camera info, video duration, resolution, codec info. Unified API across image and video formats.

**Why notable**: The ONLY crate that handles both image EXIF and video metadata in a single API. Built on `nom` parser combinators. Growing fast. Async support via optional tokio feature.

### little_exif
| Field | Value |
|-------|-------|
| Crate | `little_exif` |
| Version | 0.6.23 |
| Downloads | 107K total / 40K recent |
| Pure Rust | Yes |

**What it extracts**: Full EXIF read AND WRITE. Supports PNG, JPEG, HEIF/HEIC/HIF, AVIF, JXL, TIFF, WebP. The broadest format support for EXIF write operations.

**When to prefer**: When you need to write/modify EXIF data, not just read it. Only pure-Rust crate with true read+write support across this many formats.

### gufo-exif + gufo-xmp (paired)
| Field | Value |
|-------|-------|
| Crate | `gufo-exif` / `gufo-xmp` |
| Version | 0.4.0 / 0.4.0 |
| Downloads | 90K / 35K |
| Pure Rust | Yes |

**What it extracts**: EXIF and XMP as a coordinated pair. Part of the `gufo` project (GNOME ecosystem). Good for applications that need both EXIF and XMP from the same file.

### rexiv2 (FFI wrapper)
| Field | Value |
|-------|-------|
| Crate | `rexiv2` |
| Version | 0.10.0 |
| Downloads | 102K total |
| Pure Rust | NO -- wraps gexiv2 (C) which wraps Exiv2 (C++) |

**What it extracts**: EXIF, IPTC, and XMP from virtually any image format. The most complete metadata extraction but requires system libraries (gexiv2, exiv2).

**When to prefer**: When you need ALL metadata types (EXIF + IPTC + XMP) from a single crate and can tolerate C/C++ dependencies.

---

## 4. XMP Metadata (Adobe)

### xmp_toolkit -- RECOMMENDED (full read+write, Adobe official)
| Field | Value |
|-------|-------|
| Crate | `xmp_toolkit` |
| Version | 1.12.0 |
| Downloads | 196K total / 5.8K recent |
| Pure Rust | NO -- wraps Adobe's C++ XMP Toolkit SDK |
| Repository | https://github.com/adobe/xmp-toolkit-rs |

**What it extracts/writes**: Full XMP packet read/write. Dublin Core (title, creator, description, rights), Photoshop (color mode, ICC profile), TIFF/EXIF (via XMP mirror), Camera Raw settings, PDF metadata, video metadata, custom namespaces. The official Adobe implementation.

**Trade-off**: C++ dependency but it IS the reference implementation. Low recent downloads suggest niche use.

### xmp-writer (write-only)
| Field | Value |
|-------|-------|
| Crate | `xmp-writer` |
| Version | 0.3.2 |
| Downloads | 867K total / 388K recent |
| Pure Rust | Yes |
| Repository | https://github.com/typst/xmp-writer |

**What it does**: XMP packet WRITING only (no reading). Step-by-step builder API. Used by the Typst project for PDF metadata embedding.

**When to prefer**: When you only need to generate XMP packets (e.g., for PDF or image export). Much higher adoption than xmp_toolkit.

### gufo-xmp (read+write, pure Rust)
| Field | Value |
|-------|-------|
| Crate | `gufo-xmp` |
| Version | 0.4.0 |
| Downloads | 35K total / 7K recent |
| Pure Rust | Yes |

**What it extracts**: XMP packet reading and editing. Pure Rust alternative to xmp_toolkit. Paired with gufo-exif for coordinated metadata access.

**When to prefer**: When you need pure-Rust XMP reading and don't want the Adobe C++ dependency.

---

## 5. Audio Metadata (ID3, Vorbis Comments, MP4 tags, FLAC)

### lofty -- RECOMMENDED (universal audio tags)
| Field | Value |
|-------|-------|
| Crate | `lofty` |
| Version | 0.23.3 |
| Downloads | 489K total / 87K recent |
| Pure Rust | Yes |
| Repository | https://github.com/Serial-ATA/lofty-rs |

**What it extracts**: Unified API for ALL audio tag formats:
- **ID3v1/ID3v2** (MP3): title, artist, album, year, genre, track, comments, attached pictures, lyrics, custom frames
- **Vorbis Comments** (FLAC, OGG, Opus): arbitrary key-value pairs, cover art
- **MP4/M4A iTunes atoms**: title, artist, album, cover art, tempo, compilation flag, gapless playback
- **APE tags** (Musepack, WavPack, APE): arbitrary key-value pairs
- **RIFF INFO** (WAV): title, artist, comments
- **AIFF text chunks**: name, author, comments

Supports: MP3, FLAC, OGG Vorbis, Opus, MP4/M4A/AAC, WAV, AIFF, WavPack, Musepack, APE, Speex.

**Why best**: The most comprehensive pure-Rust audio metadata crate. Reads AND writes. Actively maintained. Proper error handling. Format-agnostic `TaggedFile` API plus format-specific access when needed.

### id3
| Field | Value |
|-------|-------|
| Crate | `id3` |
| Version | 1.16.4 |
| Downloads | 10M total / 421K recent |
| Pure Rust | Yes |

**What it extracts**: ID3v1 and ID3v2 tags from MP3 files. Title, artist, album, year, genre, track, comments, attached pictures (album art), lyrics, custom text frames.

**Trade-off**: Much higher downloads than lofty but only handles ID3 (MP3). If you only need MP3 support, this is the gold standard.

### symphonia -- RECOMMENDED (audio container parsing + codec probing)
| Field | Value |
|-------|-------|
| Crate | `symphonia` |
| Version | 0.5.5 |
| Downloads | 4.9M total / 1.4M recent |
| Pure Rust | Yes |
| Repository | https://github.com/pdeljanov/Symphonia |

**What it extracts**: Audio container metadata AND codec information. Duration, sample rate, bit depth, channel count, codec type, bitrate. Also extracts embedded tags (ID3, Vorbis Comments, etc.) via `symphonia-metadata`. Supports: MP3, FLAC, WAV, AIFF, AAC, OGG, MP4/M4A, MKV/MKA, CAF.

**Why notable**: This is not just a tag reader -- it parses the actual container/codec to give you technical metadata (duration, sample rate, channels, bitrate) that tag-only libraries cannot provide. Essential for audio duration/format probing.

### mp4ameta
| Field | Value |
|-------|-------|
| Crate | `mp4ameta` |
| Version | 0.13.0 |
| Downloads | 731K total / 65K recent |
| Pure Rust | Yes |

**What it extracts**: iTunes-style MPEG-4 audio tags (M4A/MP4 audio). Title, artist, album, album art, genre, year, track/disc number, tempo, compilation flag, rating, purchase date, custom atoms.

### metaflac
| Field | Value |
|-------|-------|
| Crate | `metaflac` |
| Version | 0.2.8 |
| Downloads | 194K total / 17K recent |
| Pure Rust | Yes |

**What it extracts**: FLAC metadata blocks: stream info (sample rate, channels, bits per sample, total samples, duration), Vorbis comments, cover art (PICTURE block), seek tables, application metadata.

---

## 6. Video Metadata (MP4, MKV, WebM, MOV)

### mp4 -- RECOMMENDED (MP4/MOV)
| Field | Value |
|-------|-------|
| Crate | `mp4` |
| Version | 0.14.0 |
| Downloads | 9.8M total / 838K recent |
| Pure Rust | Yes |
| Repository | https://github.com/alfg/mp4-rust |

**What it extracts**: Full MP4/MOV box parsing. Duration, timescale, creation/modification time, video track (width, height, codec, frame rate, bitrate), audio track (sample rate, channels, codec, bitrate), subtitle tracks, chapter markers, overall bitrate.

**Why best**: 10M downloads, pure Rust, reads AND writes MP4. Gives you detailed per-track metadata including codec parameters.

### mp4parse (Mozilla)
| Field | Value |
|-------|-------|
| Crate | `mp4parse` |
| Version | 0.17.0 |
| Downloads | 863K total / 206K recent |
| Pure Rust | Yes |
| Repository | https://github.com/mozilla/mp4parse-rust |

**What it extracts**: ISO Base Media File Format (ISOBMFF) parsing. Used in Firefox. More focused on correctness and security than the `mp4` crate. Extracts tracks, codec info, duration, sample tables. Hardened against malicious inputs.

**When to prefer**: When parsing untrusted MP4 files (user uploads) and security is paramount. Mozilla's fuzzing infrastructure backs this crate.

### matroska -- RECOMMENDED (MKV/WebM)
| Field | Value |
|-------|-------|
| Crate | `matroska` |
| Version | 0.30.0 |
| Downloads | 145K total / 14K recent |
| Pure Rust | Yes |
| Repository | https://github.com/tuffy/matroska |

**What it extracts**: MKV/WebM container metadata. Duration, title, muxing/writing application, video tracks (codec, width, height, display dimensions, frame rate, interlacing, color info), audio tracks (codec, sample rate, channels, bit depth), subtitle tracks, chapter info, attachments, tags.

### symphonia-format-isomp4 + symphonia-format-mkv
| Field | Value |
|-------|-------|
| Crate | `symphonia-format-isomp4` / `symphonia-format-mkv` |
| Version | 0.5.5 / 0.5.5 |
| Downloads | 2.7M / 2.4M |
| Pure Rust | Yes |

**What they extract**: Part of the Symphonia ecosystem. Parse MP4 and MKV containers for audio stream metadata. Focus is on audio streams within video containers, not video track metadata per se.

### ffprobe (CLI wrapper)
| Field | Value |
|-------|-------|
| Crate | `ffprobe` |
| Version | 0.4.0 |
| Downloads | 800K total / 541K recent |
| Pure Rust | Rust wrapper, but REQUIRES ffprobe binary installed |
| Repository | https://github.com/theduke/ffprobe-rs |

**What it extracts**: Everything ffprobe can extract -- literally all media metadata for any format ffmpeg supports. Duration, codec, bitrate, resolution, frame rate, color space, HDR metadata, audio channels, sample rate, container format, stream mapping, chapters, embedded subtitles, etc.

**Trade-off**: The nuclear option. Extracts the most metadata of any solution but requires ffprobe/ffmpeg installed at runtime. Consider as a fallback for formats not covered by pure-Rust crates.

### nom-exif (also covers video)
Already covered in Section 3. Handles MOV/MP4/3GP/WebM/MKV metadata extraction in the same API as image EXIF. Extracts duration, resolution, creation date, GPS from video containers.

---

## 7. ICC Color Profile Extraction

### moxcms -- RECOMMENDED (pure Rust CMS)
| Field | Value |
|-------|-------|
| Crate | `moxcms` |
| Version | 0.8.1 |
| Downloads | 17.8M total / 10M recent |
| Pure Rust | Yes |
| Repository | https://github.com/awxkee/moxcms |

**What it does**: Full color management system. Parses ICC profiles, performs color space transforms (sRGB, Display P3, Adobe RGB, ProPhoto RGB, CMYK). Extracts ICC profile header: color space, PCS, device class, rendering intent, illuminant, creation date, profile version.

**Why best**: Massively adopted (10M recent downloads -- trending), pure Rust, actively maintained. The modern replacement for lcms2 bindings.

### lcms2 (FFI wrapper)
| Field | Value |
|-------|-------|
| Crate | `lcms2` |
| Version | 6.1.1 |
| Downloads | 4.5M total / 464K recent |
| Pure Rust | NO -- wraps Little CMS (C library) |

**What it does**: Industry-standard ICC profile handling. Full v2/v4 ICC profile parsing, color transforms, proofing, named color profiles. The most feature-complete ICC solution.

**When to prefer**: When you need advanced ICC features (proofing, device link profiles, abstract profiles, named colors) and can tolerate a C dependency.

### qcms (Mozilla)
| Field | Value |
|-------|-------|
| Crate | `qcms` |
| Version | 0.3.0 |
| Downloads | 859K total / 397K recent |
| Pure Rust | Mostly (optional libc dep) |
| Repository | https://github.com/FirefoxGraphics/qcms |

**What it does**: Lightweight color management. Ported from Firefox's C implementation. Parses ICC profiles and performs fast color transforms. Focused on speed over completeness.

### img-parts (ICC extraction from containers)
| Field | Value |
|-------|-------|
| Crate | `img-parts` |
| Version | 0.4.0 |
| Downloads | 9.1M total / 342K recent |
| Pure Rust | Yes |

**What it does**: Low-level JPEG/PNG/RIFF container manipulation. Can extract and inject ICC profile chunks (iCCP in PNG, APP2 in JPEG). Does not parse the ICC profile itself -- pairs well with moxcms/lcms2 for that.

---

## 8. Additional Metadata Types

### IPTC (Press/News metadata)
| Field | Value |
|-------|-------|
| Crate | `iptc` |
| Version | 0.3.0 |
| Downloads | 6.5K |
| Pure Rust | Yes |

**What it extracts**: IPTC-IIM tags from JPEG/TIFF: caption, headline, credit, source, copyright, keywords, city, country, date created, byline, special instructions. Niche but important for press/stock photography workflows.

### C2PA (Content Provenance)
| Field | Value |
|-------|-------|
| Crate | `c2pa` |
| Version | 0.78.4 |
| Downloads | 8.6M total |
| Pure Rust | Mixed (some C deps) |

**What it extracts**: Coalition for Content Provenance and Authenticity manifests. Provenance chain, editing history, AI generation claims. The future of media authentication.

---

## Recommended Stack for Nika's Universal Media Metadata Extractor

### Tier 1: Essential (add these)

| Purpose | Crate | Why |
|---------|-------|-----|
| Type detection | `infer` | Zero-dep, 77M downloads, routes to specialized extractors |
| Image dimensions | `imagesize` | Zero-dep, 12M downloads, instant dimensions without decode |
| Image EXIF | `kamadak-exif` | 1-dep, 8M downloads, battle-tested EXIF reader |
| Audio tags | `lofty` | Pure Rust, all tag formats, read+write, actively maintained |
| MP4/MOV metadata | `mp4` | Pure Rust, 10M downloads, duration/resolution/codec per track |

### Tier 2: Enhanced coverage

| Purpose | Crate | Why |
|---------|-------|-----|
| MKV/WebM metadata | `matroska` | Pure Rust, MKV/WebM container parsing |
| Multi-format EXIF+video | `nom-exif` | Unified API for both image EXIF and video metadata |
| Audio probing | `symphonia` | Duration, sample rate, channels, bitrate for all audio formats |
| ICC profiles | `moxcms` | Pure Rust CMS, parse ICC profile headers |

### Tier 3: Specialized (add as needed)

| Purpose | Crate | Why |
|---------|-------|-----|
| XMP read | `gufo-xmp` | Pure Rust XMP reading |
| XMP write | `xmp-writer` | Pure Rust XMP generation (for export) |
| ID3 only | `id3` | If you need ID3-specific deep features |
| ICC extraction from images | `img-parts` | Extract raw ICC chunks from JPEG/PNG containers |
| IPTC | `iptc` | Press/stock photography metadata |
| Fallback for everything | `ffprobe` | When nothing else works (requires ffmpeg) |

### Proposed Dependency Configuration

```toml
[dependencies]
# Tier 1 - always on
infer = "0.19"
imagesize = "0.14"
kamadak-exif = "0.6"
lofty = "0.23"
mp4 = "0.14"

# Tier 2 - feature-gated
matroska = { version = "0.30", optional = true }
nom-exif = { version = "2.7", optional = true }
symphonia = { version = "0.5", optional = true, features = ["all"] }
moxcms = { version = "0.8", optional = true }

# Tier 3 - feature-gated
gufo-xmp = { version = "0.4", optional = true }
img-parts = { version = "0.4", optional = true }

[features]
default = ["media-metadata"]
media-metadata = []
media-metadata-full = ["matroska", "nom-exif", "symphonia", "moxcms"]
media-metadata-xmp = ["gufo-xmp", "img-parts"]
```

### Extraction Pipeline Architecture

```
Input bytes
  |
  v
[infer] --> MIME type + extension
  |
  +-- image/* --> [imagesize] --> dimensions
  |              [kamadak-exif] --> EXIF (JPEG/TIFF)
  |              [img-parts] --> ICC chunk --> [moxcms] --> color profile
  |              [gufo-xmp] --> XMP
  |
  +-- audio/* --> [lofty] --> tags (ID3/Vorbis/MP4/APE)
  |              [symphonia] --> duration, sample rate, channels, bitrate
  |
  +-- video/* --> [mp4] --> MP4/MOV tracks, duration, codecs
  |              [matroska] --> MKV/WebM tracks, duration, codecs
  |              [nom-exif] --> embedded EXIF, GPS, creation date
  |
  +-- application/pdf --> (future: lopdf for PDF metadata)
  |
  v
MediaMetadata {
    mime_type: String,
    dimensions: Option<(u32, u32)>,
    duration: Option<Duration>,
    exif: Option<ExifData>,
    audio: Option<AudioMeta>,
    video: Option<VideoMeta>,
    color_profile: Option<IccProfile>,
    xmp: Option<XmpData>,
    tags: HashMap<String, String>,
}
```

---

## Key Observations

1. **nom-exif is a hidden gem**: It is the only crate that bridges image and video metadata in one API. Worth watching as it matures.

2. **lofty has won the audio metadata war**: It subsumes id3, mp4ameta, metaflac, and ogg_metadata into one crate with better ergonomics.

3. **symphonia is essential for audio probing**: Tag libraries give you user-facing metadata (title, artist). Symphonia gives you technical metadata (duration, bitrate, sample rate). You need both.

4. **moxcms is the new standard for ICC**: With 10M recent downloads, it has eclipsed lcms2 bindings as the go-to for color management in pure Rust.

5. **All Tier 1 crates are pure Rust**: No C/C++ build dependencies, no system library requirements. This is critical for Nika's cross-platform story.

6. **imagesize has zero dependencies**: It is literally just Rust code that reads headers. The perfect crate.

7. **ffprobe is the escape hatch**: For any format not covered by pure-Rust crates, wrapping the ffprobe CLI gives you complete coverage at the cost of a runtime dependency.

---

## Sources

All data gathered from crates.io API on 2026-03-18. Download counts, versions, and dependency trees are current as of this date.

| Source | What it provided |
|--------|-----------------|
| crates.io/api/v1/crates/{name} | Download counts, versions, descriptions |
| crates.io/api/v1/crates/{name}/{version}/dependencies | Dependency trees, pure-Rust verification |
| GitHub repositories (linked above) | Feature lists, API design, documentation |

## Confidence Level

**High** -- Data comes directly from crates.io (authoritative source for Rust packages). Download counts are objective. Pure-Rust status verified via dependency tree analysis. Feature lists cross-referenced with crate documentation.
