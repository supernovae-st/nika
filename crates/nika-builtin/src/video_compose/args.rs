// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resolved arguments of one `nika:video_compose` call, parsed against
//! the contract `nika_cap::video` holds for the check too (pure · before any
//! I/O · every refusal `-001`, except the segment budget, `-005`).
//!
//! A value the static check accepts as a literal is a value this parser
//! accepts: the bounds, the closed sets, the size grammar, the digest form
//! and the metadata measure are the check's own (one table, two readers).
//! At run every value is resolved, so nothing here defers to a template.

// The encode pipeline that consumes these fields is the video backend's
// change; until it lands the call refuses `-010` right after this parse.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the parsed fields feed the encode pipeline the video backend wires; \
                  this build parses, then refuses -010"
    )
)]

use nika_cap::video::{
    self, ARGS, AudioCodec, DURATION_MS, FPS, MAX_SEGMENTS, METADATA_MAX_BYTES, PAD_MS, Quality,
    RESERVED_TRANSITION, SEGMENT_KEYS, Scale, TIMEOUT_MS, Transition,
};
use serde_json::{Map, Value};

use super::{C_ARGS, C_BUDGET};
use crate::{Args, BuiltinFailure};

/// One media input: the path read, and the digest its bytes must hash to
/// when the reference carried one (`-004` otherwise, at the read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediaRef {
    /// The path, as resolved.
    pub(crate) path: String,
    /// The 64-lowercase-hex digest of an object reference.
    pub(crate) sha256: Option<String>,
}

/// How long one segment holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hold {
    /// `duration_ms`: the whole segment (at least its clip, when it has one).
    For {
        /// Milliseconds.
        duration_ms: u64,
    },
    /// No `duration_ms`: the clip, then `pad_ms` of silence.
    Clip {
        /// Milliseconds of silence after the clip (default 0).
        pad_ms: u64,
    },
}

/// One segment, in playback order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Segment {
    /// The still.
    pub(crate) image: MediaRef,
    /// The clip, when the segment has one.
    pub(crate) audio: Option<MediaRef>,
    /// How long it holds.
    pub(crate) hold: Hold,
}

/// The parsed call, every default applied.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VideoArgs {
    /// 1..=200 segments.
    pub(crate) segments: Vec<Segment>,
    /// The one soundtrack, when given (then no segment carries a clip).
    pub(crate) soundtrack: Option<MediaRef>,
    /// The audio track's codec.
    pub(crate) audio_codec: AudioCodec,
    /// The artifact path (`.mp4`).
    pub(crate) out: String,
    /// The frame width, in pixels (even).
    pub(crate) width: u32,
    /// The frame height, in pixels (even).
    pub(crate) height: u32,
    /// Frames per second.
    pub(crate) fps: u32,
    /// How each still lands in the frame.
    pub(crate) scale: Scale,
    /// The letterbox colour, and the colour behind alpha.
    pub(crate) background: [u8; 3],
    /// The quality ladder step.
    pub(crate) quality: Quality,
    /// How one still gives way to the next.
    pub(crate) transition: Transition,
    /// The free object echoed into the outputs (never to the encoder).
    pub(crate) metadata: Option<Map<String, Value>>,
    /// The call's own deadline, in milliseconds.
    pub(crate) timeout_ms: u64,
}

fn fail(message: impl Into<String>) -> BuiltinFailure {
    BuiltinFailure::new(C_ARGS, message)
}

/// Parse a call's resolved arguments.
///
/// # Errors
/// `-001` for every shape, closed-set and bound refusal (the first one
/// found, in argument order) · `-005` for more than [`MAX_SEGMENTS`].
pub(crate) fn parse(args: &Args) -> Result<VideoArgs, BuiltinFailure> {
    if let Some(unknown) = args.keys().find(|k| !ARGS.contains(&k.as_str())) {
        return Err(fail(format!(
            "unknown argument `{unknown}` (the arguments are {} · captions are not v1)",
            ARGS.join(" · ")
        )));
    }
    let soundtrack = match present(args, "audio") {
        Some(value) => Some(media_ref("audio", value)?),
        None => None,
    };
    let segments = parse_segments(args, soundtrack.is_some())?;
    let out = match present(args, "out").and_then(Value::as_str) {
        Some(path) if !path.is_empty() && video::out_is_mp4(path) => path.to_owned(),
        Some(path) if !path.is_empty() => {
            return Err(fail(format!(
                "`out: {path}` must end in .mp4 — the artifact is an MP4"
            )));
        }
        _ => return Err(fail("`out:` is required (a non-empty .mp4 path)")),
    };
    let (width, height) = match present(args, "size") {
        None => video::DEFAULT_SIZE,
        Some(value) => value.as_str().and_then(video::size).ok_or_else(|| {
            fail(format!(
                "`size: {}` must be `WIDTHxHEIGHT` with both sides even · width {}..={} · \
                 height {}..={}",
                shown(value),
                video::WIDTH.start(),
                video::WIDTH.end(),
                video::HEIGHT.start(),
                video::HEIGHT.end()
            ))
        })?,
    };
    let fps = match present(args, "fps") {
        None => video::DEFAULT_FPS,
        Some(value) => u32::try_from(int_in("fps", value, &FPS)?)
            .map_err(|_| fail("`fps:` must be an integer 1..=60"))?,
    };
    let timeout_ms = match present(args, "timeout_ms") {
        None => video::DEFAULT_TIMEOUT_MS,
        Some(value) => int_in("timeout_ms", value, &TIMEOUT_MS)?,
    };
    Ok(VideoArgs {
        segments,
        soundtrack,
        audio_codec: word(
            args,
            "audio_codec",
            AudioCodec::parse,
            &AudioCodec::ALL.map(AudioCodec::as_str),
        )?
        .unwrap_or(AudioCodec::Aac),
        out,
        width,
        height,
        fps,
        scale: word(args, "scale", Scale::parse, &Scale::ALL.map(Scale::as_str))?
            .unwrap_or(Scale::Fit),
        background: match present(args, "background") {
            None => video::DEFAULT_BACKGROUND,
            Some(value) => parse_background(value)?,
        },
        quality: word(
            args,
            "quality",
            Quality::parse,
            &Quality::ALL.map(Quality::as_str),
        )?
        .unwrap_or(Quality::Standard),
        transition: parse_transition(present(args, "transition"))?,
        metadata: parse_metadata(present(args, "metadata"))?,
        timeout_ms,
    })
}

/// A present, non-null argument (a null optional argument is its default).
fn present<'a>(args: &'a Args, key: &str) -> Option<&'a Value> {
    args.get(key).filter(|v| !v.is_null())
}

/// A value as the author wrote it: a string bare, anything else as JSON.
fn shown(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// A literal unsigned integer inside `range` (a float, a bool or a string
/// never is one).
fn int_in(
    key: &str,
    value: &Value,
    range: &std::ops::RangeInclusive<u64>,
) -> Result<u64, BuiltinFailure> {
    value.as_u64().filter(|n| range.contains(n)).ok_or_else(|| {
        fail(format!(
            "`{key}: {}` must be an integer {}..={}",
            shown(value),
            range.start(),
            range.end()
        ))
    })
}

/// One closed-set word, `None` when the argument is absent or null.
fn word<T>(
    args: &Args,
    key: &str,
    parse: fn(&str) -> Option<T>,
    allowed: &[&str],
) -> Result<Option<T>, BuiltinFailure> {
    let Some(value) = present(args, key) else {
        return Ok(None);
    };
    value.as_str().and_then(parse).map(Some).ok_or_else(|| {
        fail(format!(
            "`{key}: {}` is not in the closed set — {}",
            shown(value),
            allowed.join(" · ")
        ))
    })
}

/// `transition`: `cut` (the default), the reserved `fade` refused loudly.
fn parse_transition(value: Option<&Value>) -> Result<Transition, BuiltinFailure> {
    let Some(value) = value else {
        return Ok(Transition::Cut);
    };
    match value.as_str() {
        Some(RESERVED_TRANSITION) => Err(fail(
            "`transition: fade` is reserved — this version cuts from still to still \
             (transition: cut)",
        )),
        Some(word) => Transition::parse(word).ok_or_else(|| {
            fail(format!(
                "`transition: {word}` is not in the closed set — cut (fade is reserved)"
            ))
        }),
        None => Err(fail(format!(
            "`transition: {}` is not in the closed set — cut (fade is reserved)",
            shown(value)
        ))),
    }
}

/// `background`: `#rrggbb` or `[r, g, b]`, channels 0..=255.
fn parse_background(value: &Value) -> Result<[u8; 3], BuiltinFailure> {
    let refused = || fail("`background:` must be #rrggbb or [r, g, b] with channels 0..=255");
    if let Some(text) = value.as_str() {
        let hex = text
            .strip_prefix('#')
            .filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(refused)?;
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| refused());
        return Ok([channel(0)?, channel(2)?, channel(4)?]);
    }
    let list = value
        .as_array()
        .filter(|l| l.len() == 3)
        .ok_or_else(refused)?;
    let channel = |i: usize| {
        list[i]
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .ok_or_else(refused)
    };
    Ok([channel(0)?, channel(1)?, channel(2)?])
}

/// `metadata`: a free object of at most 16 KiB as canonical JSON.
fn parse_metadata(value: Option<&Value>) -> Result<Option<Map<String, Value>>, BuiltinFailure> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(map) = value.as_object() else {
        return Err(fail("`metadata:` must be an object"));
    };
    let len = video::metadata_len(value);
    if len > METADATA_MAX_BYTES {
        return Err(fail(format!(
            "`metadata:` is {len} bytes as canonical JSON · at most {METADATA_MAX_BYTES} (16 KiB)"
        )));
    }
    Ok(Some(map.clone()))
}

/// `segments`: 1..=200 closed maps (more is the segment budget, `-005`).
fn parse_segments(args: &Args, soundtrack: bool) -> Result<Vec<Segment>, BuiltinFailure> {
    let Some(value) = present(args, "segments") else {
        return Err(fail(
            "`segments:` is required — the stills, in playback order",
        ));
    };
    let Some(items) = value.as_array() else {
        return Err(fail("`segments:` must be a list of segments"));
    };
    if items.is_empty() {
        return Err(fail(
            "`segments:` holds no segment · the contract is 1..=200",
        ));
    }
    if items.len() > MAX_SEGMENTS {
        return Err(BuiltinFailure::new(
            C_BUDGET,
            format!(
                "the segment budget: {} segments · at most {MAX_SEGMENTS}",
                items.len()
            ),
        ));
    }
    items
        .iter()
        .enumerate()
        .map(|(k, item)| parse_segment(k, item, soundtrack))
        .collect()
}

/// One segment: a still, an optional clip, and how long it holds.
fn parse_segment(k: usize, item: &Value, soundtrack: bool) -> Result<Segment, BuiltinFailure> {
    let Some(map) = item.as_object() else {
        return Err(fail(format!(
            "`segments[{k}]` must be a map {{ image, audio, duration_ms, pad_ms }}"
        )));
    };
    if let Some(key) = map.keys().find(|key| !SEGMENT_KEYS.contains(&key.as_str())) {
        return Err(fail(format!(
            "`segments[{k}]` has no key `{key}` (the keys are {})",
            SEGMENT_KEYS.join(" · ")
        )));
    }
    let field = |key: &str| map.get(key).filter(|v| !v.is_null());
    let image = match field("image") {
        Some(value) => media_ref(&format!("segments[{k}].image"), value)?,
        None => {
            return Err(fail(format!(
                "`segments[{k}]` requires `image:` (a PNG still)"
            )));
        }
    };
    let audio = match field("audio") {
        Some(value) => Some(media_ref(&format!("segments[{k}].audio"), value)?),
        None => None,
    };
    if audio.is_some() && soundtrack {
        return Err(fail(format!(
            "`segments[{k}]` carries a clip beside the top-level `audio` soundtrack (one or \
             the other)"
        )));
    }
    let bounded = |key: &str, range| {
        field(key)
            .map(|value| int_in(&format!("segments[{k}].{key}"), value, range))
            .transpose()
    };
    let duration_ms = bounded("duration_ms", &DURATION_MS)?;
    let pad_ms = bounded("pad_ms", &PAD_MS)?;
    let hold = match (duration_ms, pad_ms, audio.is_some()) {
        (Some(_), Some(_), _) => {
            return Err(fail(format!(
                "`segments[{k}].pad_ms` applies only to a clip without `duration_ms:`"
            )));
        }
        (Some(duration_ms), None, _) => Hold::For { duration_ms },
        (None, pad, true) => Hold::Clip {
            pad_ms: pad.unwrap_or(0),
        },
        (None, _, false) => {
            return Err(fail(format!(
                "`segments[{k}]` has no clip, so it needs `duration_ms:`"
            )));
        }
    };
    Ok(Segment { image, audio, hold })
}

/// A media reference: a non-empty path, or an object carrying BOTH a
/// non-empty `path` and a 64-lowercase-hex `sha256` (its other keys
/// ignored, so a producer's output object passes as it is).
fn media_ref(at: &str, value: &Value) -> Result<MediaRef, BuiltinFailure> {
    match value {
        Value::String(path) if !path.is_empty() => Ok(MediaRef {
            path: path.clone(),
            sha256: None,
        }),
        Value::String(_) => Err(fail(format!("`{at}` must be a non-empty path"))),
        Value::Object(map) => {
            let missing: Vec<&str> = ["path", "sha256"]
                .into_iter()
                .filter(|key| !map.contains_key(*key))
                .collect();
            if !missing.is_empty() {
                return Err(fail(format!(
                    "`{at}` carries no `{}` — a media reference object needs BOTH `path` and \
                     `sha256`, so a misspelt key never drops the binding",
                    missing.join("` and no `")
                )));
            }
            let path = map
                .get("path")
                .and_then(Value::as_str)
                .filter(|p| !p.is_empty())
                .ok_or_else(|| fail(format!("`{at}.path` must be a non-empty path string")))?;
            let digest = map
                .get("sha256")
                .and_then(Value::as_str)
                .filter(|d| video::is_sha256_hex(d))
                .ok_or_else(|| fail(format!("`{at}.sha256` must be 64 lowercase hex digits")))?;
            Ok(MediaRef {
                path: path.to_owned(),
                sha256: Some(digest.to_owned()),
            })
        }
        _ => Err(fail(format!("`{at}` must be a path or {{ path, sha256 }}"))),
    }
}
