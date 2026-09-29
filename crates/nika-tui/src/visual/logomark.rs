// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Supernovae butterfly, the only brand mark the renderer draws.
//!
//! Every rendition is text sampled from the repository's own logomark
//! ([`SOURCE_PATH`], sha256 [`SOURCE_SHA256`]): alpha coverage over 12×24
//! samples per cell, a cell half as wide as it is tall, the density ramp
//! ` .:-=+*#%@`, no path edited, no shape invented. Five sizes exist; one is
//! chosen whole (a mark is never cut mid-line), and the layout decides where
//! it may stand: the workflow and the composer come first.
//!
//! The reveal is a 4×4 ordered dither in five frames (at 0, 140, 280, 420 and
//! 600 ms), each keeping every cell the previous one showed; the last frame is
//! the exact mark, held from 600 ms and final at [`REVEAL_ENDS`]. It never
//! loops, never stands for work in progress (a run's motion is its verb's, in
//! the theme seam), and under reduced motion the final frame is drawn at once.

use std::time::Duration;

/// Where the source logomark lives in this repository.
pub const SOURCE_PATH: &str = "media/brand/nika-logomark.svg";

/// The sha256 of the logomark every rendition was sampled from: a changed
/// mark makes the renditions stale, and the provenance test says so.
pub const SOURCE_SHA256: &str = "fe2240ff2d8a6f76bf6d68883697c29092ed2586c28083ec2014fad2a9d7dd94";

/// When each reveal frame is first drawn; the last one is the exact mark.
pub const REVEAL_AT: [Duration; 5] = [
    Duration::from_millis(0),
    Duration::from_millis(140),
    Duration::from_millis(280),
    Duration::from_millis(420),
    Duration::from_millis(600),
];

/// When a reveal is over: nothing about the mark changes after it.
pub const REVEAL_ENDS: Duration = Duration::from_millis(760);

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

    /// The five frames, each row closed by `|` so no row ends in a space,
    /// frames separated by a `~` line.
    const fn data(self) -> &'static str {
        match self {
            Self::Tiny => include_str!("logomark/butterfly-12x6.txt"),
            Self::Compact => include_str!("logomark/butterfly-16x8.txt"),
            Self::Picker => include_str!("logomark/butterfly-24x10.txt"),
            Self::Launch => include_str!("logomark/butterfly-32x14.txt"),
            Self::Board => include_str!("logomark/butterfly-48x20.txt"),
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

    /// The rows of reveal frame `index` (`0..5`); empty past the last frame.
    #[must_use]
    pub fn frame(self, index: usize) -> Vec<&'static str> {
        self.data()
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

    /// The exact mark (the reveal's last frame).
    #[must_use]
    pub fn lines(self) -> Vec<&'static str> {
        self.frame(REVEAL_AT.len() - 1)
    }

    /// The rows to draw `elapsed` after the reveal began: the frame whose time
    /// has come, or the exact mark at once under reduced motion.
    #[must_use]
    pub fn at(self, elapsed: Duration, reduced_motion: bool) -> Vec<&'static str> {
        if reduced_motion {
            return self.lines();
        }
        let index = REVEAL_AT
            .iter()
            .rposition(|start| elapsed >= *start)
            .unwrap_or(0);
        self.frame(index)
    }
}

/// Whether a reveal begun `elapsed` ago still asks for a repaint: never under
/// reduced motion, never once [`REVEAL_ENDS`] has passed.
#[must_use]
pub fn revealing(elapsed: Duration, reduced_motion: bool) -> bool {
    !reduced_motion && elapsed < REVEAL_ENDS
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    #[test]
    fn every_size_is_five_exact_ascii_rectangles() {
        for size in Size::ALL {
            let count = size.data().split("~\n").count();
            assert_eq!(count, REVEAL_AT.len(), "{size:?}");
            for index in 0..REVEAL_AT.len() {
                let frame = size.frame(index);
                assert_eq!(frame.len(), usize::from(size.rows()), "{size:?} {index}");
                for row in frame {
                    assert_eq!(row.len(), usize::from(size.columns()), "{size:?} {row:?}");
                    assert!(row.bytes().all(|b| (b' '..=b'~').contains(&b)), "{row:?}");
                }
            }
            assert!(size.frame(REVEAL_AT.len()).is_empty(), "{size:?}");
        }
    }

    #[test]
    fn a_revealed_cell_never_moves_or_changes() {
        for size in Size::ALL {
            for index in 1..REVEAL_AT.len() {
                let before = size.frame(index - 1);
                let after = size.frame(index);
                for (was, now) in before.iter().zip(&after) {
                    for (a, b) in was.chars().zip(now.chars()) {
                        assert!(
                            a == ' ' || a == b,
                            "{size:?} frame {index}: {was:?} {now:?}"
                        );
                    }
                }
            }
            // The reveal ends on the exact mark and shows more than it began with.
            let ink = |rows: &[&str]| {
                rows.iter()
                    .flat_map(|r| r.chars())
                    .filter(|c| *c != ' ')
                    .count()
            };
            assert!(ink(&size.frame(0)) < ink(&size.lines()), "{size:?}");
        }
    }

    #[test]
    fn the_reveal_follows_its_clock_and_reduced_motion_shows_the_mark_at_once() {
        let size = Size::Launch;
        let ms = Duration::from_millis;
        assert_eq!(size.at(ms(0), false), size.frame(0));
        assert_eq!(size.at(ms(139), false), size.frame(0));
        assert_eq!(size.at(ms(140), false), size.frame(1));
        assert_eq!(size.at(ms(599), false), size.frame(3));
        assert_eq!(size.at(ms(600), false), size.lines());
        assert_eq!(size.at(ms(5_000), false), size.lines());
        assert_eq!(size.at(ms(0), true), size.lines());
        assert!(revealing(ms(0), false));
        assert!(revealing(ms(759), false));
        assert!(!revealing(REVEAL_ENDS, false));
        assert!(!revealing(ms(0), true));
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
