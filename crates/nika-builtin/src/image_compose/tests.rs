// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! `nika:image_compose` through the REAL dispatcher route and the real fs:
//! the artifact and its receipts, path-independent bytes, the idempotent
//! skip, the boundary on every input and on `out`, the witness frames, and
//! every coded failure.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nika_fs::TokioFs;
use nika_kernel::runtime::tool_executor::{ToolCall, ToolExecuteDyn, ToolResult};
use nika_kernel_mock::{MockClock, MockHttp};
use serde_json::{Value, json};

use crate::{BuiltinDispatcher, FsBoundary, NoWorkflow, NonInteractive, NullEmitter};

type RealFsDispatcher =
    BuiltinDispatcher<TokioFs, MockHttp, MockClock, NullEmitter, NonInteractive, NoWorkflow>;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A scratch root with four distinct PNGs under `fx/` and one under
/// `private/` (outside every read grant the tests declare).
fn scratch() -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("nika-compose-{}-{n}", std::process::id()));
    std::fs::create_dir_all(root.join("fx")).unwrap();
    std::fs::create_dir_all(root.join("private")).unwrap();
    for (i, (w, h)) in [(9, 7), (5, 11), (12, 4), (6, 6)].into_iter().enumerate() {
        std::fs::write(
            root.join(format!("fx/{i}.png")),
            png(w, h, u8::try_from(i * 60).unwrap()),
        )
        .unwrap();
    }
    std::fs::write(root.join("private/id.png"), png(4, 4, 7)).unwrap();
    root
}

fn png(w: usize, h: usize, seed: u8) -> Vec<u8> {
    let mut px = nika_fx::PixBuf::new(w, h);
    for y in 0..h {
        for x in 0..w {
            px.set(
                x,
                y,
                [
                    u8::try_from(x * 20).unwrap_or(255),
                    u8::try_from(y * 20).unwrap_or(255),
                    seed,
                ],
            );
        }
    }
    nika_fx::codec::png::encode(&px, None)
}

fn p(root: &Path, rel: &str) -> String {
    root.join(rel).to_string_lossy().into_owned()
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

/// Read every `fx/` input, write anything under `out/`.
fn boundary(root: &Path) -> FsBoundary {
    FsBoundary::declared(vec![p(root, "fx/**")], vec![p(root, "out/**")])
}

async fn call(d: &RealFsDispatcher, args: Value) -> ToolResult {
    d.execute(ToolCall::new("t", "nika:image_compose", args))
        .await
        .expect("dispatches")
}

fn sheet_args(root: &Path, out: &str) -> Value {
    json!({
        "inputs": [p(root, "fx/0.png"), p(root, "fx/1.png"), p(root, "fx/2.png"), p(root, "fx/3.png")],
        "out": p(root, out),
        "scale": "fit",
        "cell": { "width": 8, "height": 8 },
        "gap": 2,
        "padding": 3,
        "background": "#10141c",
    })
}

fn code(r: &ToolResult) -> String {
    assert!(r.is_error, "expected a coded failure, got {}", r.content);
    r.content.split(" · ").next().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn a_grid_lands_one_png_whose_bytes_match_its_receipt() {
    let root = scratch();
    let d = dispatcher(boundary(&root));
    let r = call(&d, sheet_args(&root, "out/sheet.png")).await;
    assert!(!r.is_error, "{}", r.content);
    let out = r.structured.expect("a structured receipt");
    assert_eq!(out["path"], p(&root, "out/sheet.png"));
    assert_eq!(
        (out["width"].as_u64(), out["height"].as_u64()),
        (Some(24), Some(24))
    );
    assert_eq!(
        (out["columns"].as_u64(), out["rows"].as_u64()),
        (Some(2), Some(2))
    );
    assert_eq!(out["format"], "png");
    assert_eq!(out["layout"], "grid");
    assert_eq!(out["scale"], "fit");
    assert_eq!(out["skipped_existing"], false);
    let bytes = std::fs::read(root.join("out/sheet.png")).expect("the artifact landed");
    assert_eq!(out["sha256"], nika_fx::det::sha256_hex(&bytes));
    assert_eq!(out["size_bytes"].as_u64(), Some(bytes.len() as u64));
    assert!(
        out.get("png").is_none() && out.get("bytes").is_none(),
        "bytes never ride outputs"
    );
    // Every input's lineage, in order.
    let inputs = out["inputs"].as_array().expect("inputs");
    assert_eq!(inputs.len(), 4);
    for (i, entry) in inputs.iter().enumerate() {
        let src = std::fs::read(root.join(format!("fx/{i}.png"))).unwrap();
        assert_eq!(entry["path"], p(&root, &format!("fx/{i}.png")));
        assert_eq!(entry["sha256"], nika_fx::det::sha256_hex(&src));
    }
    assert_eq!(
        (inputs[1]["width"].as_u64(), inputs[1]["height"].as_u64()),
        (Some(5), Some(11))
    );
    // An INDEPENDENT decoder agrees it is a valid 24×24 RGB8 PNG.
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
    let reader = decoder.read_info().expect("spec-valid png");
    let info = reader.info();
    assert_eq!((info.width, info.height), (24, 24));
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn the_same_bytes_from_another_folder_are_the_same_file() {
    let root = scratch();
    std::fs::create_dir_all(root.join("fx/copy")).unwrap();
    for i in 0..4 {
        std::fs::copy(
            root.join(format!("fx/{i}.png")),
            root.join(format!("fx/copy/{i}.png")),
        )
        .unwrap();
    }
    let d = dispatcher(boundary(&root));
    let first = call(&d, sheet_args(&root, "out/a.png"))
        .await
        .structured
        .expect("receipt");
    let mut moved = sheet_args(&root, "out/b.png");
    moved["inputs"] = json!(
        (0..4)
            .map(|i| p(&root, &format!("fx/copy/{i}.png")))
            .collect::<Vec<_>>()
    );
    let second = call(&d, moved).await.structured.expect("receipt");
    assert_eq!(
        first["sha256"], second["sha256"],
        "no path rides the artifact"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn a_rerun_over_identical_bytes_is_an_idempotent_skip() {
    let root = scratch();
    // The skip needs a byte-equality oracle: a read grant on the output.
    let both = FsBoundary::declared(
        vec![p(&root, "fx/**"), p(&root, "out/**")],
        vec![p(&root, "out/**")],
    );
    let d = dispatcher(both);
    let first = call(&d, sheet_args(&root, "out/sheet.png"))
        .await
        .structured
        .expect("receipt");
    let second = call(&d, sheet_args(&root, "out/sheet.png"))
        .await
        .structured
        .expect("receipt");
    assert_eq!(first["skipped_existing"], false);
    assert_eq!(second["skipped_existing"], true);
    assert_eq!(first["sha256"], second["sha256"]);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn an_input_outside_the_read_boundary_refuses_before_anything_is_read_or_written() {
    let root = scratch();
    let d = dispatcher(boundary(&root));
    let mut args = sheet_args(&root, "out/sheet.png");
    args["inputs"][2] = json!(p(&root, "private/id.png"));
    let r = call(&d, args).await;
    assert_eq!(code(&r), "NIKA-SEC-004");
    assert!(!root.join("out/sheet.png").exists(), "nothing was written");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn an_out_outside_the_write_boundary_is_refused() {
    let root = scratch();
    let d = dispatcher(boundary(&root));
    let r = call(&d, sheet_args(&root, "elsewhere/sheet.png")).await;
    assert_eq!(code(&r), "NIKA-SEC-004");
    assert!(!root.join("elsewhere").exists());
    let _ = std::fs::remove_dir_all(&root);
}

/// Every input read and the artifact write are witnessed as allowed fs
/// decisions — and a write-only `out` is never journaled as a DENIED read of
/// the builtin's own output (the `image_fx` A-F10 wart, not repeated).
#[tokio::test]
async fn the_witness_records_each_read_and_the_write_and_no_denied_self_read() {
    let root = scratch();
    let d = dispatcher(boundary(&root));
    let witness = Arc::new(nika_cap::PermitWitness::new());
    let r = crate::witness::scope_attempt_witness(
        Arc::clone(&witness),
        call(&d, sheet_args(&root, "out/sheet.png")),
    )
    .await;
    assert!(!r.is_error, "{}", r.content);
    let decisions = witness.take();
    assert!(
        decisions.iter().all(|w| w.decision == "allow"),
        "{decisions:?}"
    );
    for i in 0..4 {
        let gate = format!("permits.fs.read {}", p(&root, &format!("fx/{i}.png")));
        assert!(
            decisions.iter().any(|w| w.plane == "fs" && w.gate == gate),
            "{gate} · {decisions:?}"
        );
    }
    let write = format!("permits.fs.write {}", p(&root, "out/sheet.png"));
    assert!(decisions.iter().any(|w| w.gate == write), "{decisions:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn input_failures_carry_their_codes_and_name_the_input() {
    let root = scratch();
    std::fs::write(root.join("fx/not.png"), b"GIF89a this is not a png").unwrap();
    let d = dispatcher(boundary(&root));
    let mut args = sheet_args(&root, "out/s.png");
    args["inputs"][1] = json!(p(&root, "fx/not.png"));
    let r = call(&d, args).await;
    assert_eq!(code(&r), "NIKA-BUILTIN-IMAGE_COMPOSE-003");
    assert!(r.content.contains("inputs[1]"), "{}", r.content);

    let mut args = sheet_args(&root, "out/s.png");
    args["inputs"][3] = json!(p(&root, "fx/missing.png"));
    let r = call(&d, args).await;
    assert_eq!(code(&r), "NIKA-BUILTIN-IMAGE_COMPOSE-002");
    assert!(r.content.contains("inputs[3]"), "{}", r.content);
    let _ = std::fs::remove_dir_all(&root);
}

/// A PNG that is a signature and a valid IHDR only: no pixel data exists,
/// so a `-005` proves the cap fired from the header, before any inflate.
fn header_only(w: u32, h: u32) -> Vec<u8> {
    let mut ihdr = b"IHDR".to_vec();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10];
    out.extend_from_slice(&13u32.to_be_bytes());
    out.extend_from_slice(&ihdr);
    out.extend_from_slice(&crate::media::png::crc32(&ihdr).to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
    out
}

#[tokio::test]
async fn caps_are_a_coded_005_judged_from_the_headers() {
    let root = scratch();
    std::fs::write(root.join("fx/huge.png"), header_only(9_000, 9_000)).unwrap();
    std::fs::write(root.join("fx/long.png"), header_only(10_000, 1)).unwrap();
    let d = dispatcher(boundary(&root));
    let r = call(
        &d,
        json!({ "inputs": [p(&root, "fx/huge.png")], "out": p(&root, "out/a.png") }),
    )
    .await;
    assert_eq!(
        code(&r),
        "NIKA-BUILTIN-IMAGE_COMPOSE-005",
        "one input over 2^26 pixels"
    );
    let r = call(
        &d,
        json!({
            "inputs": [p(&root, "fx/long.png"), p(&root, "fx/long.png")],
            "out": p(&root, "out/b.png"), "layout": "row",
        }),
    )
    .await;
    assert_eq!(
        code(&r),
        "NIKA-BUILTIN-IMAGE_COMPOSE-005",
        "a 20000-wide canvas"
    );
    assert!(r.content.contains("20000x1"), "{}", r.content);
    // ONE input over the per-side cap, far inside the pixel budget: still
    // the cap code, never `-004` (a corrupt file to re-produce).
    std::fs::write(root.join("fx/wide.png"), header_only(20_000, 1)).unwrap();
    let r = call(
        &d,
        json!({ "inputs": [p(&root, "fx/wide.png")], "out": p(&root, "out/c.png") }),
    )
    .await;
    assert_eq!(
        code(&r),
        "NIKA-BUILTIN-IMAGE_COMPOSE-005",
        "an input over 16384 per side: {}",
        r.content
    );
    assert!(r.content.contains("inputs[0]"), "{}", r.content);
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn argument_refusals_are_001_and_read_nothing() {
    let root = scratch();
    let d = dispatcher(boundary(&root));
    let one = p(&root, "fx/0.png");
    let out = p(&root, "out/s.png");
    let many: Vec<String> = (0..65).map(|_| one.clone()).collect();
    for (args, needle) in [
        (
            json!({ "inputs": [one], "out": out, "labels": true }),
            "unknown argument `labels`",
        ),
        (
            json!({ "inputs": [one], "out": p(&root, "out/s.jpg") }),
            "must end in .png",
        ),
        (
            json!({ "inputs": [one], "out": out, "layout": "diagonal" }),
            "`layout: diagonal`",
        ),
        (
            json!({ "inputs": [one, one], "out": out, "layout": "row", "rows": 1 }),
            "`rows:` applies to `layout: grid` only",
        ),
        (
            json!({ "inputs": [one], "out": out, "cell": { "width": 8, "height": 8 } }),
            "scale is none",
        ),
        (json!({ "inputs": [], "out": out }), "holds 0 paths"),
        (json!({ "inputs": many, "out": out }), "holds 65 paths"),
        (json!({ "inputs": [one, 7], "out": out }), "`inputs[1]`"),
        (json!({ "inputs": one, "out": out }), "must be a list"),
        (
            json!({ "inputs": [one], "out": out, "background": "teal" }),
            "#rrggbb",
        ),
        (
            json!({ "inputs": [one], "out": out, "gap": 2000 }),
            "0..=1024",
        ),
        (
            json!({ "inputs": [one, one, one], "out": out, "columns": 1, "rows": 2 }),
            "1x2 grid holds 2 cells",
        ),
    ] {
        let r = call(&d, args).await;
        assert_eq!(code(&r), "NIKA-BUILTIN-IMAGE_COMPOSE-001", "{needle}");
        assert!(
            r.content.contains(needle),
            "`{needle}` not in: {}",
            r.content
        );
    }
    assert!(!root.join("out").exists(), "no refusal ever wrote");
    let _ = std::fs::remove_dir_all(&root);
}
