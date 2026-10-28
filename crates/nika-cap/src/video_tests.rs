// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika:video_compose` contract under test: the vocabulary, the
//! predicates the run shares, the literal shape rules and the read slots.

use super::*;
use crate::{
    BuiltinEffect, SlotPath, builtin_effect, builtin_egresses, builtin_shape_findings,
    read_slot_paths, required_fs_directions, secondary_read_slots, unbound_fs_args,
    write_slot_paths,
};
use serde_json::json;

fn findings(args: &Value) -> Vec<String> {
    builtin_shape_findings(TOOL, Some(args))
}

/// Exactly one finding, returned for content assertions: a rule that fires
/// twice, or a sibling that fires when it should not, is as wrong as silence.
fn only(args: &Value) -> String {
    let out = findings(args);
    assert_eq!(out.len(), 1, "expected exactly one finding, got {out:#?}");
    out.into_iter().next().unwrap_or_default()
}

fn silent(args: &Value) {
    let out = findings(args);
    assert!(out.is_empty(), "expected silence, got {out:#?}");
}

/// A minimal lawful call: one still for two seconds.
fn base() -> Value {
    json!({
        "segments": [{ "image": "./out/frames/a.png", "duration_ms": 2000 }],
        "out": "./out/a.mp4",
    })
}

/// `base()` with `key` set to `value` at the top level.
fn with(key: &str, value: Value) -> Value {
    let mut args = base();
    args[key] = value;
    args
}

/// `base()` whose one segment is `segment`.
fn segment(segment: Value) -> Value {
    with("segments", Value::Array(vec![segment]))
}

const DIGEST: &str = "bde5666f4a0fab4949c4ca597fce3dd3dbfb55dfd0723f7bc0e01b044c978a29";

// ── vocabulary ─────────────────────────────────────────────────────

#[test]
fn the_closed_sets_parse_exactly_their_words() {
    assert_eq!(
        Scale::ALL.map(Scale::as_str),
        ["fit", "fill", "none"],
        "the contract's order"
    );
    assert_eq!(
        Quality::ALL.map(Quality::as_str),
        ["draft", "standard", "high"]
    );
    assert_eq!(AudioCodec::ALL.map(AudioCodec::as_str), ["aac", "flac"]);
    assert_eq!(Transition::ALL.map(Transition::as_str), ["cut"]);
    for word in ["fit", "fill", "none"] {
        assert_eq!(Scale::parse(word).map(Scale::as_str), Some(word));
    }
    for refused in ["stretch", "Fit", "", " fit"] {
        assert_eq!(Scale::parse(refused), None, "{refused:?}");
    }
    assert_eq!(Quality::parse("ultra"), None);
    assert_eq!(AudioCodec::parse("mp3"), None);
    assert_eq!(
        Transition::parse(RESERVED_TRANSITION),
        None,
        "fade is reserved"
    );
    assert_eq!(Transition::parse("dissolve"), None);
}

#[test]
fn the_argument_set_and_the_segment_map_are_closed() {
    assert_eq!(ARGS.len(), 12);
    assert!(ARGS.contains(&"segments") && ARGS.contains(&"out"));
    assert!(!ARGS.contains(&"caption"), "captions are not v1");
    assert_eq!(SEGMENT_KEYS, ["image", "audio", "duration_ms", "pad_ms"]);
}

#[test]
fn the_size_grammar_takes_even_sides_inside_the_bounds() {
    assert_eq!(size("1280x720"), Some((1280, 720)));
    assert_eq!(
        size("64x64"),
        Some((64, 64)),
        "the lower bounds are inclusive"
    );
    assert_eq!(
        size("3840x2160"),
        Some((3840, 2160)),
        "the upper bounds too"
    );
    assert_eq!(
        size("0640x0360"),
        Some((640, 360)),
        "leading zeros are digits"
    );
    for refused in [
        "1279x720",  // odd width
        "1280x721",  // odd height
        "4096x2160", // too wide, even
        "3840x2162", // too tall
        "62x64",     // too narrow
        "64x62",     // too short
        "1280X720",  // the separator is a lowercase x
        "1280x",
        "x720",
        "1280x720x2",
        "-1280x720",
        "+1280x720",
        "1280 x 720",
        "1280x720\n",
        "99999999999x720",
        "",
    ] {
        assert_eq!(size(refused), None, "{refused:?}");
    }
}

#[test]
fn a_digest_is_exactly_64_lowercase_hex_digits() {
    assert!(is_sha256_hex(DIGEST));
    assert!(
        !is_sha256_hex(&DIGEST.to_uppercase()),
        "uppercase is refused"
    );
    assert!(!is_sha256_hex(&DIGEST[..63]), "one digit short");
    assert!(!is_sha256_hex(&format!("{DIGEST}0")), "one digit long");
    assert!(
        !is_sha256_hex(&format!("{}g", &DIGEST[..63])),
        "a non-hex digit"
    );
    assert!(!is_sha256_hex(""));
}

#[test]
fn the_artifact_ends_in_mp4_exactly() {
    assert!(out_is_mp4("./out/a.mp4"));
    assert!(out_is_mp4("a.mp4"));
    for refused in [
        "./out/a.mov",
        "./out/a.MP4",
        "./out/a.mp4.tmp",
        "./out/a",
        "",
    ] {
        assert!(!out_is_mp4(refused), "{refused:?}");
    }
}

/// The cap is measured on canonical JSON: sorted keys, no whitespace,
/// UTF-8 bytes (the spec fixtures' 16384/16385 pair).
#[test]
fn metadata_is_measured_as_canonical_json() {
    let at = |n: usize| json!({ "note": "x".repeat(n) });
    // `{"note":"` is 9 bytes and `"}` 2: 9 + n + 2.
    assert_eq!(metadata_len(&at(16_373)), 16_384);
    assert_eq!(metadata_len(&at(16_374)), 16_385);
    assert_eq!(
        metadata_len(&json!({ "b": 1, "a": [true, null] })),
        r#"{"a":[true,null],"b":1}"#.len()
    );
    assert_eq!(metadata_len(&json!({})), 2);
    // A multi-byte character counts its UTF-8 bytes, raw (never \u-escaped).
    assert_eq!(metadata_len(&json!({ "é": "🦋" })), "{\"é\":\"🦋\"}".len());
    // A control character is escaped, as canonical JSON writes it.
    assert_eq!(metadata_len(&json!({ "a": "\n" })), "{\"a\":\"\\n\"}".len());
}

#[test]
fn a_media_reference_names_its_path() {
    assert_eq!(media_ref_path(&json!("./a.png")), &json!("./a.png"));
    assert_eq!(
        media_ref_path(&json!({ "path": "./a.png", "sha256": DIGEST, "width": 3 })),
        &json!("./a.png")
    );
    assert_eq!(
        media_ref_path(&json!({ "sha256": DIGEST })),
        &Value::Null,
        "an object without a path names none"
    );
    assert_eq!(media_ref_path(&json!(7)), &json!(7));
}

#[test]
fn the_host_program_is_named_without_probing() {
    assert_eq!(FFMPEG, "ffmpeg");
    assert_eq!(FFMPEG_ENV, "NIKA_FFMPEG");
    assert_eq!(
        FFMPEG_CANDIDATES,
        [
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/usr/bin/ffmpeg"
        ]
    );
    assert!(
        FFMPEG_CANDIDATES.iter().all(|c| c.starts_with('/')),
        "PATH is never consulted: every candidate is absolute"
    );
    assert_eq!((FFMPEG_FLOOR, FFMPEG_LIB_FLOOR), ("6.0", 60));
    assert_eq!(H264_ENCODER, "libx264");
}

// ── the literal shape rules ───────────────────────────────────────

/// The spec's valid fixture: string and `{path, sha256}` references (a
/// producer's extra keys ignored), every optional argument at a lawful value.
#[test]
fn the_full_lawful_call_is_silent() {
    silent(&json!({
        "segments": [
            { "image": "./out/frames/scene-1.png", "audio": "./out/voice/scene-1.wav", "pad_ms": 600 },
            {
                "image": { "path": "./out/frames/scene-2.png", "sha256": DIGEST },
                "audio": { "path": "./out/voice/scene-2.wav", "sha256": DIGEST, "duration_ms": 7800 },
                "pad_ms": 600
            },
            { "image": "./out/frames/credits.png", "duration_ms": 3000 }
        ],
        "out": "./out/storyboard.mp4",
        "size": "1280x720",
        "fps": 25,
        "scale": "fit",
        "background": "#000000",
        "quality": "standard",
        "audio_codec": "aac",
        "transition": "cut",
        "metadata": { "campaign": "spring" },
        "timeout_ms": 600_000
    }));
    silent(&base());
    silent(&with("background", json!([16, 20, 28])));
    silent(&with("audio_codec", json!("flac")));
    silent(&with("scale", json!("none")));
    silent(&with("quality", json!("high")));
}

/// A templated value is the run's; a templated whole list, element, path,
/// digest or metadata string never makes a static claim.
#[test]
fn templated_values_defer_to_the_run() {
    silent(&with("segments", json!("${{ with.segments }}")));
    silent(&with(
        "segments",
        json!(["${{ with.first }}", { "image": "./a.png", "duration_ms": 500 }]),
    ));
    silent(&segment(
        json!({ "image": "${{ with.still }}", "duration_ms": "${{ with.hold }}" }),
    ));
    silent(&segment(json!({
        "image": { "path": "${{ with.p }}", "sha256": "${{ with.d }}" },
        "audio": "${{ with.clip }}",
        "pad_ms": "${{ with.pad }}"
    })));
    silent(&with("out", json!("./out/${{ with.name }}")));
    for key in [
        "size",
        "fps",
        "scale",
        "quality",
        "audio_codec",
        "transition",
        "timeout_ms",
    ] {
        silent(&with(key, json!("${{ with.value }}")));
    }
    silent(&with("metadata", json!("${{ with.meta }}")));
    // A template inside the metadata leaves its size to the run, even past the cap.
    silent(&with(
        "metadata",
        json!({ "title": "${{ with.title }}", "pad": "x".repeat(20_000) }),
    ));
}

/// A null optional argument is its default, as if absent.
#[test]
fn a_null_optional_argument_is_its_default() {
    for key in [
        "audio",
        "size",
        "fps",
        "scale",
        "background",
        "quality",
        "audio_codec",
        "transition",
        "metadata",
        "timeout_ms",
    ] {
        silent(&with(key, Value::Null));
    }
}

#[test]
fn the_required_arguments_refuse_null() {
    assert!(only(&with("segments", Value::Null)).contains("`segments:` is required"));
    assert!(only(&with("out", Value::Null)).contains("`out:` must be a non-empty path string"));
    assert!(only(&with("out", json!(""))).contains("`out:` must be a non-empty path string"));
    assert!(only(&with("out", json!(7))).contains("`out:`"));
}

#[test]
fn the_artifact_must_be_an_mp4() {
    let found = only(&with("out", json!("./out/a.mov")));
    assert!(
        found.contains("`out: ./out/a.mov` must end in .mp4"),
        "{found}"
    );
    assert!(
        found.ends_with("(builtins-v0.1.md §nika:video_compose)"),
        "{found}"
    );
    assert!(only(&with("out", json!("./out/a.MP4"))).contains("must end in .mp4"));
}

/// The segment budget's 200/201 boundary (spec fixtures 097 and 098), the
/// empty list (096), and a list that is not a list.
#[test]
fn the_segment_list_holds_one_to_two_hundred_maps() {
    let still = json!({ "image": "./out/frames/a.png", "duration_ms": 500 });
    silent(&with(
        "segments",
        Value::Array(vec![still.clone(); MAX_SEGMENTS]),
    ));
    let over = only(&with(
        "segments",
        Value::Array(vec![still; MAX_SEGMENTS + 1]),
    ));
    assert!(
        over.contains("holds 201 items · the contract is 1..=200"),
        "{over}"
    );
    assert!(only(&with("segments", json!([]))).contains("holds 0 items"));
    assert!(only(&with("segments", json!("./frames"))).contains("must be a list of segments"));
    assert!(only(&with("segments", json!({ "image": "./a.png" }))).contains("must be a list"));
    let element = only(&with("segments", json!([7])));
    assert!(element.contains("`segments[0]` must be a map"), "{element}");
}

/// Each segment is a closed map (spec fixture 092 · a `caption` key).
#[test]
fn a_segment_is_a_closed_map() {
    let found = only(&segment(
        json!({ "image": "./a.png", "duration_ms": 500, "caption": "hi" }),
    ));
    assert!(
        found.contains("`segments[0]` has no key `caption`"),
        "{found}"
    );
    assert!(
        found.contains("image · audio · duration_ms · pad_ms"),
        "{found}"
    );
    // Two unknown keys are two findings, in sorted order.
    let two = findings(&segment(
        json!({ "image": "./a.png", "duration_ms": 500, "z": 1, "b": 2 }),
    ));
    assert_eq!(two.len(), 2, "{two:#?}");
    assert!(two[0].contains("`b`") && two[1].contains("`z`"), "{two:#?}");
}

#[test]
fn every_segment_needs_a_still() {
    assert!(only(&segment(json!({ "duration_ms": 500 }))).contains("requires `image:`"));
    assert!(
        only(&segment(json!({ "image": null, "duration_ms": 500 }))).contains("requires `image:`")
    );
    assert!(
        only(&segment(json!({ "image": "", "duration_ms": 500 })))
            .contains("must be a non-empty path")
    );
    assert!(
        only(&segment(json!({ "image": 7, "duration_ms": 500 })))
            .contains("must be a path or { path, sha256 }")
    );
}

/// The duration rules (spec fixtures 099 to 102): a clipless segment needs
/// `duration_ms`, `pad_ms` is for a clip without one, and both bounds are
/// inclusive.
#[test]
fn the_duration_rules_hold_at_both_ends() {
    let clip = "./out/voice/a.wav";
    let without = only(&segment(json!({ "image": "./a.png" })));
    assert!(
        without.contains("has no clip, so it needs `duration_ms:`"),
        "{without}"
    );
    let padded = only(&segment(
        json!({ "image": "./a.png", "audio": clip, "duration_ms": 4000, "pad_ms": 600 }),
    ));
    assert!(
        padded.contains("`segments[0].pad_ms` applies only"),
        "{padded}"
    );
    // A clipless pad is the missing-duration refusal, once.
    assert!(
        only(&segment(json!({ "image": "./a.png", "pad_ms": 600 })))
            .contains("needs `duration_ms:`")
    );
    silent(&segment(json!({ "image": "./a.png", "audio": clip })));
    silent(&segment(
        json!({ "image": "./a.png", "audio": clip, "pad_ms": 0 }),
    ));
    silent(&segment(
        json!({ "image": "./a.png", "audio": clip, "duration_ms": 4000 }),
    ));
    for (key, lo, hi) in [("duration_ms", 100_u64, 600_000_u64), ("pad_ms", 0, 10_000)] {
        let at = |n: Value| {
            let mut seg = json!({ "image": "./a.png", "audio": clip });
            seg[key] = n;
            segment(seg)
        };
        silent(&at(json!(lo)));
        silent(&at(json!(hi)));
        let above = only(&at(json!(hi + 1)));
        assert!(
            above.contains(&format!(
                "{key}: {}` must be an integer {lo}..={hi}",
                hi + 1
            )),
            "{above}"
        );
        if lo > 0 {
            assert!(only(&at(json!(lo - 1))).contains(&format!("{lo}..={hi}")));
        }
        assert!(
            only(&at(json!(-1))).contains(&format!("{lo}..={hi}")),
            "{key} negative"
        );
        assert!(
            only(&at(json!(500.5))).contains(&format!("{lo}..={hi}")),
            "{key} float"
        );
        assert!(
            only(&at(json!("500"))).contains(&format!("{lo}..={hi}")),
            "{key} string"
        );
    }
}

/// A soundtrack and a clip are one or the other (spec fixture 091).
#[test]
fn a_soundtrack_excludes_the_clips() {
    let found = only(&json!({
        "segments": [{ "image": "./a.png", "audio": "./a.wav", "duration_ms": 2000 }],
        "audio": "./track.wav",
        "out": "./out/a.mp4"
    }));
    assert!(
        found.contains("carries a clip beside the top-level `audio` soundtrack"),
        "{found}"
    );
    silent(&json!({
        "segments": [{ "image": "./a.png", "duration_ms": 2000 }],
        "audio": { "path": "./track.wav", "sha256": DIGEST },
        "out": "./out/a.mp4"
    }));
    // The soundtrack is a media reference under the same rules.
    assert!(only(&with("audio", json!({ "path": "./t.wav" }))).contains("carries no `sha256`"));
}

/// The object form needs BOTH keys, and its digest is 64 lowercase hex
/// digits (spec fixtures 088, 114 and 115).
#[test]
fn a_media_reference_object_binds_its_bytes() {
    let at = |image: Value| segment(json!({ "image": image, "duration_ms": 500 }));
    let misspelt = only(&at(json!({ "path": "./a.png", "sha_256": DIGEST })));
    assert!(misspelt.contains("carries no `sha256`"), "{misspelt}");
    assert!(misspelt.contains("BOTH `path` and `sha256`"), "{misspelt}");
    let neither = only(&at(json!({ "file": "./a.png" })));
    assert!(
        neither.contains("carries no `path` and no `sha256`"),
        "{neither}"
    );
    let upper = only(&at(
        json!({ "path": "./a.png", "sha256": DIGEST.to_uppercase() }),
    ));
    assert!(
        upper.contains("`segments[0].image.sha256` must be 64 lowercase hex"),
        "{upper}"
    );
    assert!(
        only(&at(json!({ "path": "./a.png", "sha256": &DIGEST[..63] })))
            .contains("64 lowercase hex")
    );
    assert!(only(&at(json!({ "path": "./a.png", "sha256": 7 }))).contains("64 lowercase hex"));
    assert!(
        only(&at(json!({ "path": "", "sha256": DIGEST })))
            .contains("`segments[0].image.path` must be")
    );
    assert!(only(&at(json!({ "path": null, "sha256": DIGEST }))).contains(".path` must be"));
    // Both parts wrong: both named.
    assert_eq!(findings(&at(json!({ "path": 1, "sha256": "x" }))).len(), 2);
}

/// The scalar bounds (spec fixtures 090, 103, 104 and 105).
#[test]
fn the_scalar_bounds_are_inclusive() {
    let odd = only(&with("size", json!("1279x720")));
    assert!(
        odd.contains("`size: 1279x720` must be `WIDTHxHEIGHT` with both sides even"),
        "{odd}"
    );
    assert!(only(&with("size", json!("4096x2160"))).contains("width 64..=3840"));
    assert!(only(&with("size", json!(1280))).contains("`size: 1280`"));
    for (key, lo, hi) in [("fps", 1_u64, 60_u64), ("timeout_ms", 1_000, 3_600_000)] {
        silent(&with(key, json!(lo)));
        silent(&with(key, json!(hi)));
        assert!(only(&with(key, json!(hi + 1))).contains(&format!(
            "`{key}: {}` must be an integer {lo}..={hi}",
            hi + 1
        )));
        assert!(only(&with(key, json!(lo - 1))).contains(&format!("{lo}..={hi}")));
        assert!(
            only(&with(key, json!(true))).contains(&format!("{lo}..={hi}")),
            "{key} bool"
        );
    }
}

/// The closed sets (spec fixtures 089, 106 to 109).
#[test]
fn the_closed_sets_refuse_every_other_word() {
    assert!(
        only(&with("scale", json!("stretch")))
            .contains("`scale: stretch` is not in the closed set — fit · fill · none")
    );
    assert!(only(&with("quality", json!("ultra"))).contains("draft · standard · high"));
    assert!(only(&with("audio_codec", json!("mp3"))).contains("aac · flac"));
    assert!(only(&with("scale", json!(3))).contains("`scale: 3`"));
    let fade = only(&with("transition", json!("fade")));
    assert!(fade.contains("`transition: fade` is reserved"), "{fade}");
    let dissolve = only(&with("transition", json!("dissolve")));
    assert!(
        dissolve.contains("`transition: dissolve` is not in the closed set — cut"),
        "{dissolve}"
    );
    assert!(only(&with("background", json!("#12345"))).contains("#rrggbb"));
    assert!(only(&with("background", json!([0, 0, 256]))).contains("#rrggbb"));
}

/// `metadata` is an object of at most 16 KiB as canonical JSON (spec
/// fixtures 110 to 112).
#[test]
fn metadata_is_an_object_within_sixteen_kib() {
    assert!(only(&with("metadata", json!("spring"))).contains("`metadata:` must be an object"));
    assert!(only(&with("metadata", json!([1]))).contains("must be an object"));
    silent(&with("metadata", json!({ "note": "x".repeat(16_373) })));
    let over = only(&with("metadata", json!({ "note": "x".repeat(16_374) })));
    assert!(
        over.contains("is 16385 bytes as canonical JSON · at most 16384"),
        "{over}"
    );
}

/// Unknown top-level keys and absent required arguments are the catalog
/// row's refusals: the shape rules never report them twice.
#[test]
fn the_catalog_row_owns_unknown_keys_and_absent_arguments() {
    silent(&with("caption", json!("hello")));
    silent(&json!({ "segments": [{ "image": "./a.png", "duration_ms": 500 }] }));
    silent(&json!({ "out": "./out/a.mp4" }));
    assert!(builtin_shape_findings(TOOL, None).is_empty());
    assert!(builtin_shape_findings(TOOL, Some(&json!("args"))).is_empty());
}

// ── the effect table ──────────────────────────────────────────────

/// The primary effect is the `out` write; every media reference is a
/// STATIC read slot (never left to the run); the call needs both sides.
#[test]
fn the_call_writes_out_and_reads_every_media_reference() {
    assert_eq!(
        builtin_effect(TOOL, None),
        Some(BuiltinEffect::Fs {
            path_arg: "out",
            reads: false,
            writes: true,
            recursive: false,
            walk_root: false,
        })
    );
    assert_eq!(required_fs_directions(TOOL), Some((true, true)));
    assert!(builtin_egresses(TOOL), "a write is egress");
    let args = json!({
        "segments": [
            { "image": "./a.png", "audio": { "path": "./a.wav", "sha256": DIGEST }, "pad_ms": 600 },
            "${{ with.second }}",
            { "image": { "sha256": DIGEST }, "audio": null, "duration_ms": 500 },
            7
        ],
        "audio": "./track.wav",
        "out": "./out/v.mp4"
    });
    let slots: Vec<(String, Value)> = secondary_read_slots(TOOL, Some(&args))
        .into_iter()
        .map(|(name, value)| (name, value.clone()))
        .collect();
    assert_eq!(
        slots,
        vec![
            ("segments[0].image".to_owned(), json!("./a.png")),
            ("segments[0].audio".to_owned(), json!("./a.wav")),
            ("segments[1]".to_owned(), json!("${{ with.second }}")),
            ("segments[2].image".to_owned(), Value::Null),
            ("segments[3]".to_owned(), json!(7)),
            ("audio".to_owned(), json!("./track.wav")),
        ]
    );
    // A whole templated list is ONE hidden slot, never an empty read set.
    let templated = json!({ "segments": "${{ with.segments }}", "out": "./out/v.mp4" });
    let one = secondary_read_slots(TOOL, Some(&templated));
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].0, "segments");
    assert!(secondary_read_slots(TOOL, None).is_empty());
    assert!(secondary_read_slots(TOOL, Some(&json!({ "out": "./v.mp4" }))).is_empty());
}

/// The file channel follows a file a video reads and the file it writes,
/// from the same slot list the binding judges.
#[test]
fn the_slots_project_each_read_and_the_write() {
    let literal = |path: &str| SlotPath::Literal {
        path: path.to_owned(),
        descends: false,
    };
    let args = json!({
        "segments": [
            { "image": "./a.png", "audio": "${{ with.clip }}" },
            { "image": { "path": "./b.png", "sha256": DIGEST }, "duration_ms": 500 }
        ],
        "out": "./out/v.mp4"
    });
    assert_eq!(
        read_slot_paths(TOOL, Some(&args)),
        [literal("./a.png"), SlotPath::Unknown, literal("./b.png")]
    );
    assert_eq!(
        write_slot_paths(TOOL, Some(&args)),
        [literal("./out/v.mp4")]
    );
    let hidden = json!({ "segments": "${{ with.segments }}", "out": "./out/v.mp4" });
    assert_eq!(read_slot_paths(TOOL, Some(&hidden)), [SlotPath::Unknown]);
}

#[test]
fn every_media_path_and_out_bind_to_the_stated_reach() {
    let stated = |paths: &[&str]| paths.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
    let reads = stated(&["./a.png", "./a.wav", "./track.wav"]);
    let writes = stated(&["./out/v.mp4"]);
    let lawful = json!({
        "segments": [{ "image": "a.png", "audio": { "path": "./a.wav", "sha256": DIGEST } }],
        "audio": "./track.wav",
        "out": "./out/v.mp4"
    });
    assert_eq!(
        unbound_fs_args(TOOL, Some(&lawful), &reads, &writes),
        Vec::<String>::new()
    );
    let stray = json!({
        "segments": [{ "image": "./a.png", "audio": "./zq-canary.wav" }],
        "out": "./out/v.mp4"
    });
    let found = unbound_fs_args(TOOL, Some(&stray), &reads, &writes);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("`segments[0].audio`") && found[0].contains("its reads"),
        "{found:?}"
    );
    assert!(
        !found[0].contains("zq-canary"),
        "the value is never repeated"
    );
    let hidden = json!({ "segments": "${{ with.segments }}", "out": "./out/v.mp4" });
    let found = unbound_fs_args(TOOL, Some(&hidden), &reads, &writes);
    assert!(
        found
            .iter()
            .any(|f| f.contains("`segments` must be a list of segment maps")),
        "{found:?}"
    );
    let escaping = json!({ "segments": [{ "image": "./a.png", "duration_ms": 500 }], "out": "./elsewhere.mp4" });
    let found = unbound_fs_args(TOOL, Some(&escaping), &reads, &writes);
    assert!(found.iter().any(|f| f.contains("`out`")), "{found:?}");
    // A read is never satisfied by a write grant.
    let swapped = json!({ "segments": [{ "image": "./out/v.mp4", "duration_ms": 500 }], "out": "./out/v.mp4" });
    let found = unbound_fs_args(TOOL, Some(&swapped), &reads, &writes);
    assert!(
        found.iter().any(|f| f.contains("`segments[0].image`")),
        "{found:?}"
    );
}
