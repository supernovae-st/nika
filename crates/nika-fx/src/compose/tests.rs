// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The composition contract, pinned: exact canvas sizes, the pixels at the
//! places the geometry puts them, golden sha256 per layout, and every gate
//! firing from the headers before any inflate.

use super::*;
use crate::codec::png::{dual_ihdr, encode, encode_samples, header_only};

/// A `w × h` RGB PNG painted one flat colour.
fn flat(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> {
    let px = PixBuf::from_pixels(w, h, vec![rgb; w * h]).expect("pixel count");
    encode(&px, None)
}

/// A `w × h` RGB gradient (distinct pixels, so a scaled or cropped tile is
/// told apart from a misplaced one).
fn gradient(w: usize, h: usize, seed: u8) -> Vec<u8> {
    let mut px = PixBuf::new(w, h);
    for y in 0..h {
        for x in 0..w {
            px.set(
                x,
                y,
                [(x * 255 / w.max(2)) as u8, (y * 255 / h.max(2)) as u8, seed],
            );
        }
    }
    encode(&px, None)
}

fn args(layout: Layout, scale: Scale) -> ComposeArgs {
    let mut a = ComposeArgs::new();
    a.layout = layout;
    a.scale = scale;
    a
}

fn decoded(out: &ComposeOutput) -> PixBuf {
    crate::codec::png::decode(&out.png).expect("the artifact decodes")
}

const RED: [u8; 3] = [220, 30, 30];
const BLUE: [u8; 3] = [30, 60, 220];
const GREEN: [u8; 3] = [20, 200, 90];
const BG: [u8; 3] = [16, 20, 28];

#[test]
fn row_places_each_image_at_its_own_size_centered_in_the_row() {
    let (a, b) = (flat(3, 2, RED), flat(2, 4, BLUE));
    let mut args = args(Layout::Row, Scale::None);
    (args.gap, args.padding, args.background) = (1, 2, BG);
    let out = compose(&[&a, &b], &args).expect("row composes");
    // 2·2 padding + 3 + 2 + one gap  ×  2·2 padding + the taller (4)
    assert_eq!(
        (out.width, out.height, out.columns, out.rows),
        (10, 8, 2, 1)
    );
    let img = decoded(&out);
    // The 3×2 red is centered in the 4-high row: rows 3..5, columns 2..5.
    assert_eq!(img.get(2, 3), RED);
    assert_eq!(img.get(4, 4), RED);
    assert_eq!(img.get(2, 2), BG, "above the short image is background");
    assert_eq!(img.get(2, 5), BG, "below the short image is background");
    // The gap column, then the 2×4 blue at columns 6..8, rows 2..6.
    assert_eq!(img.get(5, 3), BG, "the gap");
    assert_eq!(img.get(6, 2), BLUE);
    assert_eq!(img.get(7, 5), BLUE);
    // The padding ring.
    assert_eq!(img.get(0, 0), BG);
    assert_eq!(img.get(9, 7), BG);
}

#[test]
fn column_stacks_top_to_bottom_centered_in_the_column() {
    let (a, b) = (flat(4, 1, RED), flat(2, 2, BLUE));
    let mut args = args(Layout::Column, Scale::None);
    (args.gap, args.background) = (3, BG);
    let out = compose(&[&a, &b], &args).expect("column composes");
    assert_eq!((out.width, out.height, out.columns, out.rows), (4, 6, 1, 2));
    let img = decoded(&out);
    assert_eq!(img.get(0, 0), RED);
    assert_eq!(img.get(3, 0), RED);
    assert_eq!(img.get(1, 2), BG, "the gap rows");
    // The 2-wide blue is centered in the 4-wide column: columns 1..3.
    assert_eq!(img.get(0, 4), BG);
    assert_eq!(img.get(1, 4), BLUE);
    assert_eq!(img.get(2, 5), BLUE);
    assert_eq!(img.get(3, 5), BG);
}

#[test]
fn grid_derives_columns_and_rows_from_the_count() {
    let none = ComposeArgs::new();
    for (n, want) in [
        (1, (1, 1)),
        (2, (2, 1)),
        (3, (2, 2)),
        (4, (2, 2)),
        (5, (3, 2)),
        (7, (3, 3)),
        (10, (4, 3)),
        (64, (8, 8)),
    ] {
        assert_eq!(grid(&none, n), Ok(want), "n = {n}");
    }
    let mut cols = ComposeArgs::new();
    cols.columns = Some(4);
    assert_eq!(grid(&cols, 10), Ok((4, 3)));
    let mut rows = ComposeArgs::new();
    rows.rows = Some(2);
    assert_eq!(grid(&rows, 5), Ok((3, 2)));
    let mut both = ComposeArgs::new();
    (both.columns, both.rows) = (Some(2), Some(3));
    assert_eq!(grid(&both, 6), Ok((2, 3)));
    assert!(
        matches!(grid(&both, 7), Err(FxError::Args(m)) if m.contains("2x3 grid holds 6 cells"))
    );
}

#[test]
fn grid_of_four_with_none_sizes_each_track_to_its_largest_tile() {
    let inputs = [
        flat(2, 2, RED),
        flat(4, 1, BLUE),
        flat(1, 3, GREEN),
        flat(2, 2, RED),
    ];
    let refs: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
    let mut args = ComposeArgs::new();
    args.background = BG;
    let out = compose(&refs, &args).expect("grid composes");
    // columns: max(2, 1) = 2 · max(4, 2) = 4 → 6 wide; rows: max(2, 1) = 2 · max(3, 2) = 3 → 5 high
    assert_eq!((out.width, out.height, out.columns, out.rows), (6, 5, 2, 2));
    let img = decoded(&out);
    assert_eq!(img.get(0, 0), RED, "inputs[0] at the top left");
    // inputs[1] is 1 high in a 2-high row: ⌊(2 − 1) ÷ 2⌋ = 0, so it sits on
    // the row's first line and the second is background.
    assert_eq!(img.get(2, 0), BLUE);
    assert_eq!(img.get(5, 0), BLUE);
    assert_eq!(img.get(2, 1), BG);
    // inputs[2] is 1 wide in a 2-wide column: the same floor puts it at x 0.
    assert_eq!(img.get(0, 2), GREEN);
    assert_eq!(img.get(0, 4), GREEN);
    assert_eq!(img.get(1, 2), BG);
    // inputs[3] centered in the 4×3 cell: x 2 + ⌊(4 − 2) ÷ 2⌋ = 3, y 2 + 0.
    assert_eq!(img.get(3, 2), RED);
    assert_eq!(img.get(4, 3), RED);
    assert_eq!(img.get(2, 2), BG);
    assert_eq!(img.get(3, 4), BG);
}

#[test]
fn fit_letterboxes_inside_the_cell_with_the_aspect_kept() {
    let wide = flat(8, 4, RED);
    let mut args = args(Layout::Row, Scale::Fit);
    (args.cell, args.background) = (Some((4, 4)), BG);
    let out = compose(&[&wide], &args).expect("fit composes");
    assert_eq!((out.width, out.height), (4, 4), "the canvas is the cell");
    let img = decoded(&out);
    // 8×4 into 4×4 → 4×2, centered: rows 1..3 red, rows 0 and 3 background.
    assert_eq!(img.get(0, 0), BG);
    assert_eq!(img.get(0, 1), RED);
    assert_eq!(img.get(3, 2), RED);
    assert_eq!(img.get(3, 3), BG);
}

#[test]
fn fit_scales_a_small_image_up_to_the_cell() {
    let small = flat(2, 2, BLUE);
    let mut args = args(Layout::Row, Scale::Fit);
    args.cell = Some((6, 6));
    let out = compose(&[&small], &args).expect("fit composes");
    let img = decoded(&out);
    assert_eq!((img.w, img.h), (6, 6));
    assert!(
        img.pixels().iter().all(|p| *p == BLUE),
        "a flat colour stays flat when scaled up"
    );
}

#[test]
fn fill_covers_the_cell_then_crops_the_center() {
    // An 8×2 strip: left half red, right half blue.
    let mut strip = PixBuf::new(8, 2);
    for y in 0..2 {
        for x in 0..8 {
            strip.set(x, y, if x < 4 { RED } else { BLUE });
        }
    }
    let mut args = args(Layout::Row, Scale::Fill);
    args.cell = Some((2, 2));
    let out = compose(&[&encode(&strip, None)], &args).expect("fill composes");
    assert_eq!((out.width, out.height), (2, 2));
    // 8×2 covers 2×2 at its own size (8·2 ≥ 2·2 → 8×2), cropped at x = 3:
    // the two center columns, red then blue.
    let img = decoded(&out);
    assert_eq!(img.get(0, 0), RED);
    assert_eq!(img.get(1, 1), BLUE);
}

#[test]
fn the_default_cell_is_the_largest_input_box() {
    let (a, b) = (flat(4, 2, RED), flat(2, 6, BLUE));
    let out = compose(&[&a, &b], &args(Layout::Row, Scale::Fit)).expect("fit composes");
    // cell = 4 (widest) × 6 (tallest) → two 4×6 cells side by side.
    assert_eq!((out.width, out.height), (8, 6));
    assert!(out.recipe.contains("\"cell\":[4,6]"), "{}", out.recipe);
}

#[test]
fn alpha_composites_over_the_background_before_scaling() {
    // One half-transparent red pixel (alpha 128) and one fully transparent.
    let rgba = encode_samples(2, 1, 6, &[255, 0, 0, 128, 255, 255, 255, 0]);
    let mut args = args(Layout::Row, Scale::None);
    args.background = [0, 0, 255];
    let img = decoded(&compose(&[&rgba], &args).expect("rgba composes"));
    // ⌊(255·128 + 0·127 + 127) ÷ 255⌋ = 128 · ⌊(0·128 + 255·127 + 127) ÷ 255⌋ = 127
    assert_eq!(img.get(0, 0), [128, 0, 127]);
    assert_eq!(
        img.get(1, 0),
        [0, 0, 255],
        "fully transparent is the background"
    );
}

#[test]
fn identical_inputs_and_args_give_identical_bytes() {
    let inputs = [
        gradient(9, 7, 10),
        gradient(5, 11, 90),
        gradient(12, 4, 170),
    ];
    let refs: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
    let mut args = args(Layout::Grid, Scale::Fill);
    (args.cell, args.gap, args.padding, args.background) = (Some((6, 6)), 2, 3, BG);
    let one = compose(&refs, &args).expect("composes");
    let two = compose(&refs, &args).expect("composes again");
    assert_eq!(one, two);
}

/// The golden layouts: the exact bytes, pinned by sha256. A change here is a
/// change of the `image_compose/v1` wire contract — never a refactor.
/// One golden row: its name, the layout, the scale, the cell and the
/// artifact's sha256.
type GoldenCase = (
    &'static str,
    Layout,
    Scale,
    Option<(u32, u32)>,
    &'static str,
);

#[test]
fn golden_sha256_per_layout() {
    let inputs = [
        gradient(9, 7, 10),
        gradient(5, 11, 90),
        gradient(12, 4, 170),
        gradient(6, 6, 250),
    ];
    let refs: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
    let cases: [GoldenCase; 5] = [
        (
            "grid · none",
            Layout::Grid,
            Scale::None,
            None,
            GOLDEN_GRID_NONE,
        ),
        (
            "grid · fit",
            Layout::Grid,
            Scale::Fit,
            Some((8, 8)),
            GOLDEN_GRID_FIT,
        ),
        (
            "grid · fill",
            Layout::Grid,
            Scale::Fill,
            Some((8, 8)),
            GOLDEN_GRID_FILL,
        ),
        (
            "row · none",
            Layout::Row,
            Scale::None,
            None,
            GOLDEN_ROW_NONE,
        ),
        (
            "column · fit",
            Layout::Column,
            Scale::Fit,
            Some((10, 6)),
            GOLDEN_COLUMN_FIT,
        ),
    ];
    let mut actual = Vec::new();
    let mut expected = Vec::new();
    for (name, layout, scale, cell, golden) in &cases {
        let mut a = args(*layout, *scale);
        (a.cell, a.gap, a.padding, a.background) = (*cell, 2, 3, BG);
        let out = compose(&refs, &a).expect(name);
        actual.push(format!(
            "{name} · {}x{} · {}",
            out.width,
            out.height,
            det::sha256_hex(&out.png)
        ));
        expected.push(format!("{name} · {golden}"));
    }
    assert_eq!(
        actual, expected,
        "golden drift — the image_compose/v1 bytes changed"
    );
}

// Pinned 2026-10-26 from the first implementation; the dimensions were
// computed by hand from the contract's geometry before the bytes were taken
// (grid · none: 2·3 + 12 + 6 + 2 = 26 wide, 2·3 + 11 + 6 + 2 = 25 high).
const GOLDEN_GRID_NONE: &str =
    "26x25 · 6939c4e5a6613385518562352d5acebb21e2c6bd7e3cf8b0b2de0d7b833aaeec";
const GOLDEN_GRID_FIT: &str =
    "24x24 · b4786c39f6a97e450a1e96543cc416a11c47822a1fee3219af43278b370e3167";
const GOLDEN_GRID_FILL: &str =
    "24x24 · 4c7a215382826e9229710f1397c3a7fdfa870d58e4cf5913a4210d4c24c91f9c";
const GOLDEN_ROW_NONE: &str =
    "44x17 · 6b61bd221d3f776b9b58642494f370bc18bd668a68e5a5e46fbee93b70e6c2a2";
const GOLDEN_COLUMN_FIT: &str =
    "16x36 · 5a86764070da49c984fa4427d92f8fc0f76ecc1281890a68133b23ca23ce8fcd";

#[test]
fn the_recipe_rides_the_png_with_hashes_and_no_path() {
    let (a, b) = (flat(2, 2, RED), flat(2, 2, BLUE));
    let mut args = args(Layout::Row, Scale::None);
    (args.gap, args.background) = (1, BG);
    let out = compose(&[&a, &b], &args).expect("composes");
    let needle = b"tEXtnika\0";
    let at = out
        .png
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("a nika tEXt chunk");
    let json_start = at + needle.len();
    let len = u32::from_be_bytes(out.png[at - 4..at].try_into().expect("4 bytes")) as usize;
    let recipe = std::str::from_utf8(&out.png[json_start..at + 4 + len]).expect("ascii json");
    assert_eq!(recipe, out.recipe);
    assert!(
        recipe.starts_with("{\"kind\":\"image_compose\",\"v\":1,\"engine\":\"image_compose/v1\"")
    );
    assert!(recipe.contains(&det::sha256_hex(&a)) && recipe.contains(&det::sha256_hex(&b)));
    assert!(
        recipe.contains(
            "\"layout\":\"row\",\"columns\":2,\"rows\":1,\"scale\":\"none\",\"cell\":null"
        )
    );
    assert!(recipe.ends_with("\"gap\":1,\"padding\":0,\"background\":\"#10141c\"}"));
    assert_eq!(out.inputs[1].sha256, det::sha256_hex(&b));
    assert_eq!((out.inputs[0].width, out.inputs[0].height), (2, 2));
}

#[test]
fn argument_bounds_refuse_before_any_decode() {
    let one = flat(1, 1, RED);
    let junk: &[u8] = b"not a png";
    // Bad args win over a bad input: nothing is read past the args.
    let mut row = args(Layout::Row, Scale::None);
    row.columns = Some(2);
    assert!(
        matches!(compose(&[junk], &row), Err(FxError::Args(m)) if m.contains("`columns:` applies to `layout: grid` only"))
    );
    let mut cell = ComposeArgs::new();
    cell.cell = Some((4, 4));
    assert!(
        matches!(compose(&[&one], &cell), Err(FxError::Args(m)) if m.contains("scale is none"))
    );
    let mut big_gap = ComposeArgs::new();
    big_gap.gap = MAX_SPACING + 1;
    assert!(matches!(compose(&[&one], &big_gap), Err(FxError::Args(_))));
    let mut zero_cell = args(Layout::Grid, Scale::Fit);
    zero_cell.cell = Some((0, 4));
    assert!(matches!(
        compose(&[&one], &zero_cell),
        Err(FxError::Args(_))
    ));
    let mut wide_grid = ComposeArgs::new();
    wide_grid.columns = Some(MAX_TRACKS + 1);
    assert!(matches!(
        compose(&[&one], &wide_grid),
        Err(FxError::Args(_))
    ));
    assert!(
        matches!(compose(&[], &ComposeArgs::new()), Err(FxError::Args(m)) if m.contains("holds 0 paths"))
    );
    let many: Vec<&[u8]> = vec![&one; MAX_INPUTS + 1];
    assert!(
        matches!(compose(&many, &ComposeArgs::new()), Err(FxError::Args(m)) if m.contains("holds 65 paths"))
    );
}

#[test]
fn caps_fire_from_the_headers_before_any_inflate() {
    // Header-only PNGs carry no pixel data at all: reaching decode would fail
    // `Decode`, so a `Budget`/`Cap` proves the gate ran on the headers alone.
    let half = header_only(8_192, 4_096); // 2^25 pixels each
    let sum = compose(&[&half, &half, &half], &ComposeArgs::new());
    assert!(
        matches!(sum, Err(FxError::Budget { pixels, max }) if pixels == 3 << 25 && max == MAX_PIXELS)
    );
    let long = header_only(10_000, 1);
    let side = compose(&[&long, &long], &args(Layout::Row, Scale::None));
    assert!(
        matches!(&side, Err(FxError::Cap(m)) if m.contains("20000x1")),
        "{side:?}"
    );
    // A 1-pixel-high strip filling a square cell would scale past the side cap.
    let mut fill = args(Layout::Row, Scale::Fill);
    fill.cell = Some((4_000, 4_000));
    let strip = header_only(4_000, 1);
    assert!(matches!(compose(&[&strip], &fill), Err(FxError::Cap(_))));
    // The canvas pixel budget: two 6000×6000 fit cells side by side.
    let mut cells = args(Layout::Row, Scale::Fit);
    cells.cell = Some((6_000, 6_000));
    let dot = header_only(1, 1);
    assert!(matches!(
        compose(&[&dot, &dot], &cells),
        Err(FxError::Budget { .. })
    ));
}

#[test]
fn an_input_over_the_side_cap_is_a_cap_even_inside_the_pixel_budget() {
    // 20000x1 is far under 2^26 pixels: only the per-side cap can refuse it,
    // and the stdlib files that cap under `-005`, never a corrupt input.
    for (w, h) in [(20_000, 1), (1, 16_385)] {
        let claim = header_only(w, h);
        let r = compose(&[&claim], &ComposeArgs::new());
        assert!(
            matches!(&r, Err(FxError::Cap(m))
                if m.starts_with("inputs[0]:") && m.contains(&format!("{w}x{h}"))),
            "{r:?}"
        );
    }
}

#[test]
fn a_second_ihdr_cannot_smuggle_pixels_past_the_summed_budget() {
    // The header gate reads 1x1; the decoder must never paint the 64x64 a
    // second IHDR claims (five such inputs would be 5x the gated total).
    let smuggler = dual_ihdr((1, 1), (64, 64));
    let r = compose(&[&smuggler, &smuggler], &args(Layout::Row, Scale::None));
    assert!(
        matches!(&r, Err(FxError::Decode(m))
            if m.starts_with("inputs[0]:") && m.contains("second IHDR")),
        "{:?}",
        r.map(|o| (o.width, o.height))
    );
}

#[test]
fn input_failures_name_their_index_and_keep_their_class() {
    let good = flat(2, 2, RED);
    let not_png: &[u8] = b"GIF89a.......";
    assert!(matches!(
        compose(&[&good, not_png], &ComposeArgs::new()),
        Err(FxError::UnsupportedFormat(m)) if m.starts_with("inputs[1]: not a PNG")
    ));
    let mut corrupt = flat(4, 4, BLUE);
    let last = corrupt.len() - 13; // the last byte of the IDAT chunk's CRC
    corrupt[last] ^= 0x55;
    assert!(matches!(
        compose(&[&corrupt], &ComposeArgs::new()),
        Err(FxError::Decode(m)) if m.starts_with("inputs[0]:")
    ));
}

/// One cell's placed box is the composition's own geometry: `fit` keeps the
/// whole image inside the cell, `fill` covers it before the crop, `none`
/// keeps the image as it is — and the caps fire from the sizes alone.
#[test]
fn placed_size_is_the_one_cell_geometry_with_its_caps() {
    assert_eq!(
        placed_size(400, 300, Scale::Fit, (1280, 720)).ok(),
        Some((960, 720))
    );
    assert_eq!(
        placed_size(400, 300, Scale::Fill, (1280, 720)).ok(),
        Some((1280, 960))
    );
    assert_eq!(
        placed_size(400, 300, Scale::None, (1280, 720)).ok(),
        Some((400, 300))
    );
    // A small image scales UP to the cell.
    assert_eq!(placed_size(9, 7, Scale::Fit, (8, 8)).ok(), Some((8, 6)));
    // A very wide still filling a frame breaks the per-side cap.
    assert!(matches!(
        placed_size(16_384, 1, Scale::Fill, (3840, 2160)),
        Err(FxError::Cap(m)) if m.contains("16384 per-side cap")
    ));
    // `fit` never leaves the cell, so it never trips a cap the cell keeps.
    assert_eq!(
        placed_size(16_384, 1, Scale::Fit, (3840, 2160)).ok(),
        Some((3840, 1))
    );
    assert!(matches!(
        placed_size(0, 3, Scale::Fit, (8, 8)),
        Err(FxError::Args(_))
    ));
    assert!(matches!(
        placed_size(3, 3, Scale::Fill, (0, 8)),
        Err(FxError::Args(_))
    ));
}
