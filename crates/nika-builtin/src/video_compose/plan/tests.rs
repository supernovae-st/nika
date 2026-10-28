// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The plan, pinned: the storyboard's golden timing, the timing law and the
//! keyframe law over random layouts, the sample-rate rules and every budget.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use nika_cap::video::{AudioCodec, Scale};
use proptest::prelude::*;
use serde_json::{Value, json};

use super::super::args::{Hold, VideoArgs, parse};
use super::super::{C_ARGS, C_BUDGET, C_FORMAT};
use super::*;

fn args_of(value: Value) -> VideoArgs {
    let Value::Object(map) = value else {
        panic!("an args object");
    };
    parse(&map).expect("lawful arguments")
}

fn mono(samples: u64, rate: u32) -> ClipFacts {
    ClipFacts {
        samples,
        rate,
        channels: 1,
    }
}

/// The four narrated stills of the storyboard: 24 kHz mono clips of
/// 172,800 · 160,800 · 178,800 · 150,000 samples, each held 600 ms longer,
/// at 25 fps.
fn storyboard() -> (VideoArgs, Vec<Option<ClipFacts>>) {
    let segment = |k: usize| json!({ "image": format!("./f/{k}.png"), "audio": format!("./v/{k}.wav"), "pad_ms": 600 });
    let args = args_of(json!({
        "segments": [segment(1), segment(2), segment(3), segment(4)],
        "out": "./out/storyboard.mp4",
    }));
    let clips = [172_800, 160_800, 178_800, 150_000]
        .into_iter()
        .map(|n| Some(mono(n, 24_000)))
        .collect();
    (args, clips)
}

#[test]
fn the_storyboard_plan_is_golden() {
    let (args, clips) = storyboard();
    let plan = plan(&args, &clips, None).expect("plans");
    assert_eq!((plan.rate, plan.channels, plan.fps), (24_000, 1, 25));
    assert_eq!(plan.samples, 720_000);
    assert_eq!(plan.duration_ms(), 30_000);
    assert_eq!(plan.frames, 750);
    assert_eq!(plan.keyframe_interval, 250);
    let firsts: Vec<u64> = plan.segments.iter().map(|s| s.first_frame).collect();
    let frames: Vec<u64> = plan.segments.iter().map(|s| s.frames).collect();
    assert_eq!(firsts, [0, 195, 378, 579]);
    assert_eq!(frames, [195, 183, 201, 171]);
    let lengths: Vec<u64> = plan.segments.iter().map(|s| s.length).collect();
    assert_eq!(lengths, [187_200, 175_200, 193_200, 164_400]);
    assert_eq!(plan.keyframes(), [0, 195, 378, 579]);
    assert!(!plan.soundtrack_trimmed);
}

/// A long still gets a keyframe every G frames inside it, and the next
/// segment starts on one.
#[test]
fn a_long_still_carries_a_keyframe_every_ten_seconds() {
    let args = args_of(json!({
        "segments": [
            { "image": "./a.png", "duration_ms": 25_000 },
            { "image": "./b.png", "duration_ms": 1_000 },
        ],
        "out": "./v.mp4",
        "fps": 30,
    }));
    let plan = plan(&args, &[None, None], None).expect("plans");
    assert_eq!(
        plan.rate, SILENT_RATE,
        "a silent video runs on the 48 kHz clock"
    );
    assert_eq!(plan.channels, 0, "and stages no audio");
    assert_eq!(plan.keyframe_interval, 300);
    assert_eq!(plan.keyframes(), [0, 300, 600, 750]);
    assert_eq!(plan.frames, 780);
}

#[test]
fn the_rate_follows_the_clips_and_the_codec() {
    assert_eq!(sample_rate(&[], AudioCodec::Aac).ok(), Some(SILENT_RATE));
    let same = [mono(10, 22_050), mono(20, 22_050)];
    assert_eq!(sample_rate(&same, AudioCodec::Aac).ok(), Some(22_050));
    let mixed = [mono(10, 22_050), mono(20, 24_000)];
    let refused = sample_rate(&mixed, AudioCodec::Flac).expect_err("mixed rates");
    assert_eq!(refused.code, C_FORMAT);
    assert!(
        refused.message.contains("share one sample rate"),
        "{}",
        refused.message
    );
    // aac takes the MPEG-4 table only; flac keeps any rate.
    let odd = [mono(10, 37_800)];
    let refused = sample_rate(&odd, AudioCodec::Aac).expect_err("off the table");
    assert_eq!(refused.code, C_FORMAT);
    assert!(
        refused.message.contains("audio_codec: flac"),
        "{}",
        refused.message
    );
    assert_eq!(sample_rate(&odd, AudioCodec::Flac).ok(), Some(37_800));
    for rate in MPEG4_RATES {
        assert_eq!(
            sample_rate(&[mono(1, rate)], AudioCodec::Aac).ok(),
            Some(rate)
        );
    }
}

#[test]
fn stereo_wins_and_a_silent_video_has_no_channel() {
    assert_eq!(channels(&[]), 0);
    assert_eq!(channels(&[mono(1, 48_000)]), 1);
    let stereo = ClipFacts {
        channels: 2,
        ..mono(1, 48_000)
    };
    assert_eq!(channels(&[mono(1, 48_000), stereo]), 2);
}

#[test]
fn a_clip_outside_the_wav_law_is_an_unsupported_format() {
    let args = args_of(json!({
        "segments": [{ "image": "./a.png", "audio": "./a.wav" }],
        "out": "./v.mp4",
    }));
    for (clip, needle) in [
        (
            ClipFacts {
                channels: 3,
                ..mono(48_000, 48_000)
            },
            "3 channels",
        ),
        (
            ClipFacts {
                channels: 0,
                ..mono(48_000, 48_000)
            },
            "0 channels",
        ),
        (mono(48_000, 7_999), "7999 Hz"),
        (mono(48_000, 96_001), "96001 Hz"),
    ] {
        let refused = plan(&args, &[Some(clip)], None).expect_err("refused");
        assert_eq!(refused.code, C_FORMAT, "{needle}");
        assert!(
            refused.message.contains("segments[0].audio") && refused.message.contains(needle),
            "{}",
            refused.message
        );
    }
}

#[test]
fn a_duration_never_cuts_its_clip() {
    let args = args_of(json!({
        "segments": [{ "image": "./a.png", "audio": "./a.wav", "duration_ms": 1_000 }],
        "out": "./v.mp4",
    }));
    // 1000 ms at 48 kHz is 48,000 samples: a clip of exactly that fits…
    assert!(plan(&args, &[Some(mono(48_000, 48_000))], None).is_ok());
    // …and one sample more is a silent cut, refused.
    let refused = plan(&args, &[Some(mono(48_001, 48_000))], None).expect_err("cut");
    assert_eq!(refused.code, C_ARGS);
    assert!(
        refused.message.contains("shorter than its clip"),
        "{}",
        refused.message
    );
}

#[test]
fn a_segment_that_shows_no_frame_is_refused() {
    let args = args_of(json!({
        "segments": [
            { "image": "./a.png", "duration_ms": 100 },
            { "image": "./b.png", "duration_ms": 2_000 },
        ],
        "out": "./v.mp4",
        "fps": 1,
    }));
    let refused = plan(&args, &[None, None], None).expect_err("no frame");
    assert_eq!(refused.code, C_ARGS);
    assert!(
        refused
            .message
            .contains("`segments[0]` would show no frame at 1 fps"),
        "{}",
        refused.message
    );
}

#[test]
fn the_video_lasts_at_most_one_hour() {
    let hour = args_of(json!({
        "segments": (0..6).map(|k| json!({ "image": format!("./{k}.png"), "duration_ms": 600_000 })).collect::<Vec<_>>(),
        "out": "./v.mp4",
    }));
    let at = plan(&hour, &[None; 6], None).expect("one hour exactly");
    assert_eq!(at.samples, 3_600 * u64::from(SILENT_RATE));
    // 600,001 ms is above the per-segment bound: build the plan input directly.
    let mut over = hour;
    over.segments[5].hold = Hold::For {
        duration_ms: 600_001,
    };
    let refused = plan(&over, &[None; 6], None).expect_err("over an hour");
    assert_eq!(refused.code, C_BUDGET);
    assert!(
        refused.message.contains("at most one hour"),
        "{}",
        refused.message
    );
}

#[test]
fn the_intermediates_stay_within_one_gibibyte() {
    // (segments + 1) × W × H × 3/2 of frames: 201 × 3840 × 2160 × 1.5 is
    // 2,500,761,600 bytes, over the cap before any PCM.
    assert_eq!(intermediate_bytes(200, (3840, 2160), 0, 0), 2_500_761_600);
    assert_eq!(
        intermediate_bytes(1, (1280, 720), 48_000, 2),
        2 * 1_382_400 + 192_000
    );
    assert_eq!(
        intermediate_bytes(usize::MAX, (3840, 2160), u64::MAX, 2),
        u64::MAX,
        "saturates"
    );
    let still = json!({ "image": "./a.png", "duration_ms": 1_000 });
    let args = args_of(json!({
        "segments": vec![still; 200],
        "out": "./v.mp4",
        "size": "3840x2160",
    }));
    let refused = plan(&args, &vec![None; 200], None).expect_err("over 1 GiB");
    assert_eq!(refused.code, C_BUDGET);
    assert!(
        refused.message.contains("at most 1073741824"),
        "{}",
        refused.message
    );
    let small = args_of(json!({
        "segments": vec![json!({ "image": "./a.png", "duration_ms": 1_000 }); 200],
        "out": "./v.mp4",
    }));
    assert!(
        plan(&small, &vec![None; 200], None).is_ok(),
        "201 frames of 720p fit"
    );
}

#[test]
fn a_longer_soundtrack_is_trimmed_and_a_shorter_one_ends_in_silence() {
    let args = args_of(json!({
        "segments": [{ "image": "./a.png", "duration_ms": 2_000 }],
        "audio": "./t.wav",
        "out": "./v.mp4",
    }));
    let long = plan(&args, &[None], Some(mono(96_001, 48_000))).expect("plans");
    assert!(long.soundtrack_trimmed);
    assert_eq!(
        (long.rate, long.channels, long.samples),
        (48_000, 1, 96_000)
    );
    let short = plan(&args, &[None], Some(mono(10, 44_100))).expect("plans");
    assert!(!short.soundtrack_trimmed);
    assert_eq!((short.rate, short.samples), (44_100, 88_200));
}

#[test]
fn the_facts_match_the_segments_one_to_one() {
    let (args, mut clips) = storyboard();
    clips.pop();
    assert_eq!(
        plan(&args, &clips, None).expect_err("mismatch").code,
        C_ARGS
    );
}

#[test]
fn every_input_answers_to_its_byte_cap() {
    assert!(input_within_cap("segments[0].image", Input::Still, MAX_STILL_BYTES).is_ok());
    let over =
        input_within_cap("segments[0].image", Input::Still, MAX_STILL_BYTES + 1).expect_err("over");
    assert_eq!(over.code, C_BUDGET);
    assert!(
        over.message.contains("512 MiB per still"),
        "{}",
        over.message
    );
    assert!(input_within_cap("audio", Input::Clip, MAX_CLIP_BYTES).is_ok());
    let over = input_within_cap("audio", Input::Clip, MAX_CLIP_BYTES + 1).expect_err("over");
    assert!(
        over.message.contains("2 GiB per clip or soundtrack"),
        "{}",
        over.message
    );
}

#[test]
fn the_scaled_box_is_image_composes_geometry_with_its_caps() {
    let frame = (1280, 720);
    assert_eq!(
        placed_box("s", (400, 300), Scale::Fit, frame).ok(),
        Some((960, 720))
    );
    assert_eq!(
        placed_box("s", (400, 300), Scale::Fill, frame).ok(),
        Some((1280, 960))
    );
    assert_eq!(
        placed_box("s", (400, 300), Scale::None, frame).ok(),
        Some((400, 300))
    );
    let refused =
        placed_box("segments[2].image", (16_384, 1), Scale::Fill, (3840, 2160)).expect_err("cap");
    assert_eq!(refused.code, C_BUDGET);
    assert!(
        refused.message.contains("segments[2].image"),
        "{}",
        refused.message
    );
}

#[test]
fn the_rounding_is_half_up_and_never_divides_by_zero() {
    assert_eq!(round_div(5, 2), 3);
    assert_eq!(round_div(4, 2), 2);
    assert_eq!(round_div(1, 3), 0);
    assert_eq!(round_div(7, 0), 0);
    assert_eq!(frame_at(0, 25, 0), 0);
    assert_eq!(ms_of(22_050, 44_100), 500);
    assert_eq!(samples_of(1, 44_100), 44, "44.1 rounds down");
    assert_eq!(samples_of(10, 22_050), 221, "220.5 rounds up");
    assert_eq!(
        (MAX_ENCODER_ARG_BYTES, MAX_OUTPUT_BYTES),
        (65_536, 268_435_456)
    );
}

/// A random layout: segment holds (a duration, or a clip with its pad), one
/// rate from the MPEG-4 table, mono or stereo clips, 1 to 60 fps.
fn layout() -> impl Strategy<Value = (Vec<(bool, u64, u64)>, u32, u32, bool)> {
    (
        proptest::collection::vec((any::<bool>(), 100_u64..=60_000, 0_u64..=2_000), 1..12),
        proptest::sample::select(MPEG4_RATES.to_vec())
            .prop_filter("a WAV rate", |r| CLIP_RATES.contains(r)),
        1_u32..=60,
        any::<bool>(),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// The timing law: the segments tile the video's samples and frames
    /// exactly, every boundary is within 1/(2f) seconds of its sample, and
    /// the keyframes start every segment and never leave a gap above G.
    #[test]
    fn the_timing_and_keyframe_laws_hold_over_random_layouts(
        (layout, rate, fps, stereo) in layout()
    ) {
        let segments: Vec<Value> = layout
            .iter()
            .enumerate()
            .map(|(k, (clip, ms, pad))| {
                if *clip {
                    json!({ "image": format!("./{k}.png"), "audio": format!("./{k}.wav"), "pad_ms": pad })
                } else {
                    json!({ "image": format!("./{k}.png"), "duration_ms": ms })
                }
            })
            .collect();
        let args = args_of(json!({ "segments": segments, "out": "./v.mp4", "fps": fps, "audio_codec": "flac" }));
        let clips: Vec<Option<ClipFacts>> = layout
            .iter()
            .map(|(clip, ms, _)| clip.then(|| ClipFacts { samples: samples_of(*ms, rate), rate, channels: if stereo { 2 } else { 1 } }))
            .collect();
        match plan(&args, &clips, None) {
            Ok(plan) => {
                let has_clip = clips.iter().any(Option::is_some);
                prop_assert_eq!(plan.rate, if has_clip { rate } else { SILENT_RATE });
                prop_assert_eq!(plan.frames, frame_at(plan.samples, fps, plan.rate));
                let mut start = 0;
                let mut frame = 0;
                for segment in &plan.segments {
                    prop_assert_eq!(segment.start, start, "the segments tile the samples");
                    prop_assert_eq!(segment.first_frame, frame, "and the frames");
                    prop_assert!(segment.frames >= 1);
                    // |S_k/R − F(S_k)/f| ≤ 1/(2f), as integers.
                    let (s, f, r) = (i128::from(segment.start), i128::from(fps), i128::from(plan.rate));
                    prop_assert!((2 * s * f - 2 * i128::from(segment.first_frame) * r).abs() <= r);
                    start += segment.length;
                    frame += segment.frames;
                }
                prop_assert_eq!(start, plan.samples);
                prop_assert_eq!(frame, plan.frames);
                prop_assert_eq!(plan.duration_ms(), ms_of(plan.samples, plan.rate));
                let keys = plan.keyframes();
                prop_assert_eq!(keys.first().copied(), Some(0));
                prop_assert!(keys.windows(2).all(|w| w[0] < w[1] && w[1] - w[0] <= plan.keyframe_interval));
                prop_assert!(keys.iter().all(|k| *k < plan.frames));
                for segment in &plan.segments {
                    prop_assert!(keys.contains(&segment.first_frame), "every segment starts on a keyframe");
                }
            }
            // The one refusal a lawful layout can meet: a segment too short for one frame.
            Err(refused) => {
                prop_assert_eq!(refused.code, C_ARGS);
                prop_assert!(refused.message.contains("would show no frame"), "{}", refused.message);
            }
        }
    }
}
