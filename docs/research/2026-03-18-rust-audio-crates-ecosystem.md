# Research Report: Rust Audio Processing Ecosystem

> Date: 2026-03-18 | Context: Nika media pipeline — audio artifact support

## Summary

The Rust audio ecosystem is mature for **decoding** but thin for **encoding**. **Symphonia** is the undisputed pure-Rust audio decoder (4.9M downloads, 14 codecs, +/-15% of FFmpeg performance). For metadata, **lofty** has emerged as the modern replacement for the older `id3` crate. For Nika's workflow engine, a minimal stack of `symphonia` + `lofty` + `hound` covers all realistic audio artifact needs without any C dependencies.

## Key Findings

### 1. DECODING: Symphonia is the clear winner

| Field | Value |
|-------|-------|
| Crate | `symphonia` |
| Version | 0.5.5 |
| Downloads | 4,912,472 total / 1,413,173 recent |
| License | MPL-2.0 |
| Pure Rust | Yes, 100% safe Rust |
| MSRV | 1.53+ |
| Source | https://github.com/pdeljanov/Symphonia |

**Supported formats (demuxers):**

| Format | Status | Gapless | Feature Flag |
|--------|--------|---------|--------------|
| WAV | Excellent | Yes | `wav` (default) |
| OGG | Great | Yes | `ogg` (default) |
| MKV/WebM | Good | No | `mkv` (default) |
| AIFF | Great | Yes | `aiff` |
| CAF | Good | No | `caf` |
| ISO/MP4 | Great | No | `isomp4` |

**Supported codecs (decoders):**

| Codec | Status | Gapless | Feature Flag |
|-------|--------|---------|--------------|
| PCM | Excellent | Yes | `pcm` (default) |
| FLAC | Excellent | Yes | `flac` (default) |
| Vorbis | Excellent | Yes | `vorbis` (default) |
| MP3 | Excellent | Yes | `mp3` |
| AAC-LC | Great | No | `aac` |
| ALAC | Great | Yes | `alac` |
| ADPCM | Good | Yes | `adpcm` (default) |
| MP1/MP2 | Great | No | `mpa` |
| Opus | In progress | - | `opus` |
| WavPack | In progress | - | `wavpack` |

**Performance:** +/-15% of FFmpeg. SIMD optimizations available (SSE, AVX, Neon) via feature flags.

**Why Symphonia wins:**
- Pure Rust, no C FFI, no system dependencies
- Unified API for all formats (probe -> demux -> decode)
- Built-in metadata reading (ID3v1, ID3v2, Vorbis comments, RIFF, MP4)
- Fuzz-tested, DoS-resistant
- Active development (updated Oct 2025)

**Usage for Nika:**
```rust
// Enable all common formats
symphonia = { version = "0.5", features = ["all"] }

// Or be selective (smaller binary)
symphonia = { version = "0.5", features = ["mp3", "flac", "aac", "ogg", "wav", "isomp4"] }
```

### 2. METADATA: lofty (modern) vs id3 (legacy)

#### lofty — Recommended

| Field | Value |
|-------|-------|
| Crate | `lofty` |
| Version | 0.23.3 |
| Downloads | 489,497 total / 87,085 recent |
| Pure Rust | Yes |
| Source | https://github.com/Serial-ATA/lofty-rs |

Supports ALL major tag formats:
- ID3v1, ID3v2 (MP3, AIFF, WAV)
- Vorbis Comments (FLAC, OGG, Opus, Speex)
- MP4/iTunes atoms (M4A, AAC)
- APE tags (APE, MP3)
- RIFF INFO (WAV)
- WavPack tags

Key advantage: **Unified API across all tag formats.** Read/write with one interface.

#### id3 — Legacy but very popular

| Field | Value |
|-------|-------|
| Crate | `id3` |
| Version | 1.16.4 |
| Downloads | 10,038,356 total / 420,540 recent |
| Pure Rust | Yes |

Only handles ID3v1/ID3v2 (MP3 only). Still heavily used but `lofty` is strictly superior for multi-format support.

#### Other metadata crates

| Crate | Downloads | Purpose |
|-------|-----------|---------|
| `mp4ameta` | 730K | MP4/M4A metadata specifically |
| `metaflac` | 194K | FLAC metadata specifically |
| `audiotags` | 110K | Unified wrapper (but unmaintained) |
| `ogg-metadata` | 54K | OGG metadata specifically |

**Recommendation for Nika:** Use `lofty` for metadata extraction. It covers all formats Symphonia decodes. Alternatively, Symphonia itself reads basic metadata (ID3, Vorbis comments) during probing -- may be sufficient if you only need title/artist/duration.

### 3. WAV READ/WRITE: hound

| Field | Value |
|-------|-------|
| Crate | `hound` |
| Version | 3.5.1 |
| Downloads | 10,128,838 total / 2,189,503 recent |
| Pure Rust | Yes |
| Source | https://github.com/ruuda/hound |

Simple, reliable WAV reader/writer. Used by `rodio` internally. If Nika needs to write intermediate WAV artifacts (e.g., decoded audio for downstream processing), `hound` is the standard.

### 4. PLAYBACK: rodio and cpal (less relevant for Nika)

#### cpal — Low-level audio I/O

| Field | Value |
|-------|-------|
| Crate | `cpal` |
| Version | 0.17.3 |
| Downloads | 11,332,131 total / 2,083,633 recent |
| Pure Rust | No (platform audio backends: ALSA, CoreAudio, WASAPI) |

Cross-platform audio device abstraction. Handles input/output streams. Foundation for `rodio`.

#### rodio — High-level playback

| Field | Value |
|-------|-------|
| Crate | `rodio` |
| Version | 0.22.2 |
| Downloads | 6,872,246 total / 1,223,400 recent |
| Pure Rust | Partially (uses cpal for device I/O, symphonia for decoding) |

Audio playback library with mixing, volume control, spatial audio. Built on cpal + symphonia.

**Relevance to Nika:** Low. Nika is a workflow engine, not a player. Unless you plan audio preview in the TUI, skip these.

### 5. DSP AND RESAMPLING

#### rubato — Audio resampling

| Field | Value |
|-------|-------|
| Crate | `rubato` |
| Version | 1.0.1 |
| Downloads | 4,407,414 total / 1,398,478 recent |
| Pure Rust | Yes |

High-quality async resampler. Essential if Nika needs to normalize sample rates (e.g., 44.1kHz to 48kHz) during audio processing steps. Reached 1.0 -- API is stable.

#### dasp — Digital Audio Signal Processing

| Field | Value |
|-------|-------|
| Crate | `dasp` |
| Version | 0.11.0 |
| Downloads | 3,128,785 total / 584,609 recent |
| Pure Rust | Yes |

Provides audio sample types, conversions, ring buffers, interpolation. Foundational primitives. Previously named `sample`.

#### rustfft / realfft — FFT for spectrum analysis

| Crate | Version | Downloads |
|-------|---------|-----------|
| `rustfft` | 6.4.1 | 15,272,990 / 3,097,808 recent |
| `realfft` | 3.5.0 | 8,588,597 / 2,286,618 recent |

Pure Rust FFT. Required for waveform/spectrum generation from raw audio.

#### ebur128 — Loudness measurement

| Field | Value |
|-------|-------|
| Crate | `ebur128` |
| Version | 0.1.10 |
| Downloads | 583,642 total / 64,302 recent |

EBU R128 loudness standard implementation. Useful for audio normalization workflows.

### 6. WAVEFORM/VISUALIZATION

This is the weakest area in the Rust ecosystem. No dominant crate exists.

| Crate | Downloads | Description |
|-------|-----------|-------------|
| `spectrum-analyzer` | 115,844 | no_std FFT-based spectrum analysis |
| `sonogram` | 29,676 | Spectrogram image generation |
| `audio-visualizer` | 28,438 | Simple waveform/spectrum PNG generation |

**Practical approach for Nika:** Roll your own waveform thumbnail:
1. Decode with `symphonia`
2. Downsample to ~1000 points (peak envelope)
3. Render with a tiny image crate or output as JSON array for frontend rendering

This is ~50 lines of code and avoids pulling in a niche dependency.

### 7. ENCODING (the weak spot)

Pure Rust encoding barely exists. Most encoding crates are FFI wrappers around C libraries.

| Crate | Type | Downloads | Notes |
|-------|------|-----------|-------|
| `hound` | Pure Rust | 10.1M | WAV only (but WAV is lossless, good intermediate) |
| `flacenc` | Pure Rust | 449K | FLAC encoding, pure Rust |
| `mp3lame-encoder` | FFI (libmp3lame) | 580K | MP3 encoding, requires C lib |
| `vorbis-encoder` | FFI (libvorbis) | 45K | OGG Vorbis encoding |
| `opus` | FFI (libopus) | 901K | Opus encode/decode |
| `ogg-opus` | FFI | 11K | Ogg Opus encode/decode |

**Key insight:** If Nika needs to *produce* compressed audio, the options are:
- WAV/FLAC: Pure Rust (hound / flacenc)
- MP3/AAC/Opus: Requires C FFI or shelling out to `ffmpeg`

### 8. GAME AUDIO ENGINES (context only)

| Crate | Downloads | Purpose |
|-------|-----------|---------|
| `kira` | 666K | Game audio with tweening, modular routing |
| `fundsp` | 138K | Audio synthesis DSP graph |
| `oddio` | 48K | Lightweight spatial audio |

Not relevant for Nika but shows ecosystem maturity.

## Recommended Stack for Nika

### Tier 1: Essential (add now)

```toml
[dependencies]
# Decode any audio format to PCM samples
symphonia = { version = "0.5", features = ["aac", "alac", "flac", "mp3", "pcm", "vorbis", "isomp4", "ogg", "wav", "mkv", "caf", "aiff"] }

# Rich metadata extraction (ID3, Vorbis comments, MP4 atoms, etc.)
lofty = "0.23"
```

**Why these two:**
- Symphonia decodes everything, lofty reads/writes all tag formats
- Both are pure Rust, no C dependencies
- Together they cover: MP3, FLAC, WAV, OGG/Vorbis, AAC/M4A, ALAC, AIFF, WebM, MKV, CAF
- Symphonia also reads metadata during probe, but lofty gives you write access and richer extraction

### Tier 2: Useful (add when needed)

```toml
# WAV writing (for intermediate artifacts / decoded output)
hound = "3.5"

# Sample rate conversion
rubato = "1.0"

# Audio fundamentals (sample types, conversions)
dasp = "0.11"
```

### Tier 3: Specialized (add if scope expands)

```toml
# FFT for waveform/spectrum thumbnails
rustfft = "6.4"

# Loudness measurement (EBU R128)
ebur128 = "0.1"

# FLAC encoding (pure Rust)
flacenc = "0.5"
```

### What NOT to use

| Avoid | Why |
|-------|-----|
| `lewton` | Superseded by symphonia-codec-vorbis |
| `claxon` | Superseded by symphonia-bundle-flac |
| `minimp3` | C FFI, superseded by symphonia-bundle-mp3 |
| `audiotags` | Unmaintained, use `lofty` instead |
| `wav` | Deprecated, use `hound` |
| `id3` | MP3-only, use `lofty` for multi-format |
| `rodio` / `cpal` | Playback-focused, Nika doesn't need audio output |

## Format Coverage Matrix (Symphonia + Lofty)

| Format | Extension | Decode | Metadata Read | Metadata Write | Encode |
|--------|-----------|--------|---------------|----------------|--------|
| MP3 | .mp3 | Symphonia (Excellent) | Lofty (ID3v1/v2) | Lofty | mp3lame-encoder (FFI) |
| FLAC | .flac | Symphonia (Excellent) | Lofty (Vorbis) | Lofty | flacenc (pure Rust) |
| WAV | .wav | Symphonia (Excellent) | Lofty (RIFF) | Lofty | hound (pure Rust) |
| OGG Vorbis | .ogg | Symphonia (Excellent) | Lofty (Vorbis) | Lofty | vorbis-encoder (FFI) |
| AAC/M4A | .m4a, .aac | Symphonia (Great) | Lofty (MP4 atoms) | Lofty | None pure Rust |
| ALAC | .m4a | Symphonia (Great) | Lofty (MP4 atoms) | Lofty | None |
| AIFF | .aiff | Symphonia (Great) | Lofty (ID3v2) | Lofty | None |
| Opus | .opus | Not yet (in progress) | Lofty (Vorbis) | Lofty | opus (FFI) |
| WavPack | .wv | Not yet (in progress) | Lofty (APE) | Lofty | None |
| WebM | .webm | Symphonia (Good) | Symphonia only | No | None |
| CAF | .caf | Symphonia (Good) | Symphonia only | No | None |

## Sources

1. [Symphonia GitHub](https://github.com/pdeljanov/Symphonia) -- README with full format matrix and benchmarks
2. [crates.io/crates/symphonia](https://crates.io/crates/symphonia) -- v0.5.5, 4.9M downloads
3. [crates.io/crates/lofty](https://crates.io/crates/lofty) -- v0.23.3, 489K downloads
4. [crates.io/crates/rodio](https://crates.io/crates/rodio) -- v0.22.2, 6.9M downloads
5. [crates.io/crates/cpal](https://crates.io/crates/cpal) -- v0.17.3, 11.3M downloads
6. [crates.io/crates/hound](https://crates.io/crates/hound) -- v3.5.1, 10.1M downloads
7. [crates.io/crates/rubato](https://crates.io/crates/rubato) -- v1.0.1, 4.4M downloads
8. [crates.io/crates/dasp](https://crates.io/crates/dasp) -- v0.11.0, 3.1M downloads
9. [crates.io/crates/rustfft](https://crates.io/crates/rustfft) -- v6.4.1, 15.3M downloads
10. [crates.io/crates/id3](https://crates.io/crates/id3) -- v1.16.4, 10.0M downloads
11. [crates.io/crates/ebur128](https://crates.io/crates/ebur128) -- v0.1.10, 584K downloads
12. [crates.io/crates/flacenc](https://crates.io/crates/flacenc) -- v0.5.1, 449K downloads

## Methodology

- All download counts and versions fetched live from crates.io API on 2026-03-18
- Symphonia format support matrix from official GitHub README
- Recommendations based on: pure Rust preference, download momentum, maintenance status, API quality

## Confidence Level

**High** -- The Rust audio ecosystem is well-established. Symphonia is the consensus choice for decoding (used by rodio, Spotify's librespot, and many other projects). Lofty is the actively maintained metadata library. These recommendations are unlikely to change significantly in the near term.

## Gaps and Risks

1. **Opus decoding** -- Symphonia's Opus codec is still "in progress". If Opus is critical, you may need the `opus` crate (FFI to libopus) as a fallback.
2. **No pure-Rust MP3/AAC encoding** -- If you need to produce compressed audio without C deps, your only option is FLAC (via `flacenc`) or WAV (via `hound`).
3. **Waveform visualization** -- No mature crate. Plan to write ~50 LOC custom peak-envelope extractor.
4. **Symphonia is 0.x** -- API may have breaking changes, though it has been stable for a while.
