// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:video_compose` — the contract the check and the run both read
//! (stdlib §`nika:video_compose` · §Media graduate #6).
//!
//! The static checker judges every LITERAL argument with these rules before
//! anything runs; the builtin parses the RESOLVED arguments against the same
//! bounds, closed sets and predicates at run. One table, two readers, so a
//! value the check accepts is a value the run accepts (check ≡ run).
//!
//! The call reads every media reference — each `segments[k].image`, each
//! `segments[k].audio` and the top-level `audio`, a path string or an object
//! carrying both `path` and `sha256` — and writes `out`
//! ([`crate::secondary_read_slots`] lists the reads, [`crate::builtin_effect`]
//! the write). It also names the host program the call starts, cfg-free and
//! without probing the host: the check says what the run starts, and a
//! host-dependent finding would break check ≡ run and the wasm checker.

use std::ops::RangeInclusive;

use serde_json::Value;

/// The builtin's id.
pub const TOOL: &str = "nika:video_compose";

/// The closed argument set (the catalog row carries the same list).
pub const ARGS: [&str; 12] = [
    "segments",
    "audio",
    "audio_codec",
    "out",
    "size",
    "fps",
    "scale",
    "background",
    "quality",
    "transition",
    "metadata",
    "timeout_ms",
];

/// The keys of one segment, a closed map.
pub const SEGMENT_KEYS: [&str; 4] = ["image", "audio", "duration_ms", "pad_ms"];

/// The most segments one call takes. A literal list over it is refused at
/// check; a resolved one is the segment budget at run (`-005`).
pub const MAX_SEGMENTS: usize = 200;

/// `segments[].duration_ms`, in milliseconds.
pub const DURATION_MS: RangeInclusive<u64> = 100..=600_000;

/// `segments[].pad_ms`, in milliseconds.
pub const PAD_MS: RangeInclusive<u64> = 0..=10_000;

/// The `size` width, in pixels (even).
pub const WIDTH: RangeInclusive<u32> = 64..=3840;

/// The `size` height, in pixels (even).
pub const HEIGHT: RangeInclusive<u32> = 64..=2160;

/// The default `size`, `1280x720`.
pub const DEFAULT_SIZE: (u32, u32) = (1280, 720);

/// `fps`, frames per second.
pub const FPS: RangeInclusive<u64> = 1..=60;

/// The default `fps`.
pub const DEFAULT_FPS: u32 = 25;

/// `timeout_ms`, in milliseconds.
pub const TIMEOUT_MS: RangeInclusive<u64> = 1_000..=3_600_000;

/// The default `timeout_ms`.
pub const DEFAULT_TIMEOUT_MS: u64 = 600_000;

/// The default `background`, black (the letterbox convention).
pub const DEFAULT_BACKGROUND: [u8; 3] = [0, 0, 0];

/// `metadata`'s cap, in bytes of canonical JSON ([`metadata_len`]).
pub const METADATA_MAX_BYTES: usize = 16_384;

/// The one reserved `transition` word: refused loudly, never guessed.
pub const RESERVED_TRANSITION: &str = "fade";

/// The host program the call starts itself — named here, never probed.
pub const FFMPEG: &str = "ffmpeg";

/// The engine configuration that names the program (an absolute path, read
/// at the composition root, never from the workflow · it never falls through
/// to [`FFMPEG_CANDIDATES`] when set).
pub const FFMPEG_ENV: &str = "NIKA_FFMPEG";

/// The fixed places the program is looked for, in order, when
/// [`FFMPEG_ENV`] is unset. `PATH` is never consulted.
pub const FFMPEG_CANDIDATES: [&str; 3] = [
    "/opt/homebrew/bin/ffmpeg",
    "/usr/local/bin/ffmpeg",
    "/usr/bin/ffmpeg",
];

/// The version floor, as an operator reads it.
pub const FFMPEG_FLOOR: &str = "6.0";

/// The floor as the capability probe reads it: the libavformat and
/// libavcodec majors.
pub const FFMPEG_LIB_FLOOR: u32 = 60;

/// The H.264 encoder the floor requires.
pub const H264_ENCODER: &str = "libx264";

/// How many words a closed set holds (its `ALL` array's length).
macro_rules! count_words {
    () => { 0 };
    ($head:ident $($rest:ident)*) => { 1 + count_words!($($rest)*) };
}

macro_rules! closed_words {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $word:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vdoc])* $variant,)+
        }

        impl $name {
            /// Every choice, in the contract's order.
            pub const ALL: [Self; count_words!($($variant)+)] = [$(Self::$variant),+];

            /// The exact, case-sensitive argument spelling.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $word,)+
                }
            }

            /// Parse an explicit choice; omission is the caller's default.
            #[must_use]
            pub fn parse(value: &str) -> Option<Self> {
                Self::ALL.into_iter().find(|choice| choice.as_str() == value)
            }
        }
    };
}

closed_words! {
    /// How a still lands in the frame (`scale`, default [`Scale::Fit`]).
    Scale {
        /// The whole still, letterboxed on `background` (the default).
        Fit => "fit",
        /// Covers the frame, center-cropped.
        Fill => "fill",
        /// Native size, centered or center-cropped.
        None => "none",
    }
}

closed_words! {
    /// The ordered quality ladder each backend maps (`quality`, default
    /// [`Quality::Standard`]).
    Quality {
        /// The smallest file.
        Draft => "draft",
        /// The default.
        Standard => "standard",
        /// The best picture and sound.
        High => "high",
    }
}

closed_words! {
    /// The audio track's codec (`audio_codec`, default [`AudioCodec::Aac`]).
    AudioCodec {
        /// AAC-LC (the default).
        Aac => "aac",
        /// FLAC.
        Flac => "flac",
    }
}

closed_words! {
    /// How one still gives way to the next (`transition`). `fade` is
    /// reserved ([`RESERVED_TRANSITION`]).
    Transition {
        /// A cut, the only transition in this version.
        Cut => "cut",
    }
}

/// `(width, height)` when the text is the `WIDTHxHEIGHT` grammar with both sides even and
/// inside [`WIDTH`] and [`HEIGHT`]; `None` otherwise.
#[must_use]
pub fn size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once('x')?;
    let side = |digits: &str| -> Option<u32> {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    };
    let (w, h) = (side(w)?, side(h)?);
    (w % 2 == 0 && h % 2 == 0 && WIDTH.contains(&w) && HEIGHT.contains(&h)).then_some((w, h))
}

/// Exactly 64 lowercase hex digits, the form every Nika producer writes.
#[must_use]
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether an `out` path names an MP4 artifact: it ends in `.mp4`, exactly.
#[must_use]
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the contract's own test: the name ends in `.mp4`, case-sensitively, as the spec \
              oracle judges it (`.MP4` is refused at check and at run alike)"
)]
pub fn out_is_mp4(path: &str) -> bool {
    path.ends_with(".mp4")
}

/// The size of `value` as canonical JSON, in bytes: sorted keys, no
/// whitespace, UTF-8 (the measure `metadata`'s cap is judged in).
#[must_use]
pub fn metadata_len(value: &Value) -> usize {
    /// Counts what serialization writes; it never stores it.
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    // A `Value` always serializes; a failure would be counted as over any cap.
    match serde_json::to_writer(&mut count, value) {
        Ok(()) => count.0,
        Err(_) => usize::MAX,
    }
}

/// The value an absent key reads as.
static NULL: Value = Value::Null;

/// The path a media reference names, as written: the string itself, an
/// object's `path` (null when it has none), any other value as it is.
#[must_use]
pub fn media_ref_path(value: &Value) -> &Value {
    match value {
        Value::Object(map) => map.get("path").unwrap_or(&NULL),
        other => other,
    }
}

/// One place a call may read: a media reference's path, or a value that
/// hides its references (a templated list or element, or a malformed one).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReadSlot<'a> {
    /// `segments[k].image` · `segments[k].audio` · `audio`, with the path
    /// as written ([`media_ref_path`]).
    Media {
        /// The slot name.
        name: String,
        /// The path value.
        path: &'a Value,
    },
    /// `segments` (not a list) or `segments[k]` (not a map): its media are
    /// unknown until the run resolves it.
    Hidden {
        /// The slot name.
        name: String,
        /// The value as written.
        value: &'a Value,
    },
}

impl<'a> ReadSlot<'a> {
    /// The slot's name and the value a consumer judges as a path.
    pub(crate) fn pair(&self) -> (String, &'a Value) {
        match self {
            Self::Media { name, path } => (name.clone(), path),
            Self::Hidden { name, value } => (name.clone(), value),
        }
    }
}

/// Every read of one call, in argument order: each segment's `image` then
/// `audio`, then the soundtrack. A null reference is no reference (an
/// absent clip · a missing still is a shape refusal, not a read).
pub(crate) fn read_slots(args: Option<&Value>) -> Vec<ReadSlot<'_>> {
    let Some(map) = args.and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut slots = Vec::new();
    match map.get("segments") {
        Some(Value::Array(items)) => {
            for (k, item) in items.iter().enumerate() {
                match item.as_object() {
                    Some(segment) => {
                        for key in ["image", "audio"] {
                            if let Some(value) = segment.get(key).filter(|v| !v.is_null()) {
                                slots.push(ReadSlot::Media {
                                    name: format!("segments[{k}].{key}"),
                                    path: media_ref_path(value),
                                });
                            }
                        }
                    }
                    None => slots.push(ReadSlot::Hidden {
                        name: format!("segments[{k}]"),
                        value: item,
                    }),
                }
            }
        }
        Some(Value::Null) | None => {}
        Some(other) => slots.push(ReadSlot::Hidden {
            name: "segments".to_owned(),
            value: other,
        }),
    }
    if let Some(value) = map.get("audio").filter(|v| !v.is_null()) {
        slots.push(ReadSlot::Media {
            name: "audio".to_owned(),
            path: media_ref_path(value),
        });
    }
    slots
}

/// The shape-finding suffix every rule below ends with.
const SUFFIX: &str = "(builtins-v0.1.md §nika:video_compose)";

/// A literal as the author wrote it: a string bare, anything else as JSON.
fn shown(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// A string carrying a `${{ }}` template: the run's to judge.
fn templated(value: &Value) -> bool {
    value.as_str().is_some_and(|s| s.contains("${{"))
}

/// Does any string inside `value` carry a template?
fn holds_template(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains("${{"),
        Value::Array(items) => items.iter().any(holds_template),
        Value::Object(map) => map.values().any(holds_template),
        _ => false,
    }
}

/// A literal unsigned integer inside `range` (a float, a bool or a string
/// never is one).
fn int_in(value: &Value, range: &RangeInclusive<u64>) -> bool {
    value.as_u64().is_some_and(|n| range.contains(&n))
}

/// The literal rules a checker judges before anything runs (the catalog
/// row owns unknown keys and absent required arguments). A templated value
/// is the run's; a literal sibling is still judged.
pub(crate) fn check_shape(args: Option<&Value>, out: &mut Vec<String>) {
    let Some(map) = args.and_then(Value::as_object) else {
        return;
    };
    let soundtrack = map.get("audio").filter(|v| !v.is_null());
    segments_shape(map.get("segments"), soundtrack.is_some(), out);
    if let Some(value) = soundtrack {
        media_ref_shape("audio", value, out);
    }
    out_shape(map.get("out"), out);
    let present = |key: &str| map.get(key).filter(|v| !v.is_null() && !templated(v));
    if let Some(v) = present("size")
        && v.as_str().and_then(size).is_none()
    {
        out.push(format!(
            "`size: {}` must be `WIDTHxHEIGHT` with both sides even · width {}..={} · \
             height {}..={} {SUFFIX}",
            shown(v),
            WIDTH.start(),
            WIDTH.end(),
            HEIGHT.start(),
            HEIGHT.end()
        ));
    }
    for (key, range) in [("fps", &FPS), ("timeout_ms", &TIMEOUT_MS)] {
        if let Some(v) = present(key)
            && !int_in(v, range)
        {
            out.push(format!(
                "`{key}: {}` must be an integer {}..={} {SUFFIX}",
                shown(v),
                range.start(),
                range.end()
            ));
        }
    }
    let words = [
        ("scale", Scale::ALL.map(Scale::as_str).to_vec()),
        ("quality", Quality::ALL.map(Quality::as_str).to_vec()),
        (
            "audio_codec",
            AudioCodec::ALL.map(AudioCodec::as_str).to_vec(),
        ),
    ];
    for (key, allowed) in words {
        if let Some(v) = present(key)
            && !v.as_str().is_some_and(|w| allowed.contains(&w))
        {
            out.push(format!(
                "`{key}: {}` is not in the closed set — {} {SUFFIX}",
                shown(v),
                allowed.join(" · ")
            ));
        }
    }
    transition_shape(present("transition"), out);
    if let Some(v) = present("background")
        && !crate::shape::compose_color_ok(v)
    {
        out.push(format!(
            "`background:` must be #rrggbb or [r, g, b] with channels 0..=255 {SUFFIX}"
        ));
    }
    metadata_shape(present("metadata"), out);
}

/// `transition`: `cut`, the reserved `fade` refused loudly, anything else
/// outside the closed set.
fn transition_shape(value: Option<&Value>, out: &mut Vec<String>) {
    let Some(v) = value else { return };
    match v.as_str() {
        Some(RESERVED_TRANSITION) => out.push(format!(
            "`transition: fade` is reserved — this version cuts from still to still \
             (transition: cut) {SUFFIX}"
        )),
        Some(word) if Transition::parse(word).is_some() => {}
        _ => out.push(format!(
            "`transition: {}` is not in the closed set — cut (fade is reserved) {SUFFIX}",
            shown(v)
        )),
    }
}

/// `metadata`: a free object of at most [`METADATA_MAX_BYTES`] as
/// canonical JSON; a template inside it leaves its size to the run.
fn metadata_shape(value: Option<&Value>, out: &mut Vec<String>) {
    let Some(v) = value else { return };
    if !v.is_object() {
        out.push(format!("`metadata:` must be an object {SUFFIX}"));
        return;
    }
    if holds_template(v) {
        return;
    }
    let len = metadata_len(v);
    if len > METADATA_MAX_BYTES {
        out.push(format!(
            "`metadata:` is {len} bytes as canonical JSON · at most {METADATA_MAX_BYTES} \
             (16 KiB) {SUFFIX}"
        ));
    }
}

/// `out`: a non-empty path ending in `.mp4` (a missing `out` is the catalog
/// row's required-argument refusal).
fn out_shape(value: Option<&Value>, out: &mut Vec<String>) {
    let Some(v) = value else { return };
    if templated(v) {
        return;
    }
    match v.as_str() {
        Some(path) if !path.is_empty() && !out_is_mp4(path) => out.push(format!(
            "`out: {path}` must end in .mp4 — the artifact is an MP4 {SUFFIX}"
        )),
        Some(path) if !path.is_empty() => {}
        _ => out.push(format!("`out:` must be a non-empty path string {SUFFIX}")),
    }
}

/// `segments`: a list of 1..=[`MAX_SEGMENTS`] closed maps, or one whole
/// templated list.
fn segments_shape(value: Option<&Value>, soundtrack: bool, out: &mut Vec<String>) {
    let Some(v) = value else { return };
    match v {
        Value::Array(items) => {
            if !(1..=MAX_SEGMENTS).contains(&items.len()) {
                out.push(format!(
                    "`segments:` holds {} items · the contract is 1..={MAX_SEGMENTS} {SUFFIX}",
                    items.len()
                ));
            }
            for (k, item) in items.iter().enumerate() {
                segment_shape(k, item, soundtrack, out);
            }
        }
        Value::String(s) if s.contains("${{") => {}
        Value::Null => out.push(format!(
            "`segments:` is required — the stills, in playback order {SUFFIX}"
        )),
        _ => out.push(format!(
            "`segments:` must be a list of segments (or one whole `${{{{ }}}}` list) {SUFFIX}"
        )),
    }
}

/// One segment: a closed map with a still, an optional clip, and the
/// duration rules.
fn segment_shape(k: usize, item: &Value, soundtrack: bool, out: &mut Vec<String>) {
    if templated(item) {
        return;
    }
    let Some(map) = item.as_object() else {
        out.push(format!(
            "`segments[{k}]` must be a map {{ image, audio, duration_ms, pad_ms }} (or one \
             whole `${{{{ }}}}` value) {SUFFIX}"
        ));
        return;
    };
    let mut unknown: Vec<&String> = map
        .keys()
        .filter(|key| !SEGMENT_KEYS.contains(&key.as_str()))
        .collect();
    unknown.sort();
    for key in unknown {
        out.push(format!(
            "`segments[{k}]` has no key `{key}` (the keys are {}) {SUFFIX}",
            SEGMENT_KEYS.join(" · ")
        ));
    }
    match map.get("image").filter(|v| !v.is_null()) {
        Some(image) => media_ref_shape(&format!("segments[{k}].image"), image, out),
        None => out.push(format!(
            "`segments[{k}]` requires `image:` (a PNG still) {SUFFIX}"
        )),
    }
    let clip = map.get("audio").filter(|v| !v.is_null());
    if let Some(audio) = clip {
        media_ref_shape(&format!("segments[{k}].audio"), audio, out);
        if soundtrack {
            out.push(format!(
                "`segments[{k}]` carries a clip beside the top-level `audio` soundtrack (one \
                 or the other) {SUFFIX}"
            ));
        }
    }
    let duration = map.get("duration_ms").filter(|v| !v.is_null());
    let pad = map.get("pad_ms").filter(|v| !v.is_null());
    for (key, value, range) in [
        ("duration_ms", duration, &DURATION_MS),
        ("pad_ms", pad, &PAD_MS),
    ] {
        if let Some(v) = value
            && !templated(v)
            && !int_in(v, range)
        {
            out.push(format!(
                "`segments[{k}].{key}: {}` must be an integer {}..={} {SUFFIX}",
                shown(v),
                range.start(),
                range.end()
            ));
        }
    }
    if duration.is_none() && clip.is_none() {
        out.push(format!(
            "`segments[{k}]` has no clip, so it needs `duration_ms:` {SUFFIX}"
        ));
    }
    if pad.is_some() && duration.is_some() {
        out.push(format!(
            "`segments[{k}].pad_ms` applies only to a clip without `duration_ms:` {SUFFIX}"
        ));
    }
}

/// A media reference: a non-empty path, or an object carrying BOTH a path
/// and a 64-lowercase-hex `sha256` (its other keys ignored, so a producer's
/// output passes as it is). A templated part is the run's.
fn media_ref_shape(at: &str, value: &Value, out: &mut Vec<String>) {
    if templated(value) {
        return;
    }
    let map = match value {
        Value::String(path) if path.is_empty() => {
            out.push(format!("`{at}` must be a non-empty path {SUFFIX}"));
            return;
        }
        Value::String(_) => return,
        Value::Object(map) => map,
        _ => {
            out.push(format!(
                "`{at}` must be a path or {{ path, sha256 }} {SUFFIX}"
            ));
            return;
        }
    };
    let missing: Vec<&str> = ["path", "sha256"]
        .into_iter()
        .filter(|key| !map.contains_key(*key))
        .collect();
    if !missing.is_empty() {
        out.push(format!(
            "`{at}` carries no `{}` — a media reference object needs BOTH `path` and \
             `sha256`, so a misspelt key never drops the binding {SUFFIX}",
            missing.join("` and no `")
        ));
        return;
    }
    let (path, digest) = (
        map.get("path").unwrap_or(&NULL),
        map.get("sha256").unwrap_or(&NULL),
    );
    if !templated(path) && path.as_str().is_none_or(str::is_empty) {
        out.push(format!(
            "`{at}.path` must be a non-empty path string {SUFFIX}"
        ));
    }
    if !templated(digest) && !digest.as_str().is_some_and(is_sha256_hex) {
        out.push(format!(
            "`{at}.sha256` must be 64 lowercase hex digits {SUFFIX}"
        ));
    }
}

#[cfg(test)]
#[path = "video_tests.rs"]
mod tests;
