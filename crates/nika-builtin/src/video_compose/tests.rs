// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! `nika:video_compose` in this build: the resolved arguments parsed
//! against the check's own contract (every default, every refusal, and a
//! differential with the static rules), then the `-010` refusal through the
//! REAL dispatcher route — nothing read, nothing written, nothing witnessed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nika_cap::video::{AudioCodec, Quality, Scale, Transition};
use nika_fs::TokioFs;
use nika_kernel::runtime::tool_executor::{ToolCall, ToolExecuteDyn, ToolResult};
use nika_kernel_mock::{MockClock, MockHttp};
use proptest::prelude::*;
use serde_json::{Map, Value, json};

use super::args::{Hold, MediaRef, Segment, parse};
use super::{C_ARGS, C_BUDGET, C_NO_ENCODER};
use crate::{BuiltinDispatcher, FsBoundary, NoWorkflow, NonInteractive, NullEmitter};

const DIGEST: &str = "bde5666f4a0fab4949c4ca597fce3dd3dbfb55dfd0723f7bc0e01b044c978a29";

fn map(value: Value) -> Map<String, Value> {
    let Value::Object(map) = value else {
        panic!("an args object");
    };
    map
}

fn base() -> Value {
    json!({
        "segments": [{ "image": "./out/frames/a.png", "duration_ms": 2000 }],
        "out": "./out/a.mp4",
    })
}

fn with(key: &str, value: Value) -> Value {
    let mut args = base();
    args[key] = value;
    args
}

/// The refusal's code and message, for a call that must fail.
fn refused(args: Value) -> (&'static str, String) {
    let failure = parse(&map(args)).expect_err("refused");
    (failure.code, failure.message)
}

// ── the parse ─────────────────────────────────────────────────────

#[test]
fn every_default_is_the_contracts() {
    let parsed = parse(&map(base())).expect("parses");
    assert_eq!(
        parsed.segments,
        vec![Segment {
            image: MediaRef {
                path: "./out/frames/a.png".to_owned(),
                sha256: None
            },
            audio: None,
            hold: Hold::For { duration_ms: 2000 },
        }]
    );
    assert_eq!(parsed.soundtrack, None);
    assert_eq!(parsed.audio_codec, AudioCodec::Aac);
    assert_eq!(parsed.out, "./out/a.mp4");
    assert_eq!((parsed.width, parsed.height), (1280, 720));
    assert_eq!(parsed.fps, 25);
    assert_eq!(parsed.scale, Scale::Fit);
    assert_eq!(
        parsed.background,
        [0, 0, 0],
        "black: the letterbox convention"
    );
    assert_eq!(parsed.quality, Quality::Standard);
    assert_eq!(parsed.transition, Transition::Cut);
    assert_eq!(parsed.metadata, None);
    assert_eq!(parsed.timeout_ms, 600_000);
    // A null optional argument is its default, as if absent.
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
        assert_eq!(
            parse(&map(with(key, Value::Null))).expect("parses"),
            parsed,
            "{key}"
        );
    }
}

#[test]
fn every_argument_is_read_at_its_value() {
    let parsed = parse(&map(json!({
        "segments": [
            { "image": "./a.png", "audio": "./a.wav", "pad_ms": 600 },
            {
                "image": { "path": "./b.png", "sha256": DIGEST, "width": 640 },
                "audio": { "path": "./b.wav", "sha256": DIGEST },
            },
            { "image": "./c.png", "audio": "./c.wav", "duration_ms": 9000 },
        ],
        "out": "./out/v.mp4",
        "size": "640x360",
        "fps": 30,
        "scale": "fill",
        "background": [16, 20, 28],
        "quality": "high",
        "audio_codec": "flac",
        "transition": "cut",
        "metadata": { "campaign": "spring" },
        "timeout_ms": 1000,
    })))
    .expect("parses");
    assert_eq!(parsed.segments[0].hold, Hold::Clip { pad_ms: 600 });
    assert_eq!(parsed.segments[1].hold, Hold::Clip { pad_ms: 0 });
    assert_eq!(parsed.segments[2].hold, Hold::For { duration_ms: 9000 });
    assert_eq!(
        parsed.segments[1].image,
        MediaRef {
            path: "./b.png".to_owned(),
            sha256: Some(DIGEST.to_owned())
        },
        "a producer's extra keys are ignored"
    );
    assert_eq!((parsed.width, parsed.height, parsed.fps), (640, 360, 30));
    assert_eq!(
        (parsed.scale, parsed.quality, parsed.audio_codec),
        (Scale::Fill, Quality::High, AudioCodec::Flac)
    );
    assert_eq!(parsed.background, [16, 20, 28]);
    assert_eq!(parsed.metadata, Some(map(json!({ "campaign": "spring" }))));
    assert_eq!(parsed.timeout_ms, 1000);
    let hex = parse(&map(with("background", json!("#10141C")))).expect("parses");
    assert_eq!(hex.background, [16, 20, 28], "hex digits in either case");
    let track = parse(&map(json!({
        "segments": [{ "image": "./a.png", "duration_ms": 500 }],
        "audio": { "path": "./t.wav", "sha256": DIGEST },
        "out": "./v.mp4",
    })))
    .expect("parses");
    assert_eq!(track.soundtrack.map(|t| t.path), Some("./t.wav".to_owned()));
}

/// Every refusal is `-001` naming its argument, except the segment budget:
/// more than 200 is `-005` at run (a literal list is refused at check).
#[test]
fn every_refusal_names_its_argument() {
    let still = json!({ "image": "./a.png", "duration_ms": 500 });
    let (code, message) = refused(with("segments", Value::Array(vec![still.clone(); 201])));
    assert_eq!(code, C_BUDGET);
    assert!(message.contains("201 segments · at most 200"), "{message}");
    assert!(parse(&map(with("segments", Value::Array(vec![still; 200])))).is_ok());
    for (args, needle) in [
        (with("caption", json!("hi")), "unknown argument `caption`"),
        (with("segments", json!([])), "holds no segment"),
        (with("segments", Value::Null), "`segments:` is required"),
        (
            with("segments", json!("./frames")),
            "must be a list of segments",
        ),
        (with("segments", json!([7])), "`segments[0]` must be a map"),
        (
            with(
                "segments",
                json!([{ "image": "./a.png", "duration_ms": 500, "caption": 1 }]),
            ),
            "has no key `caption`",
        ),
        (
            with("segments", json!([{ "duration_ms": 500 }])),
            "requires `image:`",
        ),
        (
            with("segments", json!([{ "image": "./a.png" }])),
            "needs `duration_ms:`",
        ),
        (
            with("segments", json!([{ "image": "./a.png", "pad_ms": 600 }])),
            "needs `duration_ms:`",
        ),
        (
            with(
                "segments",
                json!([{ "image": "./a.png", "audio": "./a.wav", "duration_ms": 900, "pad_ms": 0 }]),
            ),
            "`segments[0].pad_ms` applies only",
        ),
        (
            with(
                "segments",
                json!([{ "image": "./a.png", "duration_ms": 99 }]),
            ),
            "`segments[0].duration_ms: 99` must be an integer 100..=600000",
        ),
        (
            with(
                "segments",
                json!([{ "image": "./a.png", "audio": "./a.wav", "pad_ms": 10_001 }]),
            ),
            "0..=10000",
        ),
        (with("out", json!("./out/a.mov")), "must end in .mp4"),
        (with("out", Value::Null), "`out:` is required"),
    ] {
        let (code, message) = refused(args.clone());
        assert_eq!(code, C_ARGS, "{args}");
        assert!(message.contains(needle), "{needle:?} not in {message:?}");
    }
    let (code, message) = refused(json!({
        "segments": [{ "image": "./a.png", "audio": "./a.wav", "duration_ms": 900 }],
        "audio": "./track.wav",
        "out": "./v.mp4",
    }));
    assert_eq!(code, C_ARGS);
    assert!(
        message.contains("beside the top-level `audio` soundtrack"),
        "{message}"
    );
    assert!(
        parse(&map(with(
            "metadata",
            json!({ "note": "x".repeat(16_373) })
        )))
        .is_ok()
    );
}

/// A media reference is a non-empty path, or an object carrying BOTH a
/// non-empty `path` and a `sha256` of 64 lowercase hex digits.
#[test]
fn every_media_reference_refusal_names_its_rule() {
    for (args, needle) in [
        (
            with("segments", json!([{ "image": "", "duration_ms": 500 }])),
            "must be a non-empty path",
        ),
        (
            with("segments", json!([{ "image": 7, "duration_ms": 500 }])),
            "must be a path or { path, sha256 }",
        ),
        (
            with(
                "segments",
                json!([{ "image": { "path": "./a.png", "sha_256": DIGEST }, "duration_ms": 500 }]),
            ),
            "carries no `sha256`",
        ),
        (
            with(
                "segments",
                json!([{ "image": { "path": "./a.png", "sha256": DIGEST.to_uppercase() }, "duration_ms": 500 }]),
            ),
            "64 lowercase hex digits",
        ),
        (
            with(
                "segments",
                json!([{ "image": { "path": "", "sha256": DIGEST }, "duration_ms": 500 }]),
            ),
            "`segments[0].image.path` must be",
        ),
    ] {
        let (code, message) = refused(args.clone());
        assert_eq!(code, C_ARGS, "{args}");
        assert!(message.contains(needle), "{needle:?} not in {message:?}");
    }
}

/// The scalar arguments refuse every value outside their bound or closed set,
/// with `-001` naming the argument and the rule.
#[test]
fn every_scalar_refusal_names_its_rule() {
    for (args, needle) in [
        (with("size", json!("1279x720")), "both sides even"),
        (with("size", json!("4096x2160")), "width 64..=3840"),
        (
            with("fps", json!(61)),
            "`fps: 61` must be an integer 1..=60",
        ),
        (with("fps", json!(25.0)), "1..=60"),
        (with("timeout_ms", json!(999)), "1000..=3600000"),
        (
            with("scale", json!("stretch")),
            "`scale: stretch` is not in the closed set",
        ),
        (with("quality", json!("ultra")), "draft · standard · high"),
        (with("audio_codec", json!("mp3")), "aac · flac"),
        (
            with("transition", json!("fade")),
            "`transition: fade` is reserved",
        ),
        (
            with("transition", json!("dissolve")),
            "cut (fade is reserved)",
        ),
        (with("background", json!("#12345")), "#rrggbb"),
        (with("background", json!("#+1+2+3")), "#rrggbb"),
        (with("background", json!([0, 0, 256])), "#rrggbb"),
        (
            with("metadata", json!("spring")),
            "`metadata:` must be an object",
        ),
        (
            with("metadata", json!({ "note": "x".repeat(16_374) })),
            "16385 bytes as canonical JSON",
        ),
    ] {
        let (code, message) = refused(args.clone());
        assert_eq!(code, C_ARGS, "{args}");
        assert!(message.contains(needle), "{needle:?} not in {message:?}");
    }
}

// ── check ≡ run ───────────────────────────────────────────────────

/// The catalog row the static check reads its argument set from IS the
/// contract's: the same keys in the same order, `segments` and `out`
/// required.
#[test]
fn the_catalog_row_is_the_contracts_argument_set() {
    let row = nika_catalog::find_builtin("video_compose").expect("a catalog row");
    assert_eq!(row.args, nika_cap::video::ARGS);
    assert_eq!(row.required, ["segments", "out"]);
}

/// The definition a model reads names the encoder's confinement as the
/// contract has it: the engine's jail unless the operator waives it, and
/// never « no network ».
#[test]
fn the_tool_definition_never_overclaims_the_encoder_s_confinement() {
    let def = crate::tool_defs()
        .into_iter()
        .find(|d| d.name == nika_cap::video::TOOL)
        .expect("a definition");
    assert!(
        def.description
            .contains("inside its own confinement unless the operator waives it"),
        "{}",
        def.description
    );
    assert!(
        !def.description.to_lowercase().contains("no network"),
        "{}",
        def.description
    );
}

/// What the static check says about a LITERAL call: its shape rules, then
/// the catalog row's unknown keys and required arguments.
fn check_refuses(args: &Value) -> bool {
    let shape = nika_cap::builtin_shape_findings(nika_cap::video::TOOL, Some(args));
    let row = nika_catalog::find_builtin("video_compose").expect("a catalog row");
    let object = args.as_object();
    let unknown = object.is_some_and(|m| m.keys().any(|k| !row.args.contains(&k.as_str())));
    let missing = row
        .required
        .iter()
        .any(|k| object.is_none_or(|m| !m.contains_key(*k)));
    !shape.is_empty() || unknown || missing
}

/// One literal value per argument, drawn from lawful and refused values
/// (the boundaries included), so the two judges meet on every rule.
fn arg_value(key: &'static str) -> BoxedStrategy<Option<Value>> {
    let pick = |values: Vec<Value>| {
        prop_oneof![Just(None), proptest::sample::select(values).prop_map(Some)].boxed()
    };
    match key {
        "size" => pick(vec![
            json!("1280x720"),
            json!("64x64"),
            json!("3840x2160"),
            json!("1279x720"),
            json!("4096x2160"),
            json!("62x64"),
            json!(1280),
            Value::Null,
        ]),
        "fps" => pick(vec![
            json!(1),
            json!(60),
            json!(0),
            json!(61),
            json!(25.0),
            json!("25"),
            Value::Null,
        ]),
        "timeout_ms" => pick(vec![
            json!(1000),
            json!(3_600_000),
            json!(999),
            json!(3_600_001),
            Value::Null,
        ]),
        "scale" => pick(vec![
            json!("fit"),
            json!("fill"),
            json!("none"),
            json!("stretch"),
            json!(3),
        ]),
        "quality" => pick(vec![json!("draft"), json!("high"), json!("ultra")]),
        "audio_codec" => pick(vec![json!("aac"), json!("flac"), json!("mp3")]),
        "transition" => pick(vec![
            json!("cut"),
            json!("fade"),
            json!("dissolve"),
            json!(1),
        ]),
        "background" => pick(vec![
            json!("#10141c"),
            json!("#10141C"),
            json!([0, 0, 255]),
            json!("#12345"),
            json!([0, 0, 256]),
            json!("black"),
        ]),
        "metadata" => pick(vec![
            json!({ "a": 1 }),
            json!({ "note": "x".repeat(16_373) }),
            json!({ "note": "x".repeat(16_374) }),
            json!("m"),
            json!([]),
        ]),
        "out" => pick(vec![
            json!("./o/v.mp4"),
            json!("./o/v.mov"),
            json!(""),
            json!(7),
            Value::Null,
        ]),
        "audio" => pick(vec![
            json!("./t.wav"),
            json!({ "path": "./t.wav", "sha256": DIGEST }),
            json!({ "path": "./t.wav" }),
            json!(""),
            Value::Null,
        ]),
        _ => Just(None).boxed(),
    }
}

fn media() -> impl Strategy<Value = Value> {
    proptest::sample::select(vec![
        json!("./a.png"),
        json!({ "path": "./a.png", "sha256": DIGEST, "extra": 1 }),
        json!({ "path": "./a.png", "sha256": DIGEST.to_uppercase() }),
        json!({ "path": "./a.png", "sha256": &DIGEST[..63] }),
        json!({ "path": "./a.png" }),
        json!({ "sha256": DIGEST }),
        json!(""),
        json!(7),
    ])
}

fn a_segment() -> impl Strategy<Value = Value> {
    let opt = |values: Vec<Value>| {
        prop_oneof![Just(None), proptest::sample::select(values).prop_map(Some)]
    };
    (
        prop_oneof![Just(None), media().prop_map(Some), Just(Some(Value::Null))],
        prop_oneof![Just(None), media().prop_map(Some), Just(Some(Value::Null))],
        opt(vec![
            json!(100),
            json!(600_000),
            json!(99),
            json!(600_001),
            json!(500.5),
            Value::Null,
        ]),
        opt(vec![
            json!(0),
            json!(10_000),
            json!(10_001),
            json!(-1),
            Value::Null,
        ]),
        prop::bool::weighted(0.1),
    )
        .prop_map(|(image, audio, duration, pad, extra)| {
            let mut seg = Map::new();
            for (key, value) in [
                ("image", image),
                ("audio", audio),
                ("duration_ms", duration),
                ("pad_ms", pad),
            ] {
                if let Some(value) = value {
                    seg.insert(key.to_owned(), value);
                }
            }
            if extra {
                seg.insert("caption".to_owned(), json!("hi"));
            }
            Value::Object(seg)
        })
}

fn literal_call() -> impl Strategy<Value = Value> {
    let keys = [
        "audio",
        "out",
        "size",
        "fps",
        "scale",
        "background",
        "quality",
        "audio_codec",
        "transition",
        "metadata",
        "timeout_ms",
    ];
    let values: Vec<BoxedStrategy<Option<Value>>> = keys.iter().map(|k| arg_value(k)).collect();
    (
        prop_oneof![
            proptest::collection::vec(a_segment(), 0..4).prop_map(|s| Some(Value::Array(s))),
            Just(None),
            Just(Some(json!("./not-a-list"))),
        ],
        values,
        prop::bool::weighted(0.05),
    )
        .prop_map(move |(segments, values, unknown)| {
            let mut args = Map::new();
            if let Some(segments) = segments {
                args.insert("segments".to_owned(), segments);
            }
            for (key, value) in keys.iter().zip(values) {
                if let Some(value) = value {
                    args.insert((*key).to_owned(), value);
                }
            }
            if unknown {
                args.insert("caption".to_owned(), json!("hi"));
            }
            Value::Object(args)
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// check ≡ run on literal calls: the static check refuses a call (its
    /// shape rules, the catalog's unknown keys and required arguments)
    /// exactly when the run's parse refuses it.
    #[test]
    fn the_check_refuses_exactly_what_the_run_refuses(args in literal_call()) {
        let run = parse(args.as_object().expect("an object"));
        prop_assert_eq!(check_refuses(&args), run.is_err(), "{} · run {:?}", args, run.map(|_| ()));
    }
}

// ── the run: -010, nothing touched ────────────────────────────────

type RealFsDispatcher =
    BuiltinDispatcher<TokioFs, MockHttp, MockClock, NullEmitter, NonInteractive, NoWorkflow>;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn scratch() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("nika-video-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn dispatcher(boundary: FsBoundary) -> RealFsDispatcher {
    BuiltinDispatcher::new(
        Arc::new(TokioFs),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(NullEmitter),
        Arc::new(NonInteractive),
        Arc::new(NoWorkflow),
    )
    .with_fs_boundary(boundary)
}

async fn call(d: &RealFsDispatcher, args: Value) -> ToolResult {
    d.execute(ToolCall::new("t", "nika:video_compose", args))
        .await
        .expect("dispatches")
}

fn code(r: &ToolResult) -> String {
    assert!(r.is_error, "expected a coded failure, got {}", r.content);
    r.content.split(" · ").next().unwrap_or_default().to_owned()
}

fn p(root: &Path, rel: &str) -> String {
    root.join(rel).to_string_lossy().into_owned()
}

/// A lawful call refuses `-010` in this build: no input is opened (the
/// stills do not even exist), no boundary is judged (an empty one would
/// have refused), nothing is witnessed and nothing lands at `out`.
#[tokio::test]
async fn a_lawful_call_refuses_no_encoder_before_any_effect() {
    let root = scratch();
    let d = dispatcher(FsBoundary::declared(Vec::new(), Vec::new()));
    let witness = Arc::new(nika_cap::PermitWitness::new());
    let args = json!({
        "segments": [
            { "image": p(&root, "frames/missing.png"), "audio": p(&root, "voice/missing.wav"), "pad_ms": 600 },
            { "image": { "path": p(&root, "frames/b.png"), "sha256": DIGEST }, "duration_ms": 1500 },
        ],
        "out": p(&root, "out/v.mp4"),
    });
    let r = crate::witness::scope_attempt_witness(Arc::clone(&witness), call(&d, args)).await;
    assert_eq!(code(&r), C_NO_ENCODER);
    assert!(r.content.contains("no video backend"), "{}", r.content);
    assert!(
        r.content.contains("nothing was read, staged or written"),
        "{}",
        r.content
    );
    assert!(
        witness.take().is_empty(),
        "no decision is witnessed for a call that touched nothing"
    );
    assert!(!root.join("out").exists(), "nothing was written");
    let entries = std::fs::read_dir(&root).unwrap().count();
    assert_eq!(entries, 0, "no scratch, no artifact");
    let _ = std::fs::remove_dir_all(&root);
}

/// The arguments are judged first: a refused argument is its own code, not
/// the missing encoder.
#[tokio::test]
async fn the_arguments_are_judged_before_the_missing_encoder() {
    let root = scratch();
    let d = dispatcher(FsBoundary::unbounded());
    let r = call(&d, with("transition", json!("fade"))).await;
    assert_eq!(code(&r), C_ARGS);
    assert!(
        r.content.contains("`transition: fade` is reserved"),
        "{}",
        r.content
    );
    let still = json!({ "image": "./a.png", "duration_ms": 500 });
    let r = call(&d, with("segments", Value::Array(vec![still; 201]))).await;
    assert_eq!(code(&r), C_BUDGET);
    let _ = std::fs::remove_dir_all(&root);
}
