// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where each region of the workspace screen stands at a given size.
//!
//! The composer and the object come first: at 80×24 the aside is folded and
//! the conversation sits below the object; from 100 columns the conversation
//! stands beside the object; from 120 columns the project aside appears. Every
//! region is a whole rectangle, no two overlap, and together they cover the
//! screen exactly, so a resize never leaves a stale cell or cuts a region in
//! the middle of a row.

use ratatui::layout::Rect;

/// The narrowest terminal that shows the object and the conversation side by side.
pub const SIDE_BY_SIDE_MIN_WIDTH: u16 = 100;
/// The narrowest terminal that also shows the project aside.
pub const ASIDE_MIN_WIDTH: u16 = 120;
/// The shortest terminal that gives the header its second row (the location).
pub const TALL_HEADER_MIN_HEIGHT: u16 = 30;
/// Below this size the workspace does not fit and the caller keeps the inline
/// presentation, which never needs more than a few rows.
pub const MIN_SIZE: (u16, u16) = (60, 16);

/// The regions of one workspace frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Geometry {
    /// The header: host, active project, location (one or two rows).
    pub header: Rect,
    /// The project aside (Nika · Files), when the width allows it.
    pub aside: Option<Rect>,
    /// The object in view (workflow, file, run, proof).
    pub object: Rect,
    /// The conversation thread and its composer.
    pub conversation: Rect,
    /// The pinned activity row, when a run is pinned.
    pub pinned: Option<Rect>,
    /// The conversation sits below the object instead of beside it.
    pub stacked: bool,
}

impl Geometry {
    /// The regions of a frame of `area`, with one pinned activity row when
    /// `pinned`; none when the terminal is smaller than [`MIN_SIZE`].
    #[must_use]
    pub fn of(area: Rect, pinned: bool) -> Option<Self> {
        if area.width < MIN_SIZE.0 || area.height < MIN_SIZE.1 {
            return None;
        }
        let header_rows = if area.height >= TALL_HEADER_MIN_HEIGHT {
            2
        } else {
            1
        };
        let pinned_rows = u16::from(pinned);
        let header = Rect::new(area.x, area.y, area.width, header_rows);
        let body_height = area.height - header_rows - pinned_rows;
        let body = Rect::new(area.x, area.y + header_rows, area.width, body_height);
        let pinned = pinned.then(|| Rect::new(area.x, body.bottom(), area.width, 1));
        let (aside, work) = if area.width >= ASIDE_MIN_WIDTH {
            let width = clamp_share(area.width, 18, 20, 32);
            (
                Some(Rect::new(body.x, body.y, width, body.height)),
                Rect::new(body.x + width, body.y, body.width - width, body.height),
            )
        } else {
            (None, body)
        };
        let stacked = area.width < SIDE_BY_SIDE_MIN_WIDTH;
        let (object, conversation) = if stacked {
            // The conversation keeps at least half the rows: the composer is never squeezed out.
            let talk = work.height.div_ceil(2);
            let seen = work.height - talk;
            (
                Rect::new(work.x, work.y, work.width, seen),
                Rect::new(work.x, work.y + seen, work.width, talk),
            )
        } else {
            let talk = clamp_share(work.width, 38, 36, 56);
            let seen = work.width - talk;
            (
                Rect::new(work.x, work.y, seen, work.height),
                Rect::new(work.x + seen, work.y, talk, work.height),
            )
        };
        Some(Self {
            header,
            aside,
            object,
            conversation,
            pinned,
            stacked,
        })
    }

    /// Every region shown, in reading order.
    #[must_use]
    pub fn regions(&self) -> Vec<Rect> {
        let mut regions = vec![self.header];
        regions.extend(self.aside);
        regions.push(self.object);
        regions.push(self.conversation);
        regions.extend(self.pinned);
        regions
    }
}

/// `percent` of `total`, kept within `[min, max]`.
fn clamp_share(total: u16, percent: u16, min: u16, max: u16) -> u16 {
    let share = u32::from(total) * u32::from(percent) / 100;
    u16::try_from(share).unwrap_or(max).clamp(min, max)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// The four sizes the terminal matrix qualifies.
    const SIZES: [(u16, u16); 4] = [(80, 24), (100, 32), (120, 40), (160, 48)];

    fn at(width: u16, height: u16, pinned: bool) -> Geometry {
        Geometry::of(Rect::new(0, 0, width, height), pinned).expect("fits")
    }

    fn cells(rect: Rect) -> u32 {
        u32::from(rect.width) * u32::from(rect.height)
    }

    #[test]
    fn the_regions_cover_the_screen_exactly_without_overlap() {
        for (width, height) in SIZES {
            for pinned in [false, true] {
                let geometry = at(width, height, pinned);
                let regions = geometry.regions();
                let area = Rect::new(0, 0, width, height);
                let covered: u32 = regions.iter().copied().map(cells).sum();
                assert_eq!(covered, cells(area), "{width}x{height} pinned={pinned}");
                for (i, a) in regions.iter().enumerate() {
                    assert_eq!(
                        a.intersection(area),
                        *a,
                        "{width}x{height}: {a:?} leaves the screen"
                    );
                    for b in regions.iter().skip(i + 1) {
                        assert!(!a.intersects(*b), "{width}x{height}: {a:?} meets {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_composer_and_the_object_come_first_on_a_small_terminal() {
        let small = at(80, 24, true);
        assert!(small.stacked);
        assert_eq!(small.aside, None);
        assert_eq!(small.header.height, 1);
        assert!(small.conversation.height >= small.object.height);
        assert!(small.conversation.height >= 10, "{small:?}");
        assert_eq!(small.object.width, 80);
    }

    #[test]
    fn wider_terminals_open_the_conversation_beside_then_the_aside() {
        let medium = at(100, 32, false);
        assert!(!medium.stacked);
        assert_eq!(medium.aside, None);
        assert_eq!(medium.header.height, 2);
        assert!(medium.conversation.width >= 36);
        let large = at(120, 40, false);
        let aside = large.aside.expect("aside at 120 columns");
        assert!((20..=32).contains(&aside.width));
        assert!(large.object.width >= 40, "{large:?}");
        let wide = at(160, 48, false);
        assert!(wide.conversation.width <= 56 && wide.object.width > wide.conversation.width);
    }

    #[test]
    fn a_terminal_below_the_minimum_keeps_the_inline_presentation() {
        assert_eq!(Geometry::of(Rect::new(0, 0, 59, 40), false), None);
        assert_eq!(Geometry::of(Rect::new(0, 0, 200, 15), false), None);
        assert!(Geometry::of(Rect::new(0, 0, MIN_SIZE.0, MIN_SIZE.1), true).is_some());
    }

    #[test]
    fn an_offset_area_keeps_its_origin() {
        let geometry = Geometry::of(Rect::new(3, 2, 120, 40), true).expect("fits");
        assert_eq!((geometry.header.x, geometry.header.y), (3, 2));
        let pinned = geometry.pinned.expect("pinned row");
        assert_eq!(pinned.bottom(), 42);
    }
}
