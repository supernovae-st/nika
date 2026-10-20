// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The sampler every rendition comes from, kept as a test so the committed
//! renditions are proved, not trusted. It reads the repository's logomark
//! ([`SOURCE_PATH`]) as text, flattens its one path (`M`, `L`, `C`, `Z`) into
//! straight edges, [`SEGMENTS`] per cubic, far below a sample's width at
//! every size, and judges 12×24 point samples per cell inside by the
//! non-zero winding rule, each at the centre of its square of the mark. The
//! mark is square: it spans twice as many columns as rows (a cell is half as
//! wide as it is tall), centred in its rendition.
//!
//! A ramp cell takes the step of ` .:-=+*#%@` nearest its share of inside
//! samples; a braille dot (2×4 per cell, 6×6 samples each) is drawn when at
//! least half of its samples are inside, so the two glyph sets read the same
//! samples. Each reveal frame keeps a ramp cell, or a braille dot, whose 4×4
//! Bayer value is below that frame's cut.
//!
//! `cargo test -p nika-tui-view` regenerates every rendition from the pinned
//! mark and fails on any difference; `NIKA_LOGOMARK_WRITE=1` writes them.

use std::path::{Path, PathBuf};

use super::{Glyphs, REVEAL_AT, SOURCE_PATH, Size};

/// Samples across one cell.
const ACROSS: usize = 12;
/// Samples down one cell.
const DOWN: usize = 24;
/// Samples across (and down) one braille dot.
const DOT: usize = 6;
/// Straight edges per cubic.
const SEGMENTS: u32 = 64;
/// The density ramp, sparsest first.
const RAMP: &[u8; 10] = b" .:-=+*#%@";
/// The 4×4 ordered-dither matrix, by row `y % 4` then column `x % 4`.
const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
/// Frame `k` keeps what its Bayer value puts below `CUTS[k]`.
const CUTS: [u8; 5] = [2, 5, 9, 13, 16];
/// The bit of each braille dot, by its column then its row in the cell.
const DOT_BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

type Point = (f64, f64);

/// One token of path data: a command letter, or a number.
type Token = Result<char, f64>;

/// The mark: the side of its square view box and every edge of its path.
struct Mark {
    side: f64,
    edges: Vec<(Point, Point)>,
}

/// The value of attribute `name` in `svg`.
fn attribute<'a>(svg: &'a str, name: &str) -> &'a str {
    let key = format!(" {name}=\"");
    svg.find(&key)
        .map(|at| at + key.len())
        .and_then(|start| Some(&svg[start..start + svg[start..].find('"')?]))
        .unwrap_or_else(|| panic!("the logomark has no {name}"))
}

/// The path data as commands and numbers.
fn tokens(data: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = data;
    while let Some(head) = rest.chars().next() {
        if head.is_ascii_alphabetic() {
            out.push(Ok(head));
            rest = &rest[1..];
        } else if head.is_ascii_whitespace() || head == ',' {
            rest = &rest[1..];
        } else {
            // A sign, digits and at most one dot: `1.5.5` is `1.5` then `.5`.
            let mut dot = false;
            let end = rest
                .char_indices()
                .find(|&(at, glyph)| {
                    let sign = at == 0 && matches!(glyph, '-' | '+');
                    let first_dot = glyph == '.' && !dot;
                    dot |= first_dot;
                    !(sign || first_dot || glyph.is_ascii_digit())
                })
                .map_or(rest.len(), |(at, _)| at);
            let number = rest[..end]
                .parse()
                .unwrap_or_else(|_| panic!("not a number at {:?}", rest.get(..8)));
            out.push(Err(number));
            rest = &rest[end..];
        }
    }
    out
}

/// The number at `index` in `tokens`, which then moves past it.
fn coordinate(tokens: &[Token], index: &mut usize) -> f64 {
    let value = match tokens.get(*index) {
        Some(Err(value)) => *value,
        other => panic!("a coordinate, not {other:?}"),
    };
    *index += 1;
    value
}

/// A cubic from `from` through `one` and `two` to `to`, as straight edges.
fn flatten(from: Point, one: Point, two: Point, to: Point) -> Vec<(Point, Point)> {
    let mut edges = Vec::new();
    let mut last = from;
    for step in 1..=SEGMENTS {
        let along = f64::from(step) / f64::from(SEGMENTS);
        let back = 1.0 - along;
        let weights = [
            back * back * back,
            3.0 * back * back * along,
            3.0 * back * along * along,
            along * along * along,
        ];
        let blend = |pick: fn(Point) -> f64| {
            weights[0] * pick(from)
                + weights[1] * pick(one)
                + weights[2] * pick(two)
                + weights[3] * pick(to)
        };
        let point = (blend(|p| p.0), blend(|p| p.1));
        edges.push((last, point));
        last = point;
    }
    edges
}

/// Every straight edge of the path in `data`, each subpath closed.
fn edges(data: &str) -> Vec<(Point, Point)> {
    let tokens = tokens(data);
    let mut out = Vec::new();
    let (mut start, mut pen) = ((0.0, 0.0), (0.0, 0.0));
    let mut command = 'M';
    let mut index = 0;
    while index < tokens.len() {
        if let Ok(letter) = tokens[index] {
            command = letter;
            index += 1;
            if command == 'Z' {
                out.push((pen, start));
                pen = start;
            }
            continue;
        }
        let mut next = || coordinate(&tokens, &mut index);
        match command {
            'M' => {
                if pen != start {
                    out.push((pen, start));
                }
                pen = (next(), next());
                start = pen;
                // Coordinates after a move draw lines.
                command = 'L';
            }
            'L' => {
                let to = (next(), next());
                out.push((pen, to));
                pen = to;
            }
            'C' => {
                let (one, two, to) = ((next(), next()), (next(), next()), (next(), next()));
                out.extend(flatten(pen, one, two, to));
                pen = to;
            }
            other => panic!("the sampler reads M, L, C and Z, not {other:?}: teach it first"),
        }
    }
    if pen != start {
        out.push((pen, start));
    }
    out
}

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The repository's logomark, read and flattened.
fn read_mark() -> Mark {
    let path = manifest().join("../..").join(SOURCE_PATH);
    let svg = std::fs::read_to_string(&path).expect("the repository logomark");
    assert_eq!(
        svg.matches("<path").count(),
        1,
        "the sampler reads one path"
    );
    for refused in ["transform", "fill-rule", "clip-path", "<use"] {
        assert!(
            !svg.contains(refused),
            "the sampler does not read {refused}"
        );
    }
    let view: Vec<f64> = attribute(&svg, "viewBox")
        .split_whitespace()
        .map(|number| number.parse().expect("a view box number"))
        .collect();
    assert_eq!(view.len(), 4, "{view:?}");
    assert!(
        view[0] == 0.0 && view[1] == 0.0 && (view[2] - view[3]).abs() < f64::EPSILON,
        "a square view box at the origin: {view:?}"
    );
    Mark {
        side: view[2],
        edges: edges(attribute(&svg, "d")),
    }
}

/// Whether each sample of `size`'s square mark is inside, row by row.
fn samples(mark: &Mark, size: Size) -> Vec<Vec<bool>> {
    let across = usize::from(size.rows()) * DOWN;
    let scale = mark.side / f64::from(u32::try_from(across).expect("a small grid"));
    let centre = |index: usize| (f64::from(u32::try_from(index).expect("small")) + 0.5) * scale;
    (0..across)
        .map(|row| {
            let y = centre(row);
            let crossings: Vec<(f64, i32)> = mark
                .edges
                .iter()
                .filter_map(|&((x0, y0), (x1, y1))| {
                    let winding = if y0 <= y && y < y1 {
                        1
                    } else if y1 <= y && y < y0 {
                        -1
                    } else {
                        return None;
                    };
                    Some((x0 + (y - y0) * (x1 - x0) / (y1 - y0), winding))
                })
                .collect();
            (0..across)
                .map(|column| {
                    let x = centre(column);
                    let winding: i32 = crossings
                        .iter()
                        .filter(|(at, _)| *at > x)
                        .map(|(_, winding)| winding)
                        .sum();
                    winding != 0
                })
                .collect()
        })
        .collect()
}

/// How many samples are inside the `width` × `height` block at `x`, `y`.
fn inside(grid: &[Vec<bool>], (x, y): (usize, usize), (width, height): (usize, usize)) -> usize {
    grid[y..y + height]
        .iter()
        .map(|row| row[x..x + width].iter().filter(|&&on| on).count())
        .sum()
}

/// The final mark of `size` in `glyphs`: per cell, a ramp step (`0..10`) or
/// a braille bit set, by rendition row then column (margins blank).
fn cells(grid: &[Vec<bool>], size: Size, glyphs: Glyphs) -> Vec<Vec<u32>> {
    let rows = usize::from(size.rows());
    let columns = usize::from(size.columns());
    let margin = (columns - 2 * rows) / 2;
    assert_eq!(columns, 2 * rows + 2 * margin, "{size:?}: a centred mark");
    (0..rows)
        .map(|row| {
            (0..columns)
                .map(|column| {
                    let Some(cell) = column.checked_sub(margin).filter(|c| *c < 2 * rows) else {
                        return 0;
                    };
                    let (x, y) = (cell * ACROSS, row * DOWN);
                    match glyphs {
                        Glyphs::Ramp => {
                            let count = inside(grid, (x, y), (ACROSS, DOWN));
                            let steps = RAMP.len() - 1;
                            let total = ACROSS * DOWN;
                            let step = (steps * count + total / 2) / total;
                            u32::try_from(step).expect("a ramp step")
                        }
                        Glyphs::Braille => {
                            let mut bits = 0;
                            for (dx, column_bits) in DOT_BITS.iter().enumerate() {
                                for (dy, bit) in column_bits.iter().enumerate() {
                                    let at = (x + dx * DOT, y + dy * DOT);
                                    if 2 * inside(grid, at, (DOT, DOT)) >= DOT * DOT {
                                        bits |= bit;
                                    }
                                }
                            }
                            bits
                        }
                    }
                })
                .collect()
        })
        .collect()
}

/// Reveal frame `index` of `glyphs` from the final `cells`, as rows.
fn frame(cells: &[Vec<u32>], glyphs: Glyphs, index: usize) -> Vec<String> {
    let cut = CUTS[index];
    cells
        .iter()
        .enumerate()
        .map(|(row, line)| {
            line.iter()
                .enumerate()
                .map(|(column, &cell)| match glyphs {
                    Glyphs::Ramp if BAYER[row % 4][column % 4] < cut => {
                        char::from(RAMP[usize::try_from(cell).expect("a step")])
                    }
                    Glyphs::Ramp => ' ',
                    Glyphs::Braille => {
                        let mut bits = 0;
                        for (dx, column_bits) in DOT_BITS.iter().enumerate() {
                            for (dy, bit) in column_bits.iter().enumerate() {
                                if BAYER[dy][(2 * column + dx) % 4] < cut {
                                    bits |= cell & bit;
                                }
                            }
                        }
                        if bits == 0 {
                            ' '
                        } else {
                            char::from_u32(0x2800 + bits).expect("a braille pattern")
                        }
                    }
                })
                .collect()
        })
        .collect()
}

/// The text of a rendition: its five frames, each row closed by `|`, the
/// frames separated by a `~` line.
fn text(cells: &[Vec<u32>], glyphs: Glyphs) -> String {
    let mut out = String::new();
    for index in 0..REVEAL_AT.len() {
        if index > 0 {
            out.push_str("~\n");
        }
        for row in frame(cells, glyphs, index) {
            out.push_str(&row);
            out.push_str("|\n");
        }
    }
    out
}

/// Where the rendition of `size` in `glyphs` lives.
fn file(size: Size, glyphs: Glyphs) -> PathBuf {
    let suffix = match glyphs {
        Glyphs::Ramp => "",
        Glyphs::Braille => "-braille",
    };
    manifest().join(format!(
        "src/visual/logomark/butterfly-{}x{}{suffix}.txt",
        size.columns(),
        size.rows()
    ))
}

/// The write switch; a test-only flag, never a secret.
#[allow(
    clippy::disallowed_methods,
    reason = "a test-only switch that rewrites the renditions, not a secret"
)]
fn writing() -> bool {
    std::env::var_os("NIKA_LOGOMARK_WRITE").is_some_and(|value| value == "1")
}

#[test]
fn every_rendition_is_sampled_from_the_repository_mark() {
    let mark = read_mark();
    let mut stale = Vec::new();
    for size in Size::ALL {
        let grid = samples(&mark, size);
        for glyphs in [Glyphs::Ramp, Glyphs::Braille] {
            let sampled = text(&cells(&grid, size, glyphs), glyphs);
            if sampled == size.data(glyphs) {
                continue;
            }
            if writing() {
                std::fs::write(file(size, glyphs), &sampled).expect("write the rendition");
            }
            stale.push(file(size, glyphs).display().to_string());
        }
    }
    assert!(
        stale.is_empty() || writing(),
        "stale renditions (rerun with NIKA_LOGOMARK_WRITE=1, then rebuild): {stale:#?}"
    );
}

#[test]
fn the_sampler_reads_the_path_it_is_given() {
    // A square drawn one way and a hole drawn the other: the non-zero rule
    // leaves the hole empty.
    let square = edges("M0 0L10 0L10 10L0 10ZM3 3L3 7L7 7L7 3Z");
    assert_eq!(square.len(), 8, "{square:?}");
    let mark = Mark {
        side: 10.0,
        edges: square,
    };
    // Tiny is 144 samples a side: sample 14 is centred near 1, 72 near 5,
    // 129 near 9.
    let grid = samples(&mark, Size::Tiny);
    assert!(grid[14][14] && grid[129][129], "the square is inside");
    assert!(!grid[72][72], "the hole is outside");
    // Numbers run together as SVG writes them; a curve becomes its segments.
    assert_eq!(
        tokens("M1.5.5-2C0 0 1 1 2 2"),
        [
            Ok('M'),
            Err(1.5),
            Err(0.5),
            Err(-2.0),
            Ok('C'),
            Err(0.0),
            Err(0.0),
            Err(1.0),
            Err(1.0),
            Err(2.0),
            Err(2.0),
        ]
    );
    let curve = edges("M0 0C0 0 1 1 2 2Z");
    assert_eq!(curve.len(), usize::try_from(SEGMENTS).expect("small") + 1);
    assert_eq!(
        curve[curve.len() - 2].1,
        (2.0, 2.0),
        "the curve ends on its point"
    );
}
