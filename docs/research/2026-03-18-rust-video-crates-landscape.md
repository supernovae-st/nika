# Research Report: Rust Video Processing Crates — Complete Landscape (March 2026)

## Summary

The Rust video ecosystem splits into three tiers: (1) **FFI wrappers** around battle-tested C libraries (ffmpeg-next, gstreamer, dav1d) that offer full-featured video processing; (2) **pure Rust container parsers** (mp4, mp4parse, matroska, symphonia) that can extract metadata without native dependencies; and (3) **pure Rust codecs** (rav1e, openh264) that handle encoding/decoding in safe Rust but lag behind C performance. For Nika's workflow engine, the pragmatic path is pure Rust container parsing for metadata extraction, with optional ffmpeg-next for frame extraction and thumbnail generation behind a feature gate.

## Key Findings

---

### TIER 1: FFmpeg Bindings (The Nuclear Option)

#### ffmpeg-next

| Property | Value |
|----------|-------|
| **Crate** | `ffmpeg-next` |
| **Version** | 8.1.0 |
| **Downloads** | 2,120,287 (1,192,042 recent) |
| **Type** | FFI (wraps libavcodec/libavformat/libavutil/libswscale) |
| **Repo** | https://github.com/zmwangx/rust-ffmpeg |
| **Sys crate** | `ffmpeg-sys-next` v8.1.0 (2,291,875 downloads) |

**Capabilities:**
- Full decode/encode of every major codec (H.264, H.265, VP8/VP9, AV1, ProRes, etc.)
- Container demux/mux (MP4, MKV, WebM, MOV, AVI, FLV, MPEG-TS, etc.)
- Frame extraction, thumbnail generation, transcoding
- Audio extraction, format conversion
- Hardware acceleration (VAAPI, NVENC, VideoToolbox)
- Filters (scale, crop, watermark, color correction)

**Performance:** Near-native C performance. The overhead is a thin FFI layer. This is what YouTube, VLC, and every serious video tool uses under the hood.

**Trade-offs:**
- Requires FFmpeg libraries installed on the system (or built from source via build.rs)
- Build times are heavy (FFmpeg build from source: 5-15 minutes)
- Cross-compilation is painful
- License: depends on FFmpeg build flags (LGPL or GPL)

**Verdict:** The only complete solution. If you need frame extraction or transcoding, nothing else comes close.

#### ac-ffmpeg

| Property | Value |
|----------|-------|
| **Crate** | `ac-ffmpeg` |
| **Version** | 0.19.0 |
| **Downloads** | 49,167 (2,110 recent) |
| **Type** | FFI (alternative FFmpeg wrapper) |
| **Repo** | https://github.com/angelcam/rust-ac-ffmpeg |

**Capabilities:** Simpler API than ffmpeg-next, focused on demuxing/decoding/encoding. Made by Angelcam for their camera platform. Less feature-complete but arguably cleaner API design.

**Verdict:** Niche alternative. ffmpeg-next has 40x the adoption.

#### video-rs

| Property | Value |
|----------|-------|
| **Crate** | `video-rs` |
| **Version** | 0.11.0 |
| **Downloads** | 168,617 (73,126 recent) |
| **Type** | High-level FFI (wraps ffmpeg-next) |
| **Repo** | https://github.com/oddity-ai/video-rs |

**Capabilities:**
- High-level read/write/encode/decode API
- ndarray integration for frame data
- URL streaming support
- Hardware acceleration pass-through

**Verdict:** Best ergonomics for FFmpeg in Rust. Good candidate if you want simple `reader.decode()` style API without touching raw ffmpeg-next. Active development (Feb 2026 update).

---

### TIER 2: GStreamer Bindings (The Pipeline Approach)

#### gstreamer / gstreamer-video

| Property | Value |
|----------|-------|
| **Crate** | `gstreamer` |
| **Version** | 0.25.1 |
| **Downloads** | 7,114,932 (981,893 recent) |
| **Type** | FFI (wraps libgstreamer) |
| **Repo** | https://gitlab.freedesktop.org/gstreamer/gstreamer-rs |

| Related crate | Version | Downloads |
|--------------|---------|-----------|
| `gstreamer-video` | 0.25.0 | 5,103,060 |
| `gstreamer-app` | 0.25.0 | 5,292,221 |
| `gstreamer-base` | 0.25.0 | 6,252,089 |

**Capabilities:**
- Full multimedia pipeline framework
- Element-based architecture: source -> demux -> decode -> filter -> encode -> sink
- Plugin ecosystem for every codec and container
- Hardware acceleration on all platforms
- Live streaming, RTP/RTSP, WebRTC
- Thumbnail extraction via pipeline

**Performance:** Excellent. GStreamer's C core is highly optimized with zero-copy buffer passing.

**Trade-offs:**
- Massive dependency tree (GLib, GObject, dozens of plugins)
- System-level installation required
- Complex API (element/pad/bus paradigm)
- Overkill for simple metadata extraction

**Verdict:** Best for complex pipeline workflows (transcode chains, live streams). Overkill for simple artifact metadata. The Rust bindings are maintained by Sebastian Dröge (core GStreamer developer), so they are first-class.

---

### TIER 3: Pure Rust Container Parsers (Metadata Without Dependencies)

#### mp4 (alfg/mp4-rust)

| Property | Value |
|----------|-------|
| **Crate** | `mp4` |
| **Version** | 0.14.0 |
| **Downloads** | 9,838,226 (837,973 recent) |
| **Type** | Pure Rust |
| **Repo** | https://github.com/alfg/mp4-rust |

**Capabilities:**
- Read and write MP4/ISO-BMFF containers
- Extract: duration, resolution, codec info, bitrate, frame count
- Track metadata (video tracks, audio tracks, subtitle tracks)
- Box/atom-level parsing
- No native dependencies

**Limitations:** Does NOT decode video frames. Container-level only.

**Verdict:** BEST for MP4 metadata extraction. Pure Rust, 10M downloads, battle-tested. This is what you want for `duration`, `resolution`, `codec`, `bitrate` without pulling in FFmpeg.

#### mp4parse (Mozilla)

| Property | Value |
|----------|-------|
| **Crate** | `mp4parse` |
| **Version** | 0.17.0 |
| **Downloads** | 862,791 (206,254 recent) |
| **Type** | Pure Rust |
| **Repo** | https://github.com/nicokimmel/mp4parse-rust (originally Mozilla) |

**Capabilities:**
- ISO base media file format parser
- Used in Firefox for MP4 playback
- Security-focused (fuzz-tested extensively)
- Extracts track info, codec configs, sample tables

**Verdict:** Mozilla-quality parser, security-hardened. More low-level than `mp4` crate. Good for security-critical contexts.

#### re_mp4 (Rerun)

| Property | Value |
|----------|-------|
| **Crate** | `re_mp4` |
| **Version** | 0.4.0 |
| **Downloads** | 1,013,105 (360,493 recent) |
| **Type** | Pure Rust |
| **Repo** | https://github.com/rerun-io/re_mp4 |

**Capabilities:** MP4 parser from the Rerun visualization team. Simpler API, focused on reading. Newer crate with fast-growing adoption.

#### matroska

| Property | Value |
|----------|-------|
| **Crate** | `matroska` |
| **Version** | 0.30.0 |
| **Downloads** | 144,516 (13,983 recent) |
| **Type** | Pure Rust |
| **Repo** | https://github.com/tuffy/matroska |

**Capabilities:**
- Parse Matroska (MKV) container metadata
- Extract track info, duration, codec IDs
- Chapter markers, tags, attachments

#### webm-iterable

| Property | Value |
|----------|-------|
| **Crate** | `webm-iterable` |
| **Version** | 0.6.4 |
| **Downloads** | 218,856 (40,450 recent) |
| **Type** | Pure Rust |
| **Repo** | https://github.com/austinleroy/webm-iterable |

**Capabilities:** Iterator-based WebM/Matroska parser built on EBML. Good for streaming parse of WebM containers.

#### symphonia (+ symphonia-format-mkv)

| Property | Value |
|----------|-------|
| **Crate** | `symphonia` |
| **Version** | 0.5.5 |
| **Downloads** | 4,912,472 (1,413,173 recent) |
| **Type** | Pure Rust |
| **Repo** | https://github.com/pdeljanov/Symphonia |

| Sub-crate | Downloads |
|-----------|-----------|
| `symphonia-format-mkv` | 2,413,453 |
| `symphonia-bundle-flac` | 3,343,253 |

**Capabilities:**
- Pure Rust media container demuxing + audio decoding
- Supports: MP4/M4A, MKV/WebM, OGG, WAV, FLAC, MP3, AAC, Vorbis, ALAC
- Gapless playback, seeking, metadata extraction
- Container probing (auto-detect format)
- NO video decoding (audio-only codecs)

**Verdict:** The best pure-Rust option for container demuxing + audio. Its MKV parser handles both MKV and WebM. Does not decode video frames but extracts video track metadata (codec, resolution, duration).

#### mpeg2ts-reader

| Property | Value |
|----------|-------|
| **Crate** | `mpeg2ts-reader` |
| **Version** | 0.18.2 |
| **Downloads** | 87,105 (19,326 recent) |
| **Type** | Pure Rust |

**Capabilities:** MPEG Transport Stream demuxing. Useful for broadcast/HLS segment parsing.

---

### TIER 4: Pure Rust Codecs (Encode/Decode Without C)

#### rav1e (AV1 Encoder)

| Property | Value |
|----------|-------|
| **Crate** | `rav1e` |
| **Version** | 0.8.1 |
| **Downloads** | 21,049,163 (5,839,451 recent) |
| **Type** | Pure Rust + assembly (NASM) |
| **Repo** | https://github.com/xiph/rav1e |
| **Org** | Xiph.org (same org as Opus, Vorbis, FLAC) |

**Capabilities:**
- AV1 video encoding
- Speed presets 0-10
- 8-bit and 10-bit encoding
- Tile-based parallelism
- Used massively via `ravif` for AVIF image encoding (21M downloads)

**Performance:**
- Pure Rust with hand-tuned x86 assembly (NASM)
- Competitive with libaom at speed >= 6
- Slower than SVT-AV1 at equivalent quality
- ARM NEON support

**Verdict:** The flagship pure-Rust video codec. Primarily used for AVIF images today, but fully capable of video encoding. Very high quality project from Xiph.

#### dav1d (AV1 Decoder — FFI)

| Property | Value |
|----------|-------|
| **Crate** | `dav1d` |
| **Version** | 0.11.1 |
| **Downloads** | 748,643 (118,978 recent) |
| **Type** | FFI (wraps libdav1d, written in C + assembly) |
| **Repo** | https://github.com/rust-av/dav1d-rs |

**Capabilities:** The fastest AV1 decoder in existence. Written by VideoLAN (VLC). Decodes AV1 frames to raw pixels.

**Performance:** Fastest AV1 decoder, period. Heavily SIMD-optimized.

**Verdict:** If you need to decode AV1, this is it. No pure-Rust alternative exists that is remotely competitive.

#### aom-decode

| Property | Value |
|----------|-------|
| **Crate** | `aom-decode` |
| **Version** | 0.2.13 |
| **Downloads** | 74,449 (16,630 recent) |
| **Type** | FFI (wraps libaom) |

**Capabilities:** AV1 decoder using the reference libaom. Slower than dav1d but sometimes used as fallback.

#### openh264

| Property | Value |
|----------|-------|
| **Crate** | `openh264` |
| **Version** | 0.9.3 |
| **Downloads** | 261,971 (51,067 recent) |
| **Type** | FFI (wraps Cisco's OpenH264, auto-downloads binary) |
| **Repo** | https://github.com/ralfbiedert/openh264-rs |

**Capabilities:**
- H.264 Baseline Profile encoding and decoding
- Auto-downloads pre-built OpenH264 binary (no build dependency!)
- YUV -> RGB conversion
- BSD-licensed (Cisco pays MPEG-LA royalties)

**Limitations:** Baseline profile only (no High profile). Not suitable for high-quality encoding. Decoding is limited.

**Verdict:** Great for WebRTC-style H.264 needs. The auto-download of binaries is clever. Not for production video processing.

#### h264-reader

| Property | Value |
|----------|-------|
| **Crate** | `h264-reader` |
| **Version** | 0.8.0 |
| **Downloads** | 586,542 (256,149 recent) |
| **Type** | Pure Rust |
| **Repo** | https://github.com/dholroyd/h264-reader |

**Capabilities:**
- Parse H.264 NAL units, SPS, PPS
- Extract resolution, profile, level from bitstream
- Does NOT decode frames to pixels
- Useful for bitstream analysis and metadata extraction

**Verdict:** Excellent for extracting H.264 codec parameters from raw bitstream without decoding. Pure Rust, no dependencies.

#### x264

| Property | Value |
|----------|-------|
| **Crate** | `x264` |
| **Version** | 0.5.0 |
| **Downloads** | 52,360 (33,057 recent) |
| **Type** | FFI (wraps libx264) |

**Capabilities:** H.264 encoding via the industry-standard libx264. GPL-licensed.

---

### TIER 5: Utility & Support Crates

#### v_frame

| Property | Value |
|----------|-------|
| **Crate** | `v_frame` |
| **Version** | 0.5.1 |
| **Downloads** | 21,380,738 (5,785,343 recent) |
| **Type** | Pure Rust |

**Capabilities:** Video frame data structures. The pixel buffer type used by rav1e and the rust-av ecosystem. Think of it as `image::DynamicImage` but for video planes (Y, U, V).

#### y4m

| Property | Value |
|----------|-------|
| **Crate** | `y4m` |
| **Version** | 0.8.0 |
| **Downloads** | 6,175,590 (4,465,947 recent) |
| **Type** | Pure Rust |

**Capabilities:** YUV4MPEG2 format encoder/decoder. The standard interchange format for raw video between tools (pipe ffmpeg -> rav1e).

#### av-scenechange

| Property | Value |
|----------|-------|
| **Crate** | `av-scenechange` |
| **Version** | 0.23.0 |
| **Downloads** | 5,289,133 (4,486,546 recent) |
| **Type** | Pure Rust |

**Capabilities:** Detect scene changes in video. Used to find optimal keyframe placement for encoding.

#### av-data

| Property | Value |
|----------|-------|
| **Crate** | `av-data` |
| **Version** | 0.4.4 |
| **Downloads** | 1,198,806 (350,065 recent) |
| **Type** | Pure Rust |

**Capabilities:** Common multimedia data structures (pixel formats, time bases, rational numbers). The foundation types for the rust-av ecosystem.

#### dcv-color-primitives (Amazon)

| Property | Value |
|----------|-------|
| **Crate** | `dcv-color-primitives` |
| **Version** | 1.0.0 |
| **Downloads** | 602,328 (35,527 recent) |
| **Type** | Pure Rust + SIMD |
| **Org** | Amazon (AWS DCV team) |

**Capabilities:** SIMD-optimized colorspace conversion (NV12/I420/I444 <-> ARGB/BGRA). Exactly what you need after decoding video to convert YUV planes to RGB for thumbnails.

#### imgref

| Property | Value |
|----------|-------|
| **Crate** | `imgref` |
| **Version** | 1.12.0 |
| **Downloads** | 25,725,065 (5,988,689 recent) |
| **Type** | Pure Rust |

**Capabilities:** Safe 2D pixel buffer type with width/height/stride. The bridge between video frame planes and image processing crates.

#### nom-exif

| Property | Value |
|----------|-------|
| **Crate** | `nom-exif` |
| **Version** | 2.7.0 |
| **Downloads** | 103,758 (23,104 recent) |
| **Type** | Pure Rust |
| **Repo** | https://github.com/mindeng/nom-exif |

**Capabilities:**
- Parse EXIF/metadata from images AND video files
- Supports: JPEG, HEIF, TIFF, MOV, MP4, 3GP, WebM, MKV
- Extract: creation date, GPS, camera info, duration, resolution
- nom-based parser (streaming-friendly)

**Verdict:** The most comprehensive pure-Rust metadata extractor that handles both images and video. Actively maintained (Feb 2026).

#### mp4ameta

| Property | Value |
|----------|-------|
| **Crate** | `mp4ameta` |
| **Version** | 0.13.0 |
| **Downloads** | 730,553 (65,392 recent) |
| **Type** | Pure Rust |

**Capabilities:** iTunes-style MPEG-4 audio metadata (tags, artwork). Focused on audio metadata in M4A/MP4 containers.

#### dash-mpd

| Property | Value |
|----------|-------|
| **Crate** | `dash-mpd` |
| **Version** | 0.20.2 |
| **Downloads** | 328,830 (37,608 recent) |
| **Type** | Pure Rust |

**Capabilities:** Parse and download MPEG-DASH / WebM-DASH streaming manifests.

---

### TIER 6: AV1/AVIF Ecosystem (Massive Downloads)

These form a cohesive ecosystem primarily driven by AVIF image encoding:

| Crate | Downloads | Role |
|-------|-----------|------|
| `imgref` | 25.7M | Pixel buffer |
| `avif-serialize` | 21.5M | AVIF container writer |
| `av1-grain` | 21.5M | Film grain synthesis |
| `v_frame` | 21.4M | Video frame types |
| `ravif` | 21.2M | AVIF encoder (uses rav1e) |
| `rav1e` | 21.0M | AV1 encoder core |
| `y4m` | 6.2M | Raw video interchange |
| `av-scenechange` | 5.3M | Scene detection |

This is the most active pure-Rust multimedia ecosystem. The download counts are driven by the `image` crate's AVIF support, which transitively pulls in rav1e.

---

## Comparison Matrix

### For Nika Workflow Engine: What Do You Actually Need?

| Need | Best Pure Rust | Best FFI | Recommendation |
|------|---------------|----------|----------------|
| MP4 duration/resolution | `mp4` | - | `mp4` (pure Rust, 10M dl) |
| MKV/WebM metadata | `matroska` or `symphonia` | - | `symphonia` (unified API) |
| Video thumbnail | None | `ffmpeg-next` or `video-rs` | `video-rs` (high-level API) |
| H.264 codec info | `h264-reader` | - | `h264-reader` (pure Rust) |
| AV1 encoding | `rav1e` | - | `rav1e` (pure Rust, production-ready) |
| AV1 decoding | None competitive | `dav1d` | `dav1d` (fastest decoder) |
| Full transcoding | None | `ffmpeg-next` | `ffmpeg-next` (only option) |
| Colorspace conversion | `dcv-color-primitives` | - | `dcv-color-primitives` (SIMD) |
| Multi-format metadata | `nom-exif` | - | `nom-exif` (images + video) |
| EXIF/GPS from video | `nom-exif` | - | `nom-exif` |
| Complex pipelines | None | `gstreamer` | Only if needed |

---

## Recommended Architecture for Nika

### Tier A: Always-on (pure Rust, no feature gate)

```toml
[dependencies]
mp4 = "0.14"              # MP4 container parsing (duration, resolution, codec, tracks)
nom-exif = "2.7"          # Metadata from video + image files
h264-reader = "0.8"       # H.264 bitstream parameter extraction
symphonia = { version = "0.5", features = ["mkv", "mp4"] }  # MKV/WebM demuxing
```

**What this gives you:** Extract duration, resolution, codec, bitrate, frame count, creation date, GPS coordinates from MP4/MKV/WebM files. Zero native dependencies. Compiles everywhere.

### Tier B: Feature-gated (requires system FFmpeg)

```toml
[dependencies]
video-rs = { version = "0.11", optional = true }  # High-level ffmpeg wrapper

[features]
video-decode = ["video-rs"]
```

**What this gives you:** Frame extraction, thumbnail generation, transcoding. Requires FFmpeg on the system.

### Tier C: Future/Encoding (feature-gated)

```toml
[dependencies]
rav1e = { version = "0.8", optional = true }
dcv-color-primitives = { version = "1.0", optional = true }

[features]
video-encode = ["rav1e", "dcv-color-primitives"]
```

---

## The Hard Truth About Video in Rust (March 2026)

1. **No pure-Rust H.264/H.265 decoder exists.** This is the single biggest gap. H.264-reader parses the bitstream syntax but does NOT decode frames to pixels. To get actual frames, you need FFmpeg, GStreamer, or OpenH264 (Baseline only).

2. **rav1e is production-quality** for AV1 encoding but slower than SVT-AV1 (C). The gap narrows at higher speed presets.

3. **Container parsing is excellent in pure Rust.** The `mp4`, `matroska`, `symphonia`, and `nom-exif` crates can extract every piece of metadata you need without native deps.

4. **The rust-av organization** (https://github.com/rust-av) is building toward a full pure-Rust multimedia stack, but video decoding remains the missing piece.

5. **For thumbnails, you need FFI.** There is no way around this. `video-rs` (wrapping ffmpeg-next) is the cleanest path.

6. **GStreamer is overkill** unless you are building a media player or streaming server. For a workflow engine that processes video artifacts, ffmpeg-next/video-rs is more appropriate.

---

## Sources

All data pulled from crates.io API on 2026-03-18.

| Source | What it provided |
|--------|-----------------|
| https://crates.io/api/v1/crates?q=video+processing | Video processing crate discovery |
| https://crates.io/api/v1/crates?q=ffmpeg | FFmpeg ecosystem mapping |
| https://crates.io/api/v1/crates?q=gstreamer | GStreamer ecosystem mapping |
| https://crates.io/api/v1/crates?q=rav1e+av1 | AV1 codec ecosystem |
| https://crates.io/api/v1/crates?q=mp4+parse | Container parser discovery |
| https://crates.io/api/v1/crates?q=matroska+webm | MKV/WebM ecosystem |
| https://crates.io/api/v1/crates?q=video+decode | Video decoder discovery |
| Individual crate pages | Version, download count, description |

## Methodology

- Tools used: crates.io REST API (direct queries)
- Crates analyzed: 45+
- Data source: crates.io download statistics and metadata
- Period: All-time downloads + 90-day recent downloads

## Confidence Level

**High** — Data comes directly from crates.io API (authoritative source for Rust crate statistics). Capability assessments are based on crate descriptions, README documentation, and known ecosystem relationships. Performance claims are based on published benchmarks and known architecture (pure Rust vs FFI).

## Further Research Suggestions

- Benchmark `mp4` vs `mp4parse` vs `re_mp4` for metadata extraction speed on large files
- Test `video-rs` 0.11 thumbnail extraction latency for Nika artifact processing
- Evaluate `symphonia` as a unified container probe (it can identify format + extract metadata for MP4, MKV, WebM, OGG in one pass)
- Investigate `nom-exif` for extracting camera metadata from MOV files (iPhone recordings)
- Profile `rav1e` at speed=10 for quick AV1 thumbnail encoding vs JPEG/WebP
