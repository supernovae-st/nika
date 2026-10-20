// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Supernovae butterfly, the only brand mark the renderer draws, and the
//! wordmark it is locked up with ([`WORDMARK`]: the name, then a dot in the
//! exec hue).
//!
//! Every rendition is text sampled from the repository's own logomark
//! ([`SOURCE_PATH`], sha256 [`SOURCE_SHA256`]): alpha coverage over 12×24
//! samples per cell, a cell half as wide as it is tall, no path edited, no
//! shape invented. The samples are drawn in one of two [`Glyphs`]: the
//! density ramp ` .:-=+*#%@` in the ASCII column, or Unicode braille, 2×4
//! dots per cell, each thresholded from the same samples. The sampler is a
//! test (`logomark/sample.rs`) that regenerates every rendition from the
//! pinned mark and fails on any difference. Five sizes exist; one is chosen
//! whole (a mark is never cut mid-line), and the layout decides where it may
//! stand: the workflow and the composer come first.
//!
//! The reveal is a 4×4 ordered dither in five frames (at 0, 300, 650, 1,000 and
//! 1,400 ms) over the ramp's cells or the braille's dots, each frame keeping
//! everything the previous one showed; the last frame is the exact mark, held
//! from 1,400 ms and final at [`REVEAL_ENDS`]. It never loops, never stands for
//! work in progress (a run's motion is its verb's, in the theme seam), and
//! under reduced motion the final frame is drawn at once.

use std::time::Duration;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::visual::role::{self, Depth, Tone};

/// The name the wordmark spells.
const NAME: &str = "nika";

/// The dot that closes the name.
const DOT: &str = ".";

/// The wordmark the butterfly is locked up with: the name and the dot that
/// closes it, the same characters in every glyph column.
pub const WORDMARK: &str = "nika.";

/// The wordmark as one line: the name in `name`, its dot in the exec hue at
/// `depth` (plain `nika.` without colour).
#[must_use]
pub fn wordmark(name: Style, depth: Depth) -> Line<'static> {
    Line::from(vec![
        Span::styled(NAME, name),
        Span::styled(DOT, role::tone(Tone::Dot, depth)),
    ])
}

/// Where the source logomark lives in this repository.
pub const SOURCE_PATH: &str = "media/brand/nika-logomark.svg";

/// The sha256 of the logomark every rendition was sampled from: a changed
/// mark makes the renditions stale, and the provenance test says so.
pub const SOURCE_SHA256: &str = "fe2240ff2d8a6f76bf6d68883697c29092ed2586c28083ec2014fad2a9d7dd94";

/// When each reveal frame is first drawn; the last one is the exact mark.
pub const REVEAL_AT: [Duration; 5] = [
    Duration::from_millis(0),
    Duration::from_millis(300),
    Duration::from_millis(650),
    Duration::from_millis(1000),
    Duration::from_millis(1400),
];

/// When a reveal is over: nothing about the mark changes after it.
pub const REVEAL_ENDS: Duration = Duration::from_millis(1800);

/// The glyphs a rendition is drawn with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Glyphs {
    /// The density ramp ` .:-=+*#%@`: printable ASCII, the ASCII column's.
    Ramp,
    /// Unicode braille, 2×4 dots per cell: eight times the ramp's points.
    Braille,
}

impl Glyphs {
    /// The glyphs of a glyph column: the ramp in ASCII, braille otherwise.
    #[must_use]
    pub const fn of(ascii: bool) -> Self {
        if ascii { Self::Ramp } else { Self::Braille }
    }
}

/// One rendition of the mark, named by where it may stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Size {
    /// 12×6: the last fallback before no mark at all.
    Tiny,
    /// 16×8: the compact mark beside the header, quiet once revealed.
    Compact,
    /// 24×10: an empty wide pane or the project selector.
    Picker,
    /// 32×14: the first launch, or an explicit return to the welcome.
    Launch,
    /// 48×20: a very large welcome only.
    Board,
}

impl Size {
    /// Every size, smallest first.
    pub const ALL: [Self; 5] = [
        Self::Tiny,
        Self::Compact,
        Self::Picker,
        Self::Launch,
        Self::Board,
    ];

    /// The cells one row takes.
    #[must_use]
    pub const fn columns(self) -> u16 {
        match self {
            Self::Tiny => 12,
            Self::Compact => 16,
            Self::Picker => 24,
            Self::Launch => 32,
            Self::Board => 48,
        }
    }

    /// The terminal rows the mark takes: real rows, never scaled.
    #[must_use]
    pub const fn rows(self) -> u16 {
        match self {
            Self::Tiny => 6,
            Self::Compact => 8,
            Self::Picker => 10,
            Self::Launch => 14,
            Self::Board => 20,
        }
    }

    /// The five frames in `glyphs`, each row closed by `|` so no row ends in
    /// a space, frames separated by a `~` line.
    const fn data(self, glyphs: Glyphs) -> &'static str {
        match (glyphs, self) {
            (Glyphs::Ramp, Self::Tiny) => include_str!("logomark/butterfly-12x6.txt"),
            (Glyphs::Ramp, Self::Compact) => include_str!("logomark/butterfly-16x8.txt"),
            (Glyphs::Ramp, Self::Picker) => include_str!("logomark/butterfly-24x10.txt"),
            (Glyphs::Ramp, Self::Launch) => include_str!("logomark/butterfly-32x14.txt"),
            (Glyphs::Ramp, Self::Board) => include_str!("logomark/butterfly-48x20.txt"),
            (Glyphs::Braille, Self::Tiny) => include_str!("logomark/butterfly-12x6-braille.txt"),
            (Glyphs::Braille, Self::Compact) => {
                include_str!("logomark/butterfly-16x8-braille.txt")
            }
            (Glyphs::Braille, Self::Picker) => {
                include_str!("logomark/butterfly-24x10-braille.txt")
            }
            (Glyphs::Braille, Self::Launch) => {
                include_str!("logomark/butterfly-32x14-braille.txt")
            }
            (Glyphs::Braille, Self::Board) => {
                include_str!("logomark/butterfly-48x20-braille.txt")
            }
        }
    }

    /// The largest rendition that fits whole in `columns` × `rows`; none when
    /// even the smallest does not, and then no mark is drawn.
    #[must_use]
    pub fn largest_within(columns: u16, rows: u16) -> Option<Self> {
        Self::ALL
            .into_iter()
            .rev()
            .find(|size| size.columns() <= columns && size.rows() <= rows)
    }

    /// The rows of reveal frame `index` (`0..5`) in `glyphs`; empty past the
    /// last frame.
    #[must_use]
    pub fn frame(self, glyphs: Glyphs, index: usize) -> Vec<&'static str> {
        self.data(glyphs)
            .split("~\n")
            .nth(index)
            .map(|frame| {
                frame
                    .lines()
                    .map(|row| row.strip_suffix('|').unwrap_or(row))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The exact mark in `glyphs` (the reveal's last frame).
    #[must_use]
    pub fn lines(self, glyphs: Glyphs) -> Vec<&'static str> {
        self.frame(glyphs, REVEAL_AT.len() - 1)
    }

    /// The rows to draw in `glyphs` `elapsed` after the reveal began: the
    /// frame whose time has come, or the exact mark at once under reduced
    /// motion.
    #[must_use]
    pub fn at(self, glyphs: Glyphs, elapsed: Duration, reduced_motion: bool) -> Vec<&'static str> {
        self.frame(glyphs, frame_at(elapsed, reduced_motion))
    }
}

/// The reveal frame drawn `elapsed` after the reveal began: the last whose
/// time has come, or the exact mark (the last frame) at once under reduced
/// motion.
#[must_use]
pub fn frame_at(elapsed: Duration, reduced_motion: bool) -> usize {
    if reduced_motion {
        return REVEAL_AT.len() - 1;
    }
    REVEAL_AT
        .iter()
        .rposition(|start| elapsed >= *start)
        .unwrap_or(0)
}

/// Whether a reveal begun `elapsed` ago still asks for a repaint: never under
/// reduced motion, never once [`REVEAL_ENDS`] has passed.
#[must_use]
pub fn revealing(elapsed: Duration, reduced_motion: bool) -> bool {
    !reduced_motion && elapsed < REVEAL_ENDS
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod sample;

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    use unicode_width::UnicodeWidthStr;

    const BOTH: [Glyphs; 2] = [Glyphs::Ramp, Glyphs::Braille];

    /// The dots of a braille glyph (a blank cell has none).
    fn dots(glyph: char) -> u32 {
        if glyph == ' ' {
            0
        } else {
            u32::from(glyph) - 0x2800
        }
    }

    #[test]
    fn every_size_is_five_exact_rectangles_in_each_glyph_set() {
        for size in Size::ALL {
            for glyphs in BOTH {
                let count = size.data(glyphs).split("~\n").count();
                assert_eq!(count, REVEAL_AT.len(), "{size:?} {glyphs:?}");
                for index in 0..REVEAL_AT.len() {
                    let frame = size.frame(glyphs, index);
                    assert_eq!(frame.len(), usize::from(size.rows()), "{size:?} {index}");
                    for row in frame {
                        let cells = usize::from(size.columns());
                        assert_eq!(row.chars().count(), cells, "{size:?} {row:?}");
                        assert_eq!(row.width(), cells, "{size:?} {row:?}");
                        let drawn = |glyph: char| match glyphs {
                            Glyphs::Ramp => (' '..='~').contains(&glyph),
                            // A blank cell is a space, never the empty pattern.
                            Glyphs::Braille => {
                                glyph == ' ' || ('\u{2801}'..='\u{28ff}').contains(&glyph)
                            }
                        };
                        assert!(row.chars().all(drawn), "{glyphs:?} {row:?}");
                    }
                }
                assert!(size.frame(glyphs, REVEAL_AT.len()).is_empty(), "{size:?}");
            }
        }
    }

    #[test]
    fn a_revealed_cell_or_dot_never_moves_or_goes_out() {
        for size in Size::ALL {
            for glyphs in BOTH {
                for index in 1..REVEAL_AT.len() {
                    let before = size.frame(glyphs, index - 1);
                    let after = size.frame(glyphs, index);
                    for (was, now) in before.iter().zip(&after) {
                        for (a, b) in was.chars().zip(now.chars()) {
                            let kept = match glyphs {
                                Glyphs::Ramp => a == ' ' || a == b,
                                Glyphs::Braille => dots(a) & dots(b) == dots(a),
                            };
                            assert!(kept, "{size:?} {glyphs:?} frame {index}: {was:?} {now:?}");
                        }
                    }
                }
                // The reveal ends on the exact mark and shows more than it began with.
                let ink = |rows: &[&str]| {
                    rows.iter()
                        .flat_map(|r| r.chars())
                        .map(|glyph| match glyphs {
                            Glyphs::Ramp => u32::from(glyph != ' '),
                            Glyphs::Braille => dots(glyph).count_ones(),
                        })
                        .sum::<u32>()
                };
                let (first, last) = (size.frame(glyphs, 0), size.lines(glyphs));
                assert!(ink(&first) < ink(&last), "{size:?} {glyphs:?}");
            }
        }
    }

    #[test]
    fn braille_draws_only_where_the_ramp_has_ink() {
        // One sampling: a dot needs half of its 36 samples, a ramp cell is
        // blank only below 16 of its 288.
        for size in Size::ALL {
            let ramp = size.lines(Glyphs::Ramp);
            let braille = size.lines(Glyphs::Braille);
            for (ramp, braille) in ramp.iter().zip(&braille) {
                for (cell, glyph) in ramp.chars().zip(braille.chars()) {
                    assert!(
                        glyph == ' ' || cell != ' ',
                        "{size:?}: {ramp:?} {braille:?}"
                    );
                }
            }
        }
        assert_eq!(Glyphs::of(true), Glyphs::Ramp);
        assert_eq!(Glyphs::of(false), Glyphs::Braille);
    }

    #[test]
    fn the_reveal_follows_its_clock_and_reduced_motion_shows_the_mark_at_once() {
        let size = Size::Launch;
        let ms = Duration::from_millis;
        for glyphs in BOTH {
            let at = |elapsed, reduced| size.at(glyphs, elapsed, reduced);
            assert_eq!(at(ms(0), false), size.frame(glyphs, 0));
            assert_eq!(at(ms(299), false), size.frame(glyphs, 0));
            assert_eq!(at(ms(300), false), size.frame(glyphs, 1));
            assert_eq!(at(ms(1399), false), size.frame(glyphs, 3));
            assert_eq!(at(ms(1400), false), size.lines(glyphs));
            assert_eq!(at(ms(5_000), false), size.lines(glyphs));
            assert_eq!(at(ms(0), true), size.lines(glyphs));
        }
        assert_eq!(frame_at(ms(0), false), 0);
        assert_eq!(frame_at(ms(649), false), 1);
        assert_eq!(frame_at(ms(650), false), 2);
        assert_eq!(frame_at(ms(1_400), false), REVEAL_AT.len() - 1);
        assert_eq!(frame_at(ms(0), true), REVEAL_AT.len() - 1);
        assert!(revealing(ms(0), false));
        assert!(revealing(ms(1799), false));
        assert!(!revealing(REVEAL_ENDS, false));
        assert!(!revealing(ms(0), true));
    }

    #[test]
    fn the_wordmark_is_the_name_and_its_exec_dot() {
        assert_eq!(format!("{NAME}{DOT}"), WORDMARK);
        assert!(
            WORDMARK.is_ascii(),
            "the same characters in the ASCII column"
        );
        let strong = Style::default().add_modifier(ratatui::style::Modifier::BOLD);
        let line = wordmark(strong, Depth::Rgb);
        assert_eq!(line.to_string(), WORDMARK);
        assert_eq!(line.spans[0].style, strong);
        assert_eq!(line.spans[1].style, role::tone(Tone::Dot, Depth::Rgb));
        assert_eq!(
            line.spans[1].style.fg,
            Some(ratatui::style::Color::Rgb(0xff, 0x7a, 0x3c))
        );
        // Without colour the dot is the plain character.
        let plain = wordmark(Style::default(), Depth::None);
        assert!(
            plain
                .spans
                .iter()
                .all(|span| span.style == Style::default())
        );
        assert_eq!(plain.to_string(), "nika.");
    }

    #[test]
    fn a_size_is_chosen_whole_or_not_at_all() {
        assert_eq!(Size::largest_within(80, 24), Some(Size::Board));
        assert_eq!(Size::largest_within(47, 24), Some(Size::Launch));
        assert_eq!(Size::largest_within(40, 13), Some(Size::Picker));
        assert_eq!(Size::largest_within(20, 9), Some(Size::Compact));
        assert_eq!(Size::largest_within(12, 6), Some(Size::Tiny));
        assert_eq!(Size::largest_within(11, 40), None);
        assert_eq!(Size::largest_within(200, 5), None);
    }

    #[test]
    fn the_renditions_still_name_the_repository_mark() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(SOURCE_PATH);
        let bytes = std::fs::read(&path).expect("the repository logomark");
        let digest = Sha256::digest(&bytes)
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            });
        assert_eq!(
            digest, SOURCE_SHA256,
            "{SOURCE_PATH} changed: sample the renditions again from the new mark"
        );
    }
}
