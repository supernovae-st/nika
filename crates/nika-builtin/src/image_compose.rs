// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! `nika:image_compose` — deterministic image composition (stdlib §Media
//! graduate #5): an ordered PNG list becomes ONE PNG, laid out as a `grid`,
//! a `row` or a `column`.
//!
//! The `image_fx` discipline, verbatim: NO provider plane, NO keys, NO
//! clock — fs + boundary + emitter only. The pixel work is `nika-fx`
//! (zero-dep · byte-deterministic) on the blocking pool. Every input is a
//! READ gated on `permits.fs.read` before it is opened and read through the
//! judged fs (the at-open re-judgment + final-component pin); the artifact
//! lands at `out:` (gated on `permits.fs.write`); outputs carry path +
//! sha256 + width + height, bytes never ride outputs. Identical bytes
//! already on disk = idempotent skip, decided WITHOUT journaling a denied
//! read of the builtin's own output.

use nika_fx::FxError;
use nika_fx::compose::{ComposeArgs, Layout, MAX_INPUTS, Scale};
use nika_kernel::io::fs::{FsReadDyn, FsWriteDyn};
use serde_json::{Value, json};
use std::path::Path;

use crate::permits::{FsAccess, FsBoundary};
use crate::{Args, BuiltinFailure, BuiltinOutcome, Emitter};

const C_ARGS: &str = "NIKA-BUILTIN-IMAGE_COMPOSE-001";
const C_INPUT: &str = "NIKA-BUILTIN-IMAGE_COMPOSE-002";
const C_FORMAT: &str = "NIKA-BUILTIN-IMAGE_COMPOSE-003";
const C_DECODE: &str = "NIKA-BUILTIN-IMAGE_COMPOSE-004";
const C_BUDGET: &str = "NIKA-BUILTIN-IMAGE_COMPOSE-005";
const C_SAVE: &str = "NIKA-BUILTIN-IMAGE_COMPOSE-006";

/// The closed argument set (an unknown key is refused · labels are not v1).
const KEYS: [&str; 10] = [
    "inputs",
    "out",
    "layout",
    "columns",
    "rows",
    "scale",
    "cell",
    "gap",
    "padding",
    "background",
];

fn fail(code: &'static str, message: impl Into<String>) -> BuiltinFailure {
    BuiltinFailure::new(code, message)
}

/// Run the builtin: parse → gate + read every input → compose (blocking
/// pool) → gate + save the artifact → outputs `{path, sha256, dims, …}`.
///
/// `fs` is the raw effect (the artifact write and its idempotence read);
/// `judged` is the at-open re-judging view every input read rides.
pub(crate) async fn run<F, J>(
    fs: &F,
    judged: &J,
    emitter: &dyn Emitter,
    boundary: &FsBoundary,
    args: &Args,
) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
    J: FsReadDyn,
{
    let parsed = parse(args)?;
    emitter.emit(
        "image_compose.started",
        json!({
            "inputs": parsed.inputs.len(), "out": parsed.out,
            "layout": parsed.compose.layout.name(), "scale": parsed.compose.scale.name(),
        }),
    );
    match execute(fs, judged, boundary, &parsed).await {
        Ok(output) => {
            emitter.emit(
                "image_compose.completed",
                json!({
                    "path": output["path"],
                    "sha256_8": output["sha256"].as_str().map(|s| &s[..8.min(s.len())]),
                    "size_bytes": output["size_bytes"],
                    "width": output["width"],
                    "height": output["height"],
                }),
            );
            Ok(output)
        }
        Err(failure) => {
            emitter.emit(
                "image_compose.error",
                json!({ "code": failure.code, "message": failure.message }),
            );
            Err(failure)
        }
    }
}

#[derive(Debug)]
struct Parsed {
    inputs: Vec<String>,
    out: String,
    compose: ComposeArgs,
}

async fn execute<F, J>(fs: &F, judged: &J, boundary: &FsBoundary, parsed: &Parsed) -> BuiltinOutcome
where
    F: FsReadDyn + FsWriteDyn,
    J: FsReadDyn,
{
    // Every input is gated BEFORE any is opened: one escaping path refuses
    // the call with nothing read (the boundary, not the I/O, is the gate).
    for input in &parsed.inputs {
        boundary.enforce(fs, input, FsAccess::Read).await?;
    }
    let mut bytes = Vec::with_capacity(parsed.inputs.len());
    for (i, input) in parsed.inputs.iter().enumerate() {
        let read = judged
            .read(Path::new(input))
            .await
            .map_err(|e| fail(C_INPUT, format!("inputs[{i}] `{input}`: {e}")))?;
        bytes.push(read);
    }

    // Pixel work on the blocking pool (the image_fx/jq precedent).
    let args = parsed.compose.clone();
    let output = tokio::task::spawn_blocking(move || {
        let refs: Vec<&[u8]> = bytes.iter().map(AsRef::as_ref).collect();
        nika_fx::compose::compose(&refs, &args)
    })
    .await
    .map_err(|e| fail(C_DECODE, format!("composition task failed: {e}")))?
    .map_err(map_fx_error)?;
    let sha256 = nika_fx::det::sha256_hex(&output.png);

    boundary.enforce(fs, &parsed.out, FsAccess::Write).await?;
    let out_path = Path::new(&parsed.out);
    // Idempotent skip: identical bytes already on disk. The skip-read of the
    // builtin's OWN output is judged silently — no read grant means no
    // byte-equality oracle (we just re-write), and never a denied read
    // charged to the journal for a read the author never asked for.
    let skipped = if boundary
        .enforce_unwitnessed(fs, &parsed.out, FsAccess::Read)
        .await
        .is_ok()
    {
        matches!(fs.read(out_path).await, Ok(existing) if existing.as_ref() == output.png.as_slice())
    } else {
        false
    };
    if !skipped {
        if let Some(parent) = out_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs.create_dir_all(parent)
                .await
                .map_err(|e| fail(C_SAVE, format!("creating `{}`: {e}", parent.display())))?;
        }
        fs.write(out_path, &output.png)
            .await
            .map_err(|e| fail(C_SAVE, format!("writing `{}`: {e}", parsed.out)))?;
    }

    let inputs: Vec<Value> = parsed
        .inputs
        .iter()
        .zip(&output.inputs)
        .map(|(path, facts)| {
            json!({
                "path": path, "sha256": facts.sha256,
                "width": facts.width, "height": facts.height,
            })
        })
        .collect();
    Ok(json!({
        "path": parsed.out,
        "sha256": sha256,
        "size_bytes": output.png.len(),
        "width": output.width,
        "height": output.height,
        "format": "png",
        "layout": parsed.compose.layout.name(),
        "columns": output.columns,
        "rows": output.rows,
        "scale": parsed.compose.scale.name(),
        "inputs": inputs,
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
            format!("pixel budget exceeded: {pixels} > {max} (the summed inputs or the canvas)"),
        )
        .with_details(json!({ "pixels": pixels, "max": max })),
        FxError::Cap(m) => fail(C_BUDGET, m),
        FxError::Encode(m) => fail(C_SAVE, m),
        _ => fail(C_DECODE, format!("unmapped composition error: {e}")),
    }
}

// ── args parsing (pure · pre-I/O · every rejection = -001) ────────────────

fn parse(args: &Args) -> Result<Parsed, BuiltinFailure> {
    if let Some(unknown) = args.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(fail(
            C_ARGS,
            format!(
                "unknown argument `{unknown}` (the arguments are {} · labels and JPEG are not v1)",
                KEYS.join(" · ")
            ),
        ));
    }
    let inputs = parse_inputs(args)?;
    let out = args
        .get("out")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| fail(C_ARGS, "`out:` is required (a non-empty .png path)"))?
        .to_owned();
    if Path::new(&out).extension().and_then(|e| e.to_str()) != Some("png") {
        return Err(fail(
            C_ARGS,
            format!("`out:` must end in .png — the artifact is a PNG (got `{out}`)"),
        ));
    }
    let mut compose = ComposeArgs::new();
    compose.layout = match opt_str(args, "layout")? {
        None | Some("grid") => Layout::Grid,
        Some("row") => Layout::Row,
        Some("column") => Layout::Column,
        Some(other) => {
            return Err(fail(
                C_ARGS,
                format!("`layout: {other}` (grid | row | column)"),
            ));
        }
    };
    compose.scale = match opt_str(args, "scale")? {
        None | Some("none") => Scale::None,
        Some("fit") => Scale::Fit,
        Some("fill") => Scale::Fill,
        Some(other) => {
            return Err(fail(
                C_ARGS,
                format!("`scale: {other}` (none | fit | fill)"),
            ));
        }
    };
    compose.columns = opt_u32(args, "columns")?;
    compose.rows = opt_u32(args, "rows")?;
    compose.cell = parse_cell(args)?;
    compose.gap = opt_u32(args, "gap")?.unwrap_or(0);
    compose.padding = opt_u32(args, "padding")?.unwrap_or(0);
    if let Some(bg) = args.get("background") {
        compose.background =
            parse_color(bg).map_err(|m| fail(C_ARGS, format!("`background:` {m}")))?;
    }
    // The grid law (count · tracks · cell · spacing) is nika-fx's — one
    // judge, refused here before any input is read.
    nika_fx::compose::grid(&compose, inputs.len()).map_err(map_fx_error)?;
    Ok(Parsed {
        inputs,
        out,
        compose,
    })
}

fn parse_inputs(args: &Args) -> Result<Vec<String>, BuiltinFailure> {
    let list = args
        .get("inputs")
        .ok_or_else(|| {
            fail(
                C_ARGS,
                "`inputs:` is required (an ordered list of PNG paths)",
            )
        })?
        .as_array()
        .ok_or_else(|| fail(C_ARGS, "`inputs:` must be a list of PNG paths"))?;
    if list.is_empty() || list.len() > MAX_INPUTS {
        return Err(fail(
            C_ARGS,
            format!(
                "`inputs:` holds {} paths · the contract is 1..={MAX_INPUTS}",
                list.len()
            ),
        ));
    }
    list.iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    fail(
                        C_ARGS,
                        format!("`inputs[{i}]` must be a non-empty path string"),
                    )
                })
        })
        .collect()
}

fn parse_cell(args: &Args) -> Result<Option<(u32, u32)>, BuiltinFailure> {
    let Some(cell) = args.get("cell") else {
        return Ok(None);
    };
    let map = cell
        .as_object()
        .ok_or_else(|| fail(C_ARGS, "`cell:` must be { width, height }"))?;
    if let Some(key) = map
        .keys()
        .find(|k| !matches!(k.as_str(), "width" | "height"))
    {
        return Err(fail(
            C_ARGS,
            format!("`cell` has no key `{key}` (width · height)"),
        ));
    }
    let side = |key: &str| -> Result<u32, BuiltinFailure> {
        map.get(key)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| fail(C_ARGS, format!("`cell.{key}` must be a positive integer")))
    };
    Ok(Some((side("width")?, side("height")?)))
}

fn opt_str<'a>(args: &'a Args, key: &str) -> Result<Option<&'a str>, BuiltinFailure> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| fail(C_ARGS, format!("`{key}:` must be a string"))),
    }
}

fn opt_u32(args: &Args, key: &str) -> Result<Option<u32>, BuiltinFailure> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| fail(C_ARGS, format!("`{key}:` must be a non-negative integer"))),
    }
}

/// A colour is `"#rrggbb"` or `[r, g, b]` (the `image_fx` grammar).
fn parse_color(v: &Value) -> Result<[u8; 3], String> {
    if let Some(text) = v.as_str() {
        if let Some(hex) = text.strip_prefix('#')
            && hex.len() == 6
            && hex.is_ascii()
        {
            let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16);
            if let (Ok(r), Ok(g), Ok(b)) = (channel(0), channel(2), channel(4)) {
                return Ok([r, g, b]);
            }
        }
        return Err(format!("`{text}` must be #rrggbb or [r, g, b]"));
    }
    if let Some(list) = v.as_array()
        && list.len() == 3
    {
        let channel = |i: usize| {
            list[i]
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| "colour channels are integers 0..=255".to_owned())
        };
        return Ok([channel(0)?, channel(1)?, channel(2)?]);
    }
    Err(format!("{v} must be #rrggbb or [r, g, b]"))
}

#[cfg(test)]
mod tests;
