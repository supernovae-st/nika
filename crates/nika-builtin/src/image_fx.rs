//! `nika:image_fx` — deterministic artistic image effects (stdlib §Media ·
//! deferred-media graduate: the `image editing` row · FX master plan).
//!
//! Pure-compute sibling of `image_generate`: NO provider plane, NO keys,
//! NO clock — fs + boundary + emitter only. The pixel work is `nika-fx`
//! (zero-dep · byte-deterministic); it runs on the blocking pool (the jq
//! precedent — model-controlled CPU cost must not starve the executor).
//!
//! The disk law is inherited verbatim: the artifact lands at `out:`
//! (boundary-gated per final path), outputs carry paths + sha256, bytes
//! never ride outputs. Identical bytes already on disk = idempotent skip.

use nika_fx::{
    AsciiEmit, DitherMode, FxArgs, FxError, Op, Palette, PresetPalette, ResizeFilter, ScreenAngle,
};
use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};
use serde_json::{Value, json};
use std::path::Path;

use crate::permits::{FsAccess, FsBoundary};
use crate::{Args, BuiltinFailure, BuiltinOutcome, Emitter};

const C_ARGS: &str = "NIKA-BUILTIN-IMAGE_FX-001";
const C_INPUT: &str = "NIKA-BUILTIN-IMAGE_FX-002";
const C_FORMAT: &str = "NIKA-BUILTIN-IMAGE_FX-003";
const C_DECODE: &str = "NIKA-BUILTIN-IMAGE_FX-004";
const C_BUDGET: &str = "NIKA-BUILTIN-IMAGE_FX-005";
const C_SAVE: &str = "NIKA-BUILTIN-IMAGE_FX-006";

fn fail(code: &'static str, message: impl Into<String>) -> BuiltinFailure {
    BuiltinFailure::new(code, message)
}

/// Run the builtin: parse → gate input read → transform (blocking pool) →
/// gate + save artifact → outputs `{path, sha256, dims, ops_applied}`.
pub(crate) async fn run<F>(
    fs: &F,
    emitter: &dyn Emitter,
    boundary: &FsBoundary,
    args: &Args,
) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
{
    let parsed = parse(args)?;
    emitter.emit(
        "image_fx.started",
        json!({ "input": parsed.input, "out": parsed.out, "ops": parsed.fx.ops.len(), "seed": parsed.fx.seed }),
    );
    match execute(fs, boundary, &parsed).await {
        Ok(output) => {
            emitter.emit(
                "image_fx.completed",
                json!({
                    "path": output["path"],
                    "sha256_8": output["sha256"].as_str().map(|s| &s[..8.min(s.len())]),
                    "size_bytes": output["size_bytes"],
                    "ops_applied": output["ops_applied"],
                }),
            );
            Ok(output)
        }
        Err(failure) => {
            emitter.emit(
                "image_fx.error",
                json!({ "code": failure.code, "message": failure.message }),
            );
            Err(failure)
        }
    }
}

#[derive(Debug)]
struct Parsed {
    input: String,
    out: String,
    fx: FxArgs,
}

async fn execute<F>(fs: &F, boundary: &FsBoundary, parsed: &Parsed) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
{
    boundary.enforce(fs, &parsed.input, FsAccess::Read).await?;
    let bytes = fs
        .read(Path::new(&parsed.input))
        .await
        .map_err(|e| fail(C_INPUT, format!("reading input `{}`: {e}", parsed.input)))?;

    // Pixel work on the blocking pool (the jq/ocr precedent).
    let fx_args = parsed.fx.clone();
    // The recipe carries the ALGORITHM contract tag (stable across engine
    // releases — re-render stays byte-exact until the fx contract majors).
    // WHICH binary ran lives in the trace/manifest planes.
    let engine = nika_fx::CONTRACT_TAG;
    // `Bytes` is a cheap refcount clone (no memcpy) + `'static` for the pool.
    let input_bytes = bytes.clone();
    let result =
        tokio::task::spawn_blocking(move || nika_fx::transform(&input_bytes, &fx_args, engine))
            .await
            .map_err(|e| fail(C_DECODE, format!("transform task failed: {e}")))?;
    let output = result.map_err(map_fx_error)?;

    let (artifact_bytes, format): (Vec<u8>, &str) = match output.artifact {
        nika_fx::Artifact::Png(b) => (b, "png"),
        nika_fx::Artifact::Text { body, ext } => (body.into_bytes(), ext),
        _ => {
            return Err(fail(
                C_DECODE,
                "unknown artifact kind (engine newer than wiring)",
            ));
        }
    };
    let sha256 = nika_fx::det::sha256_hex(&artifact_bytes);

    boundary.enforce(fs, &parsed.out, FsAccess::Write).await?;
    let out_path = Path::new(&parsed.out);
    // Idempotent skip: identical bytes already on disk. The skip-read of the
    // OUTPUT path is gated on a READ grant for it — without one we never read
    // (no byte-equality oracle under a write-only boundary), we just re-write.
    let skipped = if boundary
        .enforce(fs, &parsed.out, FsAccess::Read)
        .await
        .is_ok()
    {
        matches!(
            fs.read(out_path).await,
            Ok(existing) if existing.as_ref() == artifact_bytes.as_slice()
        )
    } else {
        false
    };
    if !skipped {
        if let Some(parent) = out_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs.create_dir_all(parent)
                .await
                .map_err(|e| fail(C_SAVE, format!("creating `{}`: {e}", parent.display())))?;
        }
        fs.write(out_path, &artifact_bytes)
            .await
            .map_err(|e| fail(C_SAVE, format!("writing `{}`: {e}", parsed.out)))?;
    }

    Ok(json!({
        "input": parsed.input,
        "input_sha256": output.input_sha256,
        "path": parsed.out,
        "sha256": sha256,
        "size_bytes": artifact_bytes.len(),
        "width": output.width,
        "height": output.height,
        "format": format,
        "ops_applied": output.ops_applied,
        "seed": parsed.fx.seed,
        "skipped_existing": skipped,
    }))
}

fn map_fx_error(e: FxError) -> BuiltinFailure {
    match e {
        FxError::Args(m) => fail(C_ARGS, m),
        FxError::UnsupportedFormat(m) => fail(C_FORMAT, m),
        FxError::Decode(m) => fail(C_DECODE, m),
        FxError::Budget { pixels, max } => fail(
            C_BUDGET,
            format!("decoded-pixel budget exceeded: {pixels} > {max}"),
        )
        .with_details(json!({ "pixels": pixels, "max": max })),
        FxError::Encode(m) => fail(C_SAVE, m),
        _ => fail(C_DECODE, format!("unmapped fx error: {e}")),
    }
}

// ── args parsing (pure · pre-I/O · every rejection = -001) ────────────────

fn parse(args: &Args) -> Result<Parsed, BuiltinFailure> {
    let input = required_str(args, "input")?;
    let out = required_str(args, "out")?;
    let ops_value = args
        .get("ops")
        .ok_or_else(|| fail(C_ARGS, "`ops:` is required (an ordered list of effect ops)"))?;
    let ops_list = ops_value
        .as_array()
        .ok_or_else(|| fail(C_ARGS, "`ops:` must be a list of single-key op maps"))?;
    let mut ops = Vec::with_capacity(ops_list.len());
    for (i, entry) in ops_list.iter().enumerate() {
        ops.push(parse_op(entry).map_err(|m| fail(C_ARGS, format!("ops[{i}]: {m}")))?);
    }
    let seed = match args.get("seed") {
        None => 0,
        Some(v) => v
            .as_u64()
            .ok_or_else(|| fail(C_ARGS, "`seed:` must be a non-negative integer"))?,
    };
    let fx = FxArgs::new(ops, seed);
    nika_fx::args::validate(&fx).map_err(map_fx_error)?;
    check_out_extension(&fx, &out)?;
    Ok(Parsed { input, out, fx })
}

/// The artifact kind is decided by the pipeline (ascii emit) — the `out:`
/// extension must agree, so authors never get a .png that is secretly text.
fn check_out_extension(fx: &FxArgs, out: &str) -> Result<(), BuiltinFailure> {
    let expected = match fx.ops.last() {
        Some(Op::Ascii {
            emit: AsciiEmit::Text,
            ..
        }) => "txt",
        Some(Op::Ascii {
            emit: AsciiEmit::Ansi,
            ..
        }) => "ans",
        _ => "png",
    };
    if Path::new(out).extension().and_then(|e| e.to_str()) == Some(expected) {
        Ok(())
    } else {
        Err(fail(
            C_ARGS,
            format!("`out:` must end in .{expected} for this pipeline (got `{out}`)"),
        ))
    }
}

fn required_str(args: &Args, key: &str) -> Result<String, BuiltinFailure> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| fail(C_ARGS, format!("`{key}:` is required (non-empty string)")))
}

fn parse_op(entry: &Value) -> Result<Op, String> {
    let map = entry
        .as_object()
        .filter(|m| m.len() == 1)
        .ok_or("each op is a single-key map, e.g. `- dither: {mode: bayer4, palette: gameboy}`")?;
    let (name, body) = map.iter().next().ok_or("empty op map")?;
    check_known_keys(name, body)?;
    match name.as_str() {
        "resize" => parse_resize(body),
        "crop" => Ok(Op::Crop {
            x: get_u32(body, "x")?.unwrap_or(0),
            y: get_u32(body, "y")?.unwrap_or(0),
            width: req_u32(body, "width")?,
            height: req_u32(body, "height")?,
        }),
        "levels" => Ok(Op::Levels {
            // nika-fx validate() range-checks (-255..=255 / -128..=128) —
            // saturating keeps the wiring honest on absurd inputs.
            brightness: i32::try_from(get_i64(body, "brightness")?.unwrap_or(0))
                .unwrap_or(i32::MAX),
            contrast: i32::try_from(get_i64(body, "contrast")?.unwrap_or(0)).unwrap_or(i32::MAX),
        }),
        "grayscale" => Ok(Op::Grayscale),
        "palette_map" => Ok(Op::PaletteMap {
            palette: parse_palette(body)?,
        }),
        "dither" => Ok(Op::Dither {
            mode: parse_dither_mode(body)?,
            palette: parse_palette(body)?,
        }),
        "duotone" => Ok(Op::Duotone {
            dark: req_rgb(body, "dark")?,
            light: req_rgb(body, "light")?,
        }),
        "pixelate" => Ok(Op::Pixelate {
            block: req_u32(body, "block")?,
        }),
        "halftone" => Ok(Op::Halftone {
            cell: get_u32(body, "cell")?.unwrap_or(8),
            angle: parse_angle(body)?,
        }),
        "grain" => Ok(Op::Grain {
            intensity: get_u32(body, "intensity")?.unwrap_or(48),
        }),
        "vignette" => Ok(Op::Vignette {
            strength: get_u32(body, "strength")?.unwrap_or(160),
        }),
        "chromatic_aberration" => Ok(Op::ChromaticAberration {
            shift: get_u32(body, "shift")?.unwrap_or(4),
        }),
        "scanlines" => Ok(Op::Scanlines {
            strength: get_u32(body, "strength")?.unwrap_or(96),
            period: get_u32(body, "period")?.unwrap_or(4),
        }),
        "glitch" => Ok(Op::Glitch {
            line_shift: get_u32(body, "line_shift")?.unwrap_or(0),
            channel_shift: get_u32(body, "channel_shift")?.unwrap_or(0),
            blocks: get_u32(body, "blocks")?.unwrap_or(0),
        }),
        "ascii" => Ok(Op::Ascii {
            cols: get_u32(body, "cols")?.unwrap_or(96),
            emit: parse_ascii_emit(body)?,
        }),
        other => Err(format!(
            "unknown op `{other}` (v1 ops: resize · crop · levels · grayscale · \
             palette_map · dither · duotone · pixelate · halftone · grain · vignette · \
             chromatic_aberration · scanlines · glitch · ascii)"
        )),
    }
}

/// Closed per-op key sets — an unknown parameter is rejected loudly
/// (`-001`), never silently ignored (a typo'd `intensty:` would otherwise
/// fall back to the default and LIE about the style).
fn check_known_keys(op: &str, body: &Value) -> Result<(), String> {
    let allowed: &[&str] = match op {
        "resize" => &["width", "height", "filter"],
        "crop" => &["x", "y", "width", "height"],
        "levels" => &["brightness", "contrast"],
        "grayscale" => &[],
        "palette_map" => &["palette"],
        "dither" => &["mode", "palette"],
        "duotone" => &["dark", "light"],
        "pixelate" => &["block"],
        "halftone" => &["cell", "angle"],
        "grain" => &["intensity"],
        "vignette" => &["strength"],
        "chromatic_aberration" => &["shift"],
        "scanlines" => &["strength", "period"],
        "glitch" => &["line_shift", "channel_shift", "blocks"],
        "ascii" => &["cols", "emit"],
        _ => return Ok(()), // unknown op — the match below owns that error
    };
    if let Some(map) = body.as_object() {
        for key in map.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(format!(
                    "{op}: unknown parameter `{key}` (accepted: {})",
                    if allowed.is_empty() {
                        "none".to_owned()
                    } else {
                        allowed.join(" · ")
                    }
                ));
            }
        }
    }
    Ok(())
}

fn parse_resize(body: &Value) -> Result<Op, String> {
    let filter = match get_str(body, "filter")? {
        None | Some("auto") => ResizeFilter::Auto,
        Some("nearest") => ResizeFilter::Nearest,
        Some("bilinear") => ResizeFilter::Bilinear,
        Some(other) => return Err(format!("filter `{other}` (auto | nearest | bilinear)")),
    };
    Ok(Op::Resize {
        width: get_u32(body, "width")?,
        height: get_u32(body, "height")?,
        filter,
    })
}

fn parse_dither_mode(body: &Value) -> Result<DitherMode, String> {
    match get_str(body, "mode")? {
        None | Some("floyd_steinberg") => Ok(DitherMode::FloydSteinberg),
        Some("bayer2") => Ok(DitherMode::Bayer2),
        Some("bayer4") => Ok(DitherMode::Bayer4),
        Some("bayer8") => Ok(DitherMode::Bayer8),
        Some("blue_noise") => Ok(DitherMode::BlueNoise),
        Some("ign") => Ok(DitherMode::Ign),
        Some("atkinson") => Ok(DitherMode::Atkinson),
        Some("jjn") => Ok(DitherMode::Jjn),
        Some(other) => Err(format!(
            "mode `{other}` (bayer2|bayer4|bayer8|blue_noise|ign|floyd_steinberg|atkinson|jjn)"
        )),
    }
}

fn parse_angle(body: &Value) -> Result<ScreenAngle, String> {
    match get_i64(body, "angle")? {
        None | Some(45) => Ok(ScreenAngle::A45),
        Some(0) => Ok(ScreenAngle::A0),
        Some(15) => Ok(ScreenAngle::A15),
        Some(75) => Ok(ScreenAngle::A75),
        Some(other) => Err(format!("angle {other} (0 | 15 | 45 | 75)")),
    }
}

fn parse_ascii_emit(body: &Value) -> Result<AsciiEmit, String> {
    match get_str(body, "emit")? {
        None | Some("png") => Ok(AsciiEmit::Png),
        Some("text") => Ok(AsciiEmit::Text),
        Some("ansi") => Ok(AsciiEmit::Ansi),
        Some(other) => Err(format!("emit `{other}` (png | text | ansi)")),
    }
}

fn parse_palette(body: &Value) -> Result<Palette, String> {
    let Some(v) = body.get("palette") else {
        return Ok(Palette::Preset(PresetPalette::Bw));
    };
    if let Some(name) = v.as_str() {
        return match name {
            "bw" => Ok(Palette::Preset(PresetPalette::Bw)),
            "gray4" => Ok(Palette::Preset(PresetPalette::Gray4)),
            "gameboy" => Ok(Palette::Preset(PresetPalette::Gameboy)),
            "cga" => Ok(Palette::Preset(PresetPalette::Cga)),
            "okabe_ito" => Ok(Palette::Preset(PresetPalette::OkabeIto)),
            other => Err(format!(
                "palette `{other}` (bw | gray4 | gameboy | cga | okabe_ito | inline list)"
            )),
        };
    }
    if let Some(list) = v.as_array() {
        let mut colors = Vec::with_capacity(list.len());
        for c in list {
            colors.push(parse_color(c)?);
        }
        return Ok(Palette::Custom(colors));
    }
    Err("palette is a preset name or a list of colors".into())
}

fn req_rgb(body: &Value, key: &str) -> Result<[u8; 3], String> {
    body.get(key)
        .ok_or(format!("`{key}:` is required"))
        .and_then(parse_color)
}

/// A color is `"#rrggbb"` or `[r, g, b]`.
fn parse_color(v: &Value) -> Result<[u8; 3], String> {
    if let Some(hex) = v.as_str() {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        // ASCII guard BEFORE any byte-slice: `hex.len()==6` is a BYTE count;
        // a multi-byte char (e.g. "€abc") is 6 bytes / 4 chars and would slice
        // mid-codepoint → panic. ASCII-only makes byte-len == char-len.
        if hex.len() == 6 && hex.is_ascii() {
            let parse = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
            if let (Ok(r), Ok(g), Ok(b)) = (parse(0), parse(2), parse(4)) {
                return Ok([r, g, b]);
            }
        }
        return Err(format!("color `{v}` (want #rrggbb or [r,g,b])"));
    }
    if let Some(arr) = v.as_array()
        && arr.len() == 3
    {
        let ch = |i: usize| -> Result<u8, String> {
            arr[i]
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| "color channels are integers 0..=255".to_string())
        };
        return Ok([ch(0)?, ch(1)?, ch(2)?]);
    }
    Err(format!("color `{v}` (want #rrggbb or [r,g,b])"))
}

fn get_str<'a>(body: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("`{key}:` must be a string")),
    }
}

fn get_u32(body: &Value, key: &str) -> Result<Option<u32>, String> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None), // explicit null = omitted (recipe round-trip)
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("`{key}:` must be a non-negative integer")),
    }
}

fn get_i64(body: &Value, key: &str) -> Result<Option<i64>, String> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("`{key}:` must be an integer")),
    }
}

fn req_u32(body: &Value, key: &str) -> Result<u32, String> {
    get_u32(body, key)?.ok_or_else(|| format!("`{key}:` is required"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn args(json: &serde_json::Value) -> Args {
        json.as_object().expect("test args object").clone()
    }

    #[test]
    fn parse_accepts_the_cookbook_shape() {
        let a = args(&json!({
            "input": "out/hero.png",
            "out": "out/hero-styled.png",
            "seed": 42,
            "ops": [
                { "resize": { "width": 320 } },
                { "dither": { "mode": "bayer4", "palette": "gameboy" } },
                { "grain": { "intensity": 32 } },
            ],
        }));
        let p = parse(&a).expect("parse");
        assert_eq!(p.fx.ops.len(), 3);
        assert_eq!(p.fx.seed, 42);
    }

    #[test]
    fn parse_rejects_unknown_op_with_the_menu() {
        let a = args(&json!({
            "input": "a.png", "out": "b.png",
            "ops": [ { "sharpen": {} } ],
        }));
        let e = parse(&a).unwrap_err();
        assert_eq!(e.code, C_ARGS);
        assert!(e.message.contains("unknown op `sharpen`"));
        assert!(e.message.contains("dither"));
    }

    #[test]
    fn parse_rejects_extension_mismatch() {
        let a = args(&json!({
            "input": "a.png", "out": "b.png",
            "ops": [ { "ascii": { "cols": 80, "emit": "text" } } ],
        }));
        let e = parse(&a).unwrap_err();
        assert!(e.message.contains(".txt"), "{}", e.message);
    }

    #[test]
    fn parse_accepts_inline_palettes_hex_and_rgb() {
        let a = args(&json!({
            "input": "a.png", "out": "b.png",
            "ops": [ { "dither": { "palette": ["#0f380f", [155, 188, 15]] } } ],
        }));
        let p = parse(&a).expect("parse");
        let Op::Dither {
            palette: Palette::Custom(c),
            ..
        } = &p.fx.ops[0]
        else {
            panic!("expected custom palette");
        };
        assert_eq!(c[0], [15, 56, 15]);
        assert_eq!(c[1], [155, 188, 15]);
    }

    #[test]
    fn parse_rejects_bad_seed_and_missing_ops() {
        let bad_seed = args(&json!({
            "input": "a.png", "out": "b.png", "seed": -4,
            "ops": [ { "grayscale": {} } ],
        }));
        assert_eq!(parse(&bad_seed).unwrap_err().code, C_ARGS);
        let no_ops = args(&json!({ "input": "a.png", "out": "b.png" }));
        assert!(parse(&no_ops).unwrap_err().message.contains("`ops:`"));
    }

    // ── refuter regressions (2026-07-09 wave 2) ──────────────────────────

    #[test]
    fn parse_color_rejects_multibyte_hex_without_panicking() {
        // "#€abc" is 6 BYTES / 4 chars — the naive `&hex[i..i+2]` slice would
        // panic mid-codepoint. The ASCII guard must turn it into a clean Err.
        let a = args(&json!({
            "input": "a.png", "out": "b.png",
            "ops": [ { "duotone": { "dark": "#€abc", "light": "#000000" } } ],
        }));
        let e = parse(&a).unwrap_err();
        assert_eq!(e.code, C_ARGS);
    }

    #[test]
    fn parse_accepts_recipe_null_height_as_omitted() {
        // The recipe emitter prints `"height":null` for an aspect-preserving
        // resize — re-parsing that exact shape MUST treat null as absent, or
        // the "re-render IS the tamper check" promise breaks.
        let a = args(&json!({
            "input": "a.png", "out": "b.png",
            "ops": [ { "resize": { "width": 160, "height": null, "filter": "auto" } } ],
        }));
        let p = parse(&a).expect("null height reparses");
        let Op::Resize { width, height, .. } = &p.fx.ops[0] else {
            panic!("expected resize");
        };
        assert_eq!(*width, Some(160));
        assert_eq!(*height, None);
    }

    #[test]
    fn parse_rejects_unknown_op_parameter() {
        // A typo'd sub-key must fail loudly (-001), never silently default —
        // a silent default LIES about the style ("the seed IS the style").
        let a = args(&json!({
            "input": "a.png", "out": "b.png",
            "ops": [ { "grain": { "intensty": 90 } } ],
        }));
        let e = parse(&a).unwrap_err();
        assert_eq!(e.code, C_ARGS);
        assert!(e.message.contains("intensty"), "{}", e.message);
    }

    // ── full dispatch-path coverage (house-review P0) ────────────────────

    fn tiny_png() -> Vec<u8> {
        let mut img = nika_fx::PixBuf::new(8, 6);
        for y in 0..6 {
            for x in 0..8 {
                img.set(
                    x,
                    y,
                    [
                        u8::try_from(x * 30).unwrap_or(255),
                        u8::try_from(y * 40).unwrap_or(255),
                        128,
                    ],
                );
            }
        }
        nika_fx::codec::png::encode(&img, None)
    }

    #[tokio::test]
    async fn dispatch_writes_styled_png_with_hashes() {
        use nika_kernel_mock::MockFs;
        let fs = MockFs::new().with_file("./in.png", tiny_png());
        let out = run(
            &fs,
            &crate::NullEmitter,
            &FsBoundary::declared(vec!["./**".into()], vec!["./**".into()]),
            &args(&json!({
                "input": "./in.png", "out": "./styled.png", "seed": 1,
                "ops": [ { "dither": { "mode": "bayer4", "palette": "gameboy" } } ],
            })),
        )
        .await
        .expect("dispatch runs");
        assert_eq!(out["path"], "./styled.png");
        assert_eq!(out["format"], "png");
        assert_eq!(out["ops_applied"], 1);
        let sha = out["sha256"].as_str().expect("sha256 string");
        assert_eq!(sha.len(), 64);
        assert_eq!(out["skipped_existing"], false);
        // The bytes actually landed on the mock fs, hashing to the reported sha.
        let bytes = fs
            .read(Path::new("./styled.png"))
            .await
            .expect("artifact on disk");
        assert_eq!(nika_fx::det::sha256_hex(&bytes), sha);
    }

    #[tokio::test]
    async fn dispatch_is_idempotent_on_rerun() {
        use nika_kernel_mock::MockFs;
        let fs = MockFs::new().with_file("./in.png", tiny_png());
        let a = args(&json!({
            "input": "./in.png", "out": "./o.png", "seed": 9,
            "ops": [ { "grayscale": {} } ],
        }));
        let boundary = FsBoundary::declared(vec!["./**".into()], vec!["./**".into()]);
        let first = run(&fs, &crate::NullEmitter, &boundary, &a)
            .await
            .expect("run 1");
        assert_eq!(first["skipped_existing"], false);
        let second = run(&fs, &crate::NullEmitter, &boundary, &a)
            .await
            .expect("run 2");
        assert_eq!(second["skipped_existing"], true);
        assert_eq!(first["sha256"], second["sha256"]);
    }

    #[tokio::test]
    async fn dispatch_refuses_out_of_boundary_write() {
        use nika_kernel_mock::MockFs;
        let fs = MockFs::new().with_file("./in.png", tiny_png());
        // Write boundary covers ./out only; the styled artifact aims outside.
        let boundary = FsBoundary::declared(vec!["./**".into()], vec!["./out/**".into()]);
        let e = run(
            &fs,
            &crate::NullEmitter,
            &boundary,
            &args(&json!({
                "input": "./in.png", "out": "./escape.png",
                "ops": [ { "grayscale": {} } ],
            })),
        )
        .await
        .expect_err("boundary must refuse");
        assert_eq!(e.code, "NIKA-SEC-004");
    }

    #[tokio::test]
    async fn dispatch_rejects_non_png_input() {
        use nika_kernel_mock::MockFs;
        let fs = MockFs::new().with_file("./in.png", b"not a png at all".to_vec());
        let e = run(
            &fs,
            &crate::NullEmitter,
            &FsBoundary::declared(vec!["./**".into()], vec!["./**".into()]),
            &args(&json!({
                "input": "./in.png", "out": "./o.png",
                "ops": [ { "grayscale": {} } ],
            })),
        )
        .await
        .expect_err("non-png rejected");
        assert_eq!(e.code, C_FORMAT);
    }
}
