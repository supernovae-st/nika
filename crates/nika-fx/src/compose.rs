// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Deterministic composition — several PNGs become ONE (a `grid`, a `row`,
//! a `column`): the pure substrate behind `nika:image_compose` (stdlib
//! §Media graduate #5).
//!
//! The effect pipeline's law, applied to layout: integer geometry, the
//! `resize` op's linear-light ladder for every scaled box, no clock, no
//! randomness — identical `(input bytes, args)` produce byte-identical PNG
//! bytes, and the recipe rides the artifact (`nika` tEXt chunk · no
//! timestamp · no path: the same bytes composed from another folder are the
//! same file). Every size is judged from the PNG headers and the args BEFORE
//! any decompression or canvas allocation.

#![allow(clippy::cast_possible_truncation)]
// Cast law: every `as` narrows a value already gated to ≤ 16384 per side or
// ≤ 2^26 pixels (the caps below) — the crate's pixel-math cast class.

use crate::args::ResizeFilter;
use crate::codec::png::{self, MAX_DIMENSION, MAX_PIXELS};
use crate::det;
use crate::error::FxError;
use crate::ops::geometry;
use crate::pix::PixBuf;

/// The ALGORITHM contract tag embedded in composition recipes — stable
/// across engine releases; a geometry change majors it (`image_compose/v2`).
pub const CONTRACT_TAG: &str = "image_compose/v1";
/// The most inputs one composition takes (an 8×8 sheet).
pub const MAX_INPUTS: usize = 64;
/// The most columns or rows a `grid` may declare.
pub const MAX_TRACKS: u32 = 64;
/// The widest `gap` or `padding`, in pixels.
pub const MAX_SPACING: u32 = 1024;

/// How the inputs are laid out.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Rows and columns, row-major (the default).
    Grid,
    /// One row, left to right.
    Row,
    /// One column, top to bottom.
    Column,
}

impl Layout {
    /// The wire spelling (`grid` · `row` · `column`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Grid => "grid",
            Self::Row => "row",
            Self::Column => "column",
        }
    }
}

/// How each input is sized into its cell.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    /// Each image at its own size (the default).
    None,
    /// Scaled up or down, aspect kept, to the largest box inside the cell.
    Fit,
    /// Scaled, aspect kept, to cover the cell, then center-cropped to it.
    Fill,
}

impl Scale {
    /// The wire spelling (`none` · `fit` · `fill`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fit => "fit",
            Self::Fill => "fill",
        }
    }
}

/// The composition arguments (the contract's defaults from [`Self::new`]).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeArgs {
    /// The layout (default [`Layout::Grid`]).
    pub layout: Layout,
    /// `grid` only · derived from the count when absent.
    pub columns: Option<u32>,
    /// `grid` only · derived from the count when absent.
    pub rows: Option<u32>,
    /// The sizing (default [`Scale::None`]).
    pub scale: Scale,
    /// The cell box `(width, height)` · `fit` / `fill` only · default: the
    /// widest input width × the tallest input height.
    pub cell: Option<(u32, u32)>,
    /// Pixels between cells.
    pub gap: u32,
    /// Pixels around the sheet.
    pub padding: u32,
    /// The canvas colour (default white).
    pub background: [u8; 3],
}

impl ComposeArgs {
    /// The contract defaults: a derived grid, each image at its own size,
    /// no gap, no padding, a white background.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            layout: Layout::Grid,
            columns: None,
            rows: None,
            scale: Scale::None,
            cell: None,
            gap: 0,
            padding: 0,
            background: [255, 255, 255],
        }
    }
}

impl Default for ComposeArgs {
    fn default() -> Self {
        Self::new()
    }
}

/// What one input contributed (the lineage the trace records).
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFacts {
    /// sha256 of the input's bytes.
    pub sha256: String,
    /// The input's own width.
    pub width: u32,
    /// The input's own height.
    pub height: u32,
}

/// The composed artifact and its receipts.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeOutput {
    /// The PNG bytes (RGB8 · the recipe in a `nika` tEXt chunk).
    pub png: Vec<u8>,
    /// The canvas width.
    pub width: u32,
    /// The canvas height.
    pub height: u32,
    /// The resolved column count.
    pub columns: u32,
    /// The resolved row count.
    pub rows: u32,
    /// One entry per input, in order.
    pub inputs: Vec<InputFacts>,
    /// The deterministic recipe JSON embedded in the PNG.
    pub recipe: String,
}

/// Resolve and validate the grid for `count` inputs — every argument
/// rejection is an [`FxError::Args`] raised before any byte is decoded.
///
/// # Errors
/// `FxError::Args` for a count outside `1..=`[`MAX_INPUTS`], `columns` /
/// `rows` outside `grid` or outside `1..=`[`MAX_TRACKS`], a grid with fewer
/// cells than inputs, a `cell` with [`Scale::None`] or a side outside
/// `1..=16384`, and a `gap` / `padding` over [`MAX_SPACING`].
pub fn grid(args: &ComposeArgs, count: usize) -> Result<(u32, u32), FxError> {
    if count == 0 || count > MAX_INPUTS {
        return Err(FxError::Args(format!(
            "`inputs:` holds {count} paths · the contract is 1..={MAX_INPUTS}"
        )));
    }
    let n = count as u32;
    let tracks = match args.layout {
        Layout::Row | Layout::Column => {
            for (key, value) in [("columns", args.columns), ("rows", args.rows)] {
                if value.is_some() {
                    return Err(FxError::Args(format!(
                        "`{key}:` applies to `layout: grid` only (layout is {})",
                        args.layout.name()
                    )));
                }
            }
            if args.layout == Layout::Row {
                (n, 1)
            } else {
                (1, n)
            }
        }
        Layout::Grid => grid_tracks(args.columns, args.rows, n)?,
    };
    match (args.scale, args.cell) {
        (Scale::None, Some(_)) => {
            return Err(FxError::Args(
                "`cell:` applies to `scale: fit | fill` only (scale is none)".into(),
            ));
        }
        (_, Some((w, h)))
            if !(1..=MAX_DIMENSION).contains(&w) || !(1..=MAX_DIMENSION).contains(&h) =>
        {
            return Err(FxError::Args(format!(
                "`cell:` {w}x{h} · each side is 1..={MAX_DIMENSION}"
            )));
        }
        _ => {}
    }
    for (key, value) in [("gap", args.gap), ("padding", args.padding)] {
        if value > MAX_SPACING {
            return Err(FxError::Args(format!(
                "`{key}:` {value} · the contract is 0..={MAX_SPACING}"
            )));
        }
    }
    Ok(tracks)
}

/// The `grid` derivation: ⌈√n⌉ columns when neither is given, the other
/// side as ⌈n ÷ given⌉ when one is, and a check that both hold n.
fn grid_tracks(columns: Option<u32>, rows: Option<u32>, n: u32) -> Result<(u32, u32), FxError> {
    for (key, value) in [("columns", columns), ("rows", rows)] {
        if let Some(v) = value
            && !(1..=MAX_TRACKS).contains(&v)
        {
            return Err(FxError::Args(format!(
                "`{key}:` {v} · the contract is 1..={MAX_TRACKS}"
            )));
        }
    }
    Ok(match (columns, rows) {
        (None, None) => {
            let c = ceil_sqrt(n);
            (c, n.div_ceil(c))
        }
        (Some(c), None) => (c, n.div_ceil(c)),
        (None, Some(r)) => (n.div_ceil(r), r),
        (Some(c), Some(r)) => {
            if c * r < n {
                return Err(FxError::Args(format!(
                    "a {c}x{r} grid holds {} cells · fewer than the {n} inputs",
                    c * r
                )));
            }
            (c, r)
        }
    })
}

/// The smallest `c ≥ 1` with `c·c ≥ n` — integer, no float root.
const fn ceil_sqrt(n: u32) -> u32 {
    let mut c = 1;
    while c * c < n {
        c += 1;
    }
    c
}

/// `⌊(a + ⌊b ÷ 2⌋) ÷ b⌋`, never below 1 — the contract's rounding.
fn round_div(a: u64, b: u64) -> u64 {
    ((a + b / 2) / b).max(1)
}

/// How one input lands: the box it is scaled to, the window cropped from
/// that box (`fill`), and the tile that reaches the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Placement {
    scaled: (u32, u32),
    crop: Option<(u32, u32)>,
    tile: (u32, u32),
}

/// The placed box of a `w × h` input (the contract's geometry, verbatim).
fn place(w: u32, h: u32, scale: Scale, cell: (u32, u32)) -> Result<Placement, FxError> {
    let (w64, h64) = (u64::from(w), u64::from(h));
    let (cw, ch) = (u64::from(cell.0), u64::from(cell.1));
    let scaled = match scale {
        Scale::None => {
            return Ok(Placement {
                scaled: (w, h),
                crop: None,
                tile: (w, h),
            });
        }
        Scale::Fit if w64 * ch <= h64 * cw => (round_div(w64 * ch, h64), ch),
        Scale::Fit => (cw, round_div(h64 * cw, w64)),
        Scale::Fill if w64 * ch >= h64 * cw => (round_div(w64 * ch, h64).max(cw), ch),
        Scale::Fill => (cw, round_div(h64 * cw, w64).max(ch)),
    };
    let side = u64::from(MAX_DIMENSION);
    if scaled.0 > side || scaled.1 > side {
        return Err(FxError::Cap(format!(
            "an input scaled to {}x{} exceeds the {side} per-side cap (a very wide or tall \
             input filling this cell)",
            scaled.0, scaled.1
        )));
    }
    if scaled.0 * scaled.1 > MAX_PIXELS {
        return Err(FxError::Budget {
            pixels: scaled.0 * scaled.1,
            max: MAX_PIXELS,
        });
    }
    let scaled = (scaled.0 as u32, scaled.1 as u32);
    Ok(match scale {
        Scale::Fill => Placement {
            scaled,
            crop: Some(((scaled.0 - cell.0) / 2, (scaled.1 - cell.1) / 2)),
            tile: cell,
        },
        _ => Placement {
            scaled,
            crop: None,
            tile: scaled,
        },
    })
}

/// Column widths and row heights: the cell for `fit` / `fill`, the widest /
/// tallest tile of each track for `none` (an empty track is 0).
fn tracks(
    placed: &[Placement],
    cols: u32,
    rows: u32,
    cell: Option<(u32, u32)>,
) -> (Vec<u64>, Vec<u64>) {
    if let Some((cw, ch)) = cell {
        return (
            vec![u64::from(cw); cols as usize],
            vec![u64::from(ch); rows as usize],
        );
    }
    let mut col_w = vec![0u64; cols as usize];
    let mut row_h = vec![0u64; rows as usize];
    for (i, p) in placed.iter().enumerate() {
        let (c, r) = (i % cols as usize, i / cols as usize);
        col_w[c] = col_w[c].max(u64::from(p.tile.0));
        row_h[r] = row_h[r].max(u64::from(p.tile.1));
    }
    (col_w, row_h)
}

/// `2·padding + Σ sizes + gap·(n − 1)` — one canvas side.
fn extent(sizes: &[u64], gap: u32, padding: u32) -> u64 {
    let n = sizes.len() as u64;
    2 * u64::from(padding) + sizes.iter().sum::<u64>() + u64::from(gap) * n.saturating_sub(1)
}

/// Compose `inputs` (PNG bytes, in order) into one PNG. Pure: no I/O, no
/// clock, no randomness.
///
/// # Errors
/// `FxError::Args` (see [`grid`]) · `FxError::UnsupportedFormat` /
/// `FxError::Decode` (an input, named by its index · a second IHDR) ·
/// `FxError::Budget` (the summed input pixels, a scaled box or the canvas
/// over 2^26 pixels) · `FxError::Cap` (an input, canvas or scaled-box side
/// over 16384) — every size is judged before any decompression or canvas
/// allocation.
pub fn compose(inputs: &[&[u8]], args: &ComposeArgs) -> Result<ComposeOutput, FxError> {
    let (cols, rows) = grid(args, inputs.len())?;
    // 1 · the headers: every input's size and format, and the summed pixel
    // budget, BEFORE any input is inflated.
    let mut dims = Vec::with_capacity(inputs.len());
    let mut total = 0u64;
    for (i, bytes) in inputs.iter().enumerate() {
        let (w, h) = png::header(bytes).map_err(|e| at_input(i, e))?;
        total += u64::from(w) * u64::from(h);
        dims.push((w, h));
    }
    if total > MAX_PIXELS {
        return Err(FxError::Budget {
            pixels: total,
            max: MAX_PIXELS,
        });
    }
    // 2 · the cell and each input's placed box.
    let cell = match args.scale {
        Scale::None => None,
        Scale::Fit | Scale::Fill => Some(args.cell.unwrap_or_else(|| {
            dims.iter()
                .fold((1, 1), |(mw, mh), &(w, h)| (mw.max(w), mh.max(h)))
        })),
    };
    let placed = dims
        .iter()
        .map(|&(w, h)| place(w, h, args.scale, cell.unwrap_or((w, h))))
        .collect::<Result<Vec<_>, _>>()?;
    // 3 · the tracks and the canvas, judged before it is allocated.
    let (col_w, row_h) = tracks(&placed, cols, rows, cell);
    let (width, height) = (
        extent(&col_w, args.gap, args.padding),
        extent(&row_h, args.gap, args.padding),
    );
    let side = u64::from(MAX_DIMENSION);
    if width > side || height > side {
        return Err(FxError::Cap(format!(
            "the canvas would be {width}x{height} · each side is capped at {side}"
        )));
    }
    if width * height > MAX_PIXELS {
        return Err(FxError::Budget {
            pixels: width * height,
            max: MAX_PIXELS,
        });
    }
    let (width, height) = (width as usize, height as usize);
    let bg = args.background;
    let mut canvas = PixBuf::from_pixels(width, height, vec![bg; width * height])
        .ok_or_else(|| FxError::Encode("canvas allocation mismatch".into()))?;
    // 4 · paint each input into its cell, row-major, centered.
    for (i, bytes) in inputs.iter().enumerate() {
        let (rgb, alpha) = png::decode_with_alpha(bytes).map_err(|e| at_input(i, e))?;
        // The budget judged the header's size: paint nothing else.
        if (rgb.w, rgb.h) != (dims[i].0 as usize, dims[i].1 as usize) {
            return Err(FxError::Decode(format!(
                "inputs[{i}]: decoded {}x{} but the header judged {}x{}",
                rgb.w, rgb.h, dims[i].0, dims[i].1
            )));
        }
        let flat = flatten(rgb, alpha.as_deref(), bg);
        let tile = scale_into(&flat, placed[i])?;
        let (c, r) = (i % cols as usize, i / cols as usize);
        let x0 = cell_origin(&col_w, c, args.gap, args.padding) + (col_w[c] as usize - tile.w) / 2;
        let y0 = cell_origin(&row_h, r, args.gap, args.padding) + (row_h[r] as usize - tile.h) / 2;
        blit(&mut canvas, &tile, x0, y0);
    }
    let facts: Vec<InputFacts> = inputs
        .iter()
        .zip(&dims)
        .map(|(bytes, &(w, h))| InputFacts {
            sha256: det::sha256_hex(bytes),
            width: w,
            height: h,
        })
        .collect();
    let recipe = recipe_json(args, (cols, rows), cell, &facts);
    Ok(ComposeOutput {
        png: png::encode(&canvas, Some(&recipe)),
        width: width as u32,
        height: height as u32,
        columns: cols,
        rows,
        inputs: facts,
        recipe,
    })
}

/// Name the input an error came from (the variant is kept: the builtin maps
/// variants to codes).
fn at_input(i: usize, e: FxError) -> FxError {
    match e {
        FxError::Args(m) => FxError::Args(format!("inputs[{i}]: {m}")),
        FxError::UnsupportedFormat(m) => FxError::UnsupportedFormat(format!("inputs[{i}]: {m}")),
        FxError::Decode(m) => FxError::Decode(format!("inputs[{i}]: {m}")),
        FxError::Cap(m) => FxError::Cap(format!("inputs[{i}]: {m}")),
        other => other,
    }
}

/// Composite an alpha plane over the background, per channel
/// `⌊(c·a + bg·(255 − a) + 127) ÷ 255⌋` in sRGB integers. Opaque inputs pass
/// through untouched.
fn flatten(mut rgb: PixBuf, alpha: Option<&[u8]>, bg: [u8; 3]) -> PixBuf {
    let Some(alpha) = alpha else {
        return rgb;
    };
    for (px, &a) in rgb.pixels_mut().iter_mut().zip(alpha) {
        let a = u32::from(a);
        for (c, &b) in px.iter_mut().zip(&bg) {
            *c = ((u32::from(*c) * a + u32::from(b) * (255 - a) + 127) / 255) as u8;
        }
    }
    rgb
}

/// Scale (and, for `fill`, center-crop) one flattened input to its tile —
/// the `resize` op with `filter: auto`, the same bytes that op produces.
fn scale_into(img: &PixBuf, p: Placement) -> Result<PixBuf, FxError> {
    let scaled = if (img.w, img.h) == (p.scaled.0 as usize, p.scaled.1 as usize) {
        img.clone()
    } else {
        geometry::resize(img, Some(p.scaled.0), Some(p.scaled.1), ResizeFilter::Auto)?
    };
    match p.crop {
        Some((x, y)) => geometry::crop(&scaled, x, y, p.tile.0, p.tile.1),
        None => Ok(scaled),
    }
}

/// The left (or top) edge of track `k`: the padding plus every earlier
/// track and its gap.
fn cell_origin(sizes: &[u64], k: usize, gap: u32, padding: u32) -> usize {
    let before: u64 = sizes[..k].iter().map(|s| s + u64::from(gap)).sum();
    (u64::from(padding) + before) as usize
}

/// Copy `tile` onto `canvas` at `(x0, y0)` — in bounds by construction (a
/// tile never exceeds its track).
fn blit(canvas: &mut PixBuf, tile: &PixBuf, x0: usize, y0: usize) {
    for y in 0..tile.h {
        for x in 0..tile.w {
            canvas.set(x0 + x, y0 + y, tile.get(x, y));
        }
    }
}

/// The recipe JSON — hand-built, fixed key order, no timestamp, no path.
fn recipe_json(
    args: &ComposeArgs,
    (cols, rows): (u32, u32),
    cell: Option<(u32, u32)>,
    inputs: &[InputFacts],
) -> String {
    use std::fmt::Write as _;
    let mut s =
        format!("{{\"kind\":\"image_compose\",\"v\":1,\"engine\":\"{CONTRACT_TAG}\",\"inputs\":[");
    for (i, f) in inputs.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(
            s,
            "{{\"sha256\":\"{}\",\"width\":{},\"height\":{}}}",
            f.sha256, f.width, f.height
        );
    }
    let [r, g, b] = args.background;
    let _ = write!(
        s,
        "],\"layout\":\"{}\",\"columns\":{cols},\"rows\":{rows},\"scale\":\"{}\",\"cell\":{},\
         \"gap\":{},\"padding\":{},\"background\":\"#{r:02x}{g:02x}{b:02x}\"}}",
        args.layout.name(),
        args.scale.name(),
        cell.map_or_else(|| "null".to_owned(), |(w, h)| format!("[{w},{h}]")),
        args.gap,
        args.padding,
    );
    s
}

#[cfg(test)]
mod tests;
