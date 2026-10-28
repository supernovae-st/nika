// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The plan of one `nika:video_compose` call — what every backend receives
//! (stdlib §`nika:video_compose` · Timing · Preparation · Budgets). Pure
//! integer arithmetic: no float, no clock, no I/O.
//!
//! Audio is the clock. R is the output sample rate and f the frame rate;
//! `round(a ÷ b)` is `⌊(a + ⌊b ÷ 2⌋) ÷ b⌋`. Segment k lasts `D_k` samples and
//! starts at `S_k` (the sum of the `D_j` before it); the video lasts T samples.
//! The frame of sample position t is `F(t) = ⌊(2·t·f + R) ÷ (2R)⌋`, so
//! segment k shows the frames `F(S_k)` up to, not including, `F(S_k + D_k)`: at
//! every boundary picture and sound differ by at most 1/(2f) seconds, and the
//! error never accumulates. Every segment starts with a keyframe, and two
//! consecutive keyframes are never more than G = 10 × f frames apart.

// The encode pipeline that consumes the plan is the video backend's change;
// until it lands the call refuses `-010` right after parsing.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the plan feeds the encode pipeline the video backend wires; this build \
                  parses, then refuses -010"
    )
)]

use std::ops::RangeInclusive;

use nika_cap::video::{AudioCodec, Scale};
use nika_fx::FxError;

use super::args::{Hold, VideoArgs};
use super::{C_ARGS, C_BUDGET, C_FORMAT};
use crate::BuiltinFailure;

/// The clock of a video with no audio at all.
pub(crate) const SILENT_RATE: u32 = 48_000;

/// The MPEG-4 sampling rates an `aac` track takes; a common rate outside
/// it is refused (`-003` · « use flac, or resample upstream »).
pub(crate) const MPEG4_RATES: [u32; 13] = [
    7_350, 8_000, 11_025, 12_000, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 64_000, 88_200,
    96_000,
];

/// The rates a WAV clip may carry.
pub(crate) const CLIP_RATES: RangeInclusive<u32> = 8_000..=96_000;

/// The longest video, in seconds (T ≤ 3600 × R).
pub(crate) const MAX_SECONDS: u64 = 3_600;

/// The plan's intermediates cap — 4:2:0 frames plus 16-bit PCM — in bytes.
pub(crate) const MAX_PLAN_BYTES: u64 = 1 << 30;

/// The keyframe interval, in seconds of frames (G = 10 × f).
pub(crate) const KEYFRAME_SECONDS: u64 = 10;

/// A still's byte cap, judged on the file's size before it is read.
pub(crate) const MAX_STILL_BYTES: u64 = 512 << 20;

/// A clip's or the soundtrack's byte cap, judged the same way.
pub(crate) const MAX_CLIP_BYTES: u64 = 2 << 30;

/// The cap on any one argument handed to the encoder, in bytes.
pub(crate) const MAX_ENCODER_ARG_BYTES: usize = 64 << 10;

/// The output's cap (a tripped size limit included), in bytes.
pub(crate) const MAX_OUTPUT_BYTES: u64 = 256 << 20;

/// What one clip's header says (a segment's clip or the soundtrack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClipFacts {
    /// Sample frames (per channel) the clip carries.
    pub(crate) samples: u64,
    /// Its sample rate, in hertz.
    pub(crate) rate: u32,
    /// Its channel count.
    pub(crate) channels: u16,
}

/// One segment of the plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SegmentPlan {
    /// `S_k`, in samples at R.
    pub(crate) start: u64,
    /// `D_k`, in samples at R.
    pub(crate) length: u64,
    /// `F(S_k)`.
    pub(crate) first_frame: u64,
    /// `F(S_k + D_k) − F(S_k)`, at least one.
    pub(crate) frames: u64,
    /// The clip's length at R, when the segment has one (the rest of `D_k`
    /// is digital silence).
    pub(crate) clip_samples: Option<u64>,
}

/// The whole plan: the same for every backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    /// R, the output sample rate (the silent clock for a silent video).
    pub(crate) rate: u32,
    /// The planned channel count · 0 for a silent video (no audio track).
    pub(crate) channels: u16,
    /// f, frames per second.
    pub(crate) fps: u32,
    /// T, the video's length in samples at R.
    pub(crate) samples: u64,
    /// F(T), the video's frame count.
    pub(crate) frames: u64,
    /// G = 10 × f, the longest gap between two keyframes.
    pub(crate) keyframe_interval: u64,
    /// The segments, in playback order.
    pub(crate) segments: Vec<SegmentPlan>,
    /// The soundtrack is longer than T and is cut at the end of the video
    /// (the `soundtrack_trimmed:` warning).
    pub(crate) soundtrack_trimmed: bool,
}

/// `round(a ÷ b) = ⌊(a + ⌊b ÷ 2⌋) ÷ b⌋` (the `image_compose` rounding).
/// Every call site divides by a rate or by 1000; a zero divisor yields 0
/// rather than a panic.
pub(crate) fn round_div(a: u128, b: u128) -> u128 {
    a.saturating_add(b / 2).checked_div(b).unwrap_or(0)
}

/// `F(t) = ⌊(2·t·f + R) ÷ (2R)⌋`, the frame showing at sample position `t`.
pub(crate) fn frame_at(t: u64, fps: u32, rate: u32) -> u64 {
    let (t, f, r) = (u128::from(t), u128::from(fps), u128::from(rate));
    // t ≤ 3600·R and f ≤ 60, so the frame index fits a u64 by far.
    let frame = (2 * t * f + r).checked_div(2 * r).unwrap_or(0);
    u64::try_from(frame).unwrap_or(u64::MAX)
}

/// Milliseconds of `samples` at `rate`: `round(samples × 1000 ÷ R)`.
pub(crate) fn ms_of(samples: u64, rate: u32) -> u64 {
    u64::try_from(round_div(u128::from(samples) * 1_000, u128::from(rate))).unwrap_or(u64::MAX)
}

/// Samples at `rate` of `ms` milliseconds: `round(ms × R ÷ 1000)`.
pub(crate) fn samples_of(ms: u64, rate: u32) -> u64 {
    u64::try_from(round_div(u128::from(ms) * u128::from(rate), 1_000)).unwrap_or(u64::MAX)
}

/// The output sample rate: the soundtrack's, else the clips' common rate,
/// else [`SILENT_RATE`]. Clips at different rates are refused (`-003`), and
/// so is an `aac` rate outside the MPEG-4 table.
///
/// # Errors
/// `-003` as above.
pub(crate) fn sample_rate(audio: &[ClipFacts], codec: AudioCodec) -> Result<u32, BuiltinFailure> {
    let Some(first) = audio.first() else {
        return Ok(SILENT_RATE);
    };
    if let Some(other) = audio.iter().find(|c| c.rate != first.rate) {
        return Err(BuiltinFailure::new(
            C_FORMAT,
            format!(
                "clips must share one sample rate in this version ({} Hz and {} Hz) · \
                 resample upstream",
                first.rate, other.rate
            ),
        ));
    }
    if codec == AudioCodec::Aac && !MPEG4_RATES.contains(&first.rate) {
        return Err(BuiltinFailure::new(
            C_FORMAT,
            format!(
                "an aac track takes an MPEG-4 sampling rate, and {} Hz is not one · use \
                 `audio_codec: flac`, or resample upstream",
                first.rate
            ),
        ));
    }
    Ok(first.rate)
}

/// Stereo when any clip is stereo (a mono clip plays on both channels),
/// else mono · 0 when there is no audio at all.
pub(crate) fn channels(audio: &[ClipFacts]) -> u16 {
    audio.iter().map(|c| c.channels).max().unwrap_or(0)
}

/// A clip this version takes: 1 or 2 channels at 8000..=96000 Hz (`-003`
/// otherwise, naming where it sits).
fn clip_supported(at: &str, clip: &ClipFacts) -> Result<(), BuiltinFailure> {
    if !(1..=2).contains(&clip.channels) {
        return Err(BuiltinFailure::new(
            C_FORMAT,
            format!(
                "`{at}` has {} channels · a clip is mono or stereo",
                clip.channels
            ),
        ));
    }
    if !CLIP_RATES.contains(&clip.rate) {
        return Err(BuiltinFailure::new(
            C_FORMAT,
            format!(
                "`{at}` is {} Hz · a clip is {}..={} Hz",
                clip.rate,
                CLIP_RATES.start(),
                CLIP_RATES.end()
            ),
        ));
    }
    Ok(())
}

/// The length of segment `k` in samples at `rate`, `D_k`.
fn segment_length(
    k: usize,
    hold: Hold,
    clip: Option<u64>,
    rate: u32,
) -> Result<u64, BuiltinFailure> {
    match (hold, clip) {
        (Hold::For { duration_ms }, clip) => {
            let length = samples_of(duration_ms, rate);
            match clip {
                Some(clip) if clip > length => Err(BuiltinFailure::new(
                    C_ARGS,
                    format!(
                        "`segments[{k}].duration_ms: {duration_ms}` is shorter than its clip \
                         ({} ms) · never a silent cut",
                        ms_of(clip, rate)
                    ),
                )),
                _ => Ok(length),
            }
        }
        (Hold::Clip { pad_ms }, Some(clip)) => Ok(clip.saturating_add(samples_of(pad_ms, rate))),
        (Hold::Clip { .. }, None) => Err(BuiltinFailure::new(
            C_ARGS,
            format!("`segments[{k}]` has no clip, so it needs `duration_ms:`"),
        )),
    }
}

/// Build the plan from the parsed call and each clip's header (`clips[k]`
/// is segment k's · `None` when it has none) and the soundtrack's.
///
/// # Errors
/// `-003` (a clip's channels or rate · clips at different rates · an `aac`
/// rate outside the MPEG-4 table) · `-001` (a duration shorter than its clip
/// · a segment that would show no frame) · `-005` (more than one hour · the
/// plan's intermediates above 1 GiB).
pub(crate) fn plan(
    args: &VideoArgs,
    clips: &[Option<ClipFacts>],
    soundtrack: Option<ClipFacts>,
) -> Result<Plan, BuiltinFailure> {
    if clips.len() != args.segments.len() {
        return Err(BuiltinFailure::new(
            C_ARGS,
            format!(
                "{} clip facts for {} segments",
                clips.len(),
                args.segments.len()
            ),
        ));
    }
    let mut audio = Vec::new();
    for (k, clip) in clips.iter().enumerate() {
        if let Some(clip) = clip {
            clip_supported(&format!("segments[{k}].audio"), clip)?;
            audio.push(*clip);
        }
    }
    if let Some(track) = &soundtrack {
        clip_supported("audio", track)?;
        audio.push(*track);
    }
    let rate = sample_rate(&audio, args.audio_codec)?;
    let mut segments = Vec::with_capacity(args.segments.len());
    let mut start: u64 = 0;
    for (k, (segment, clip)) in args.segments.iter().zip(clips).enumerate() {
        let clip_samples = clip.map(|c| c.samples);
        let length = segment_length(k, segment.hold, clip_samples, rate)?;
        let end = start.saturating_add(length);
        let first_frame = frame_at(start, args.fps, rate);
        let frames = frame_at(end, args.fps, rate).saturating_sub(first_frame);
        if frames == 0 {
            return Err(BuiltinFailure::new(
                C_ARGS,
                format!(
                    "`segments[{k}]` would show no frame at {} fps ({} ms) · hold it longer, \
                     or raise `fps:`",
                    args.fps,
                    ms_of(length, rate)
                ),
            ));
        }
        segments.push(SegmentPlan {
            start,
            length,
            first_frame,
            frames,
            clip_samples,
        });
        start = end;
    }
    let samples = start;
    if samples > MAX_SECONDS.saturating_mul(u64::from(rate)) {
        return Err(BuiltinFailure::new(
            C_BUDGET,
            format!(
                "the video would last {} ms · at most one hour ({} ms)",
                ms_of(samples, rate),
                MAX_SECONDS * 1_000
            ),
        ));
    }
    let channels = channels(&audio);
    let staged = intermediate_bytes(
        args.segments.len(),
        (args.width, args.height),
        samples,
        channels,
    );
    if staged > MAX_PLAN_BYTES {
        return Err(budget_of_intermediates(staged));
    }
    Ok(Plan {
        rate,
        channels,
        fps: args.fps,
        samples,
        frames: frame_at(samples, args.fps, rate),
        keyframe_interval: KEYFRAME_SECONDS * u64::from(args.fps),
        segments,
        soundtrack_trimmed: soundtrack.is_some_and(|t| t.samples > samples),
    })
}

/// The plan's intermediates, in bytes: `(segments + 1) × W × H × 3/2` of
/// 4:2:0 frames plus `T × channels × 2` of 16-bit PCM (no PCM for a silent
/// video, `channels` 0). Saturates rather than wraps.
pub(crate) fn intermediate_bytes(
    segments: usize,
    (width, height): (u32, u32),
    samples: u64,
    channels: u16,
) -> u64 {
    let segments = u128::try_from(segments).unwrap_or(u128::MAX);
    let frames = segments
        .saturating_add(1)
        .saturating_mul(u128::from(width) * u128::from(height) * 3 / 2);
    let pcm = u128::from(samples) * u128::from(channels) * 2;
    u64::try_from(frames.saturating_add(pcm)).unwrap_or(u64::MAX)
}

/// The `-005` of the plan's intermediates.
pub(crate) fn budget_of_intermediates(staged: u64) -> BuiltinFailure {
    BuiltinFailure::new(
        C_BUDGET,
        format!(
            "the plan's intermediates would hold {staged} bytes · at most {MAX_PLAN_BYTES} \
             (1 GiB) · fewer segments, a smaller size, or a shorter video"
        ),
    )
}

impl Plan {
    /// Every keyframe, ascending: each segment start s, and s + m·G inside
    /// the segment (m ≥ 1) — the placement the ffmpeg backend forces.
    pub(crate) fn keyframes(&self) -> Vec<u64> {
        let step = self.keyframe_interval.max(1);
        let mut out = Vec::new();
        for segment in &self.segments {
            let end = segment.first_frame + segment.frames;
            let mut frame = segment.first_frame;
            while frame < end {
                out.push(frame);
                frame = frame.saturating_add(step);
            }
        }
        out
    }

    /// The video's length, `round(T × 1000 ÷ R)` milliseconds.
    pub(crate) fn duration_ms(&self) -> u64 {
        ms_of(self.samples, self.rate)
    }
}

/// Which byte cap an input file answers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    /// A PNG still.
    Still,
    /// A WAV clip or the soundtrack.
    Clip,
}

/// An input file within its byte cap, judged on its size BEFORE it is read
/// and never read past (`-005` otherwise).
///
/// # Errors
/// `-005` naming the slot and the cap.
pub(crate) fn input_within_cap(
    at: &str,
    kind: Input,
    size_bytes: u64,
) -> Result<(), BuiltinFailure> {
    let (cap, label) = match kind {
        Input::Still => (MAX_STILL_BYTES, "512 MiB per still"),
        Input::Clip => (MAX_CLIP_BYTES, "2 GiB per clip or soundtrack"),
    };
    if size_bytes > cap {
        return Err(BuiltinFailure::new(
            C_BUDGET,
            format!("`{at}` is {size_bytes} bytes · the cap is {label}"),
        ));
    }
    Ok(())
}

/// The box a `width × height` still is scaled to in the frame: `fit` and
/// `fill` are `image_compose`'s geometry with ONE cell of the frame, capped
/// at 16384 per side and 2^26 pixels (`-005`, judged from the header and
/// `size`) · `none` scales nothing.
///
/// # Errors
/// `-005` when the scaled box breaks a cap.
pub(crate) fn placed_box(
    at: &str,
    (width, height): (u32, u32),
    scale: Scale,
    frame: (u32, u32),
) -> Result<(u32, u32), BuiltinFailure> {
    let scale = match scale {
        Scale::Fit => nika_fx::compose::Scale::Fit,
        Scale::Fill => nika_fx::compose::Scale::Fill,
        _ => return Ok((width, height)),
    };
    nika_fx::compose::placed_size(width, height, scale, frame).map_err(|e| match e {
        FxError::Budget { pixels, max } => BuiltinFailure::new(
            C_BUDGET,
            format!("`{at}` would be scaled to {pixels} pixels · a scaled box holds at most {max}"),
        ),
        FxError::Cap(m) => BuiltinFailure::new(C_BUDGET, format!("`{at}` · {m}")),
        other => BuiltinFailure::new(C_ARGS, format!("`{at}` · {other}")),
    })
}

#[cfg(test)]
mod tests;
