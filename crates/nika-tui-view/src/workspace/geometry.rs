// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where each region of one workspace stands, restored or with its selected
//! object expanded. The v1 layout names remain source-compatible internals.
//!
//! The restored arrangement (v1 `Session`) keeps the conversation comfortable: at 80×24 the
//! aside is folded and the conversation sits below the object; from 100
//! columns the conversation stands beside the object; from 120 columns the
//! project aside appears on the left. Object expansion (v1 `Workbench`) gives it room:
//! from 100 columns it takes every column but the narrowest conversation,
//! which stays beside it with every row; on a narrower terminal it stands
//! above a compact conversation whose composer stays in view. The aside
//! keeps its column either way. Every region is a whole rectangle,
//! no two overlap, and together they cover the screen exactly, so a resize or
//! expansion never leaves a stale cell or cuts a region in the middle of
//! a row.
//!
//! The shares the human chose ([`Arrangement`]) are kept as chosen: each
//! frame applies them within bounds (the composer and the object keep their
//! minimum), so a terminal that shrinks and grows back shows the same widths
//! again. Nothing here reads a file, a clock or the environment; keeping an
//! arrangement across launches is the host's work.

use ratatui::layout::{Position, Rect};

/// The narrowest terminal that shows the object and the conversation side by side.
pub const SIDE_BY_SIDE_MIN_WIDTH: u16 = 100;
/// The narrowest terminal that also shows the project aside.
pub const ASIDE_MIN_WIDTH: u16 = 120;
/// The shortest terminal that gives the header its second row (the location).
pub const TALL_HEADER_MIN_HEIGHT: u16 = 30;
/// Below this size the workspace does not fit: `Ctrl+T` opens the focus view
/// instead, and a workspace resized below it draws the focus view until the
/// size allows it again.
pub const MIN_SIZE: (u16, u16) = (60, 16);

/// The narrowest conversation a separator leaves beside the object.
pub const CONVERSATION_MIN_WIDTH: u16 = 40;
/// The narrowest object a separator leaves beside the conversation.
pub const OBJECT_MIN_WIDTH: u16 = 30;
/// The fewest rows the conversation keeps under the expanded object: its
/// title, a transcript row, its context row and the live area (status,
/// composer, hint), so the composer never leaves the screen.
pub const CONVERSATION_MIN_ROWS: u16 = 8;
/// The fewest rows the expanded object keeps above the conversation: its
/// title and five lines.
pub const OBJECT_MIN_ROWS: u16 = 6;
/// The narrowest project aside a separator leaves.
pub const ASIDE_MIN: u16 = 16;
/// The widest project aside a separator leaves (a third of the screen at most).
pub const ASIDE_MAX: u16 = 64;
/// The unit of a share: thousandths.
const WHOLE: u16 = 1000;

/// Whether a terminal of `size` (columns, rows) holds the workspace.
#[must_use]
pub const fn fits(size: (u16, u16)) -> bool {
    size.0 >= MIN_SIZE.0 && size.1 >= MIN_SIZE.1
}

/// The v1 names for the restored workspace and its expanded object. Both keep
/// the same conversation, draft, selection and runs; changing one to the other
/// changes presentation only. These names are not user-facing destinations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Layout {
    /// The project on the left, a comfortable conversation in the middle,
    /// the object on the right (the default).
    Session,
    /// The object larger: beside the narrowest conversation from
    /// [`SIDE_BY_SIDE_MIN_WIDTH`] columns, above a compact conversation and
    /// its composer below.
    Workbench,
}

impl Layout {
    /// Both v1 arrangements, restored first.
    pub const ALL: [Self; 2] = [Self::Session, Self::Workbench];

    /// The legacy v1 name, retained for source compatibility. The workspace
    /// exposes a contextual Expand/Restore action instead of these names.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Workbench => "Workbench",
        }
    }

    /// The other layout (what `F4` shows).
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Session => Self::Workbench,
            Self::Workbench => Self::Session,
        }
    }
}

/// What the human chose for the workspace's presentation: the layout and the
/// shares the separators were moved to, in thousandths (at most 1000); `None`
/// keeps the automatic share. A share is kept as chosen and applied within
/// bounds at each frame's size, so no resize rewrites it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Arrangement {
    /// The layout in view.
    pub layout: Layout,
    /// The project aside's width, in thousandths of the screen's width. Both
    /// layouts share it: switching never moves the project column.
    pub aside_width: Option<u16>,
    /// Session: the conversation's width beside the object, in thousandths
    /// of the width right of the aside (beside the expanded object the
    /// conversation is at its narrowest).
    pub conversation_width: Option<u16>,
    /// Workbench below [`SIDE_BY_SIDE_MIN_WIDTH`] columns: the conversation's
    /// height under the object, in thousandths of the height between the
    /// header and the pinned row.
    pub conversation_height: Option<u16>,
}

impl Arrangement {
    /// `layout`, every share automatic. (No `Default`: the start is named,
    /// like every other state of the screen.)
    #[must_use]
    pub const fn of(layout: Layout) -> Self {
        Self {
            layout,
            aside_width: None,
            conversation_width: None,
            conversation_height: None,
        }
    }

    /// This arrangement in `layout`, every share kept.
    #[must_use]
    pub const fn with_layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }

    /// This arrangement with the aside's share (`None`: automatic).
    #[must_use]
    pub fn with_aside_width(mut self, share: Option<u16>) -> Self {
        self.aside_width = share.map(|s| s.min(WHOLE));
        self
    }

    /// This arrangement with the Session conversation's share (`None`: automatic).
    #[must_use]
    pub fn with_conversation_width(mut self, share: Option<u16>) -> Self {
        self.conversation_width = share.map(|s| s.min(WHOLE));
        self
    }

    /// This arrangement with the Workbench conversation's share (`None`: automatic).
    #[must_use]
    pub fn with_conversation_height(mut self, share: Option<u16>) -> Self {
        self.conversation_height = share.map(|s| s.min(WHOLE));
        self
    }

    /// This arrangement with `separator` leaving `cells` to the region it
    /// sizes on a frame of `area` (the aside's columns, the conversation's
    /// columns beside the object or its rows under it), within the bounds at
    /// that size. Unchanged where that frame shows no such separator. Beside
    /// the expanded object the conversation is at its narrowest: leaving it
    /// more is manual sizing, which restores the object with the conversation
    /// at that width; leaving it no more keeps the expansion.
    #[must_use]
    pub fn moved(self, separator: Separator, cells: u16, area: Rect, pinned: bool) -> Self {
        let Some(geometry) = Geometry::arranged(area, pinned, &self) else {
            return self;
        };
        if !geometry.shows(separator) {
            return self;
        }
        let (low, high, total) = geometry.bounds(separator);
        let cells = cells.clamp(low, high);
        let share = Some(thousandths(cells, total));
        match separator {
            Separator::Aside => self.with_aside_width(share),
            Separator::Beside if self.layout == Layout::Workbench => {
                if cells == geometry.extent_of(separator) {
                    self
                } else {
                    self.with_layout(Layout::Session)
                        .with_conversation_width(share)
                }
            }
            Separator::Beside => self.with_conversation_width(share),
            Separator::Beneath => self.with_conversation_height(share),
        }
    }

    /// This arrangement with `separator` back to its automatic share.
    #[must_use]
    pub fn restored(self, separator: Separator) -> Self {
        match separator {
            Separator::Aside => self.with_aside_width(None),
            Separator::Beside => self.with_conversation_width(None),
            Separator::Beneath => self.with_conversation_height(None),
        }
    }
}

/// A boundary between two regions that the pointer or the keys can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Separator {
    /// The project aside's right edge (both layouts, from 120 columns).
    Aside,
    /// The edge between the conversation and the object beside it (both
    /// layouts, from 100 columns).
    Beside,
    /// The conversation's title rule under the expanded object (below 100
    /// columns).
    Beneath,
}

impl Separator {
    /// Every separator, in reading order.
    pub const ALL: [Self; 3] = [Self::Aside, Self::Beside, Self::Beneath];
}

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
    /// The layout these regions arrange.
    layout: Layout,
    /// What the object and the conversation share: the rows between the
    /// header and the pinned row, right of the aside.
    work: Rect,
}

impl Geometry {
    /// The regions of a frame of `area` in the Session layout with automatic
    /// shares, with one pinned activity row when `pinned`; none when the
    /// terminal is smaller than [`MIN_SIZE`].
    #[must_use]
    pub fn of(area: Rect, pinned: bool) -> Option<Self> {
        Self::arranged(area, pinned, &Arrangement::of(Layout::Session))
    }

    /// The regions of a frame of `area` as `arrangement` lays them out, with
    /// one pinned activity row when `pinned`; none when the terminal is
    /// smaller than [`MIN_SIZE`]. Each share applies within bounds: the
    /// conversation keeps 40 columns beside an object of 30 (exactly 40
    /// beside the expanded object), 8 rows under an expanded object of 6.
    #[must_use]
    pub fn arranged(area: Rect, pinned: bool, arrangement: &Arrangement) -> Option<Self> {
        if !fits((area.width, area.height)) {
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
        let aside_width = aside_width(area.width, arrangement.aside_width);
        let aside = aside_width.map(|width| Rect::new(body.x, body.y, width, body.height));
        let shift = aside_width.unwrap_or(0);
        let work = Rect::new(body.x + shift, body.y, body.width - shift, body.height);
        let layout = arrangement.layout;
        let stacked = area.width < SIDE_BY_SIDE_MIN_WIDTH;
        let (object, conversation) = match layout {
            Layout::Workbench if stacked => {
                let talk = conversation_rows(work.height, arrangement.conversation_height);
                split_rows(work, talk)
            }
            // Every column but the narrowest conversation, which keeps its rows.
            Layout::Workbench => split_columns(work, beside_bounds(work.width).0),
            // The conversation keeps at least half the rows: the composer is never squeezed out.
            Layout::Session if stacked => split_rows(work, work.height.div_ceil(2)),
            Layout::Session => {
                let talk = conversation_width(work.width, arrangement.conversation_width);
                split_columns(work, talk)
            }
        };
        Some(Self {
            header,
            aside,
            object,
            conversation,
            pinned,
            stacked,
            layout,
            work,
        })
    }

    /// Every region shown, in reading order.
    #[must_use]
    pub fn regions(&self) -> Vec<Rect> {
        let mut regions = vec![self.header];
        regions.extend(self.aside);
        if self.stacked {
            regions.push(self.object);
            regions.push(self.conversation);
        } else {
            regions.push(self.conversation);
            regions.push(self.object);
        }
        regions.extend(self.pinned);
        regions
    }

    /// The cells that take the pointer for `separator`, when this frame
    /// shows it: the aside's edge column, the conversation's gutter and rule
    /// beside the object, or its title rule under the object.
    #[must_use]
    pub fn handle(&self, separator: Separator) -> Option<Rect> {
        let talk = self.conversation;
        match separator {
            Separator::Aside => self
                .aside
                .map(|aside| Rect::new(aside.right() - 1, aside.y, 1, aside.height)),
            Separator::Beside => {
                (!self.stacked).then(|| Rect::new(talk.right() - 2, talk.y, 2, talk.height))
            }
            Separator::Beneath => (self.stacked && self.layout == Layout::Workbench)
                .then(|| Rect::new(talk.x, talk.y, talk.width, 1)),
        }
    }

    /// Whether this frame shows `separator`.
    #[must_use]
    pub fn shows(&self, separator: Separator) -> bool {
        self.handle(separator).is_some()
    }

    /// The separator whose handle holds `point`.
    #[must_use]
    pub fn separator_at(&self, point: Position) -> Option<Separator> {
        Separator::ALL
            .into_iter()
            .find(|s| self.handle(*s).is_some_and(|cells| cells.contains(point)))
    }

    /// The cells the region `separator` sizes holds now: the aside's columns,
    /// the conversation's columns beside the object, or its rows under it.
    #[must_use]
    pub fn extent_of(&self, separator: Separator) -> u16 {
        match separator {
            Separator::Aside => self.aside.map_or(0, |aside| aside.width),
            Separator::Beside => self.conversation.width,
            Separator::Beneath => self.conversation.height,
        }
    }

    /// The fewest and the most cells `separator` may leave to its region on
    /// this frame, and the total its share is a part of.
    fn bounds(&self, separator: Separator) -> (u16, u16, u16) {
        let width = self.header.width;
        match separator {
            Separator::Aside => {
                let (low, high) = aside_bounds(width);
                (low, high, width)
            }
            Separator::Beside => {
                let (low, high) = beside_bounds(self.work.width);
                (low, high, self.work.width)
            }
            Separator::Beneath => {
                let (low, high) = beneath_bounds(self.work.height);
                (low, high, self.work.height)
            }
        }
    }
}

/// `work` cut in two: the object above, the conversation's `talk` rows below.
fn split_rows(work: Rect, talk: u16) -> (Rect, Rect) {
    let seen = work.height - talk;
    (
        Rect::new(work.x, work.y, work.width, seen),
        Rect::new(work.x, work.y + seen, work.width, talk),
    )
}

/// `work` cut in two: the conversation's `talk` columns on the left, the
/// object right of them.
fn split_columns(work: Rect, talk: u16) -> (Rect, Rect) {
    let seen = work.width - talk;
    (
        Rect::new(work.x + talk, work.y, seen, work.height),
        Rect::new(work.x, work.y, talk, work.height),
    )
}

/// The aside's width on a screen `width` wide: none below [`ASIDE_MIN_WIDTH`],
/// the chosen share within its bounds, else the automatic width.
fn aside_width(width: u16, chosen: Option<u16>) -> Option<u16> {
    if width < ASIDE_MIN_WIDTH {
        return None;
    }
    let (low, high) = aside_bounds(width);
    Some(chosen.map_or_else(
        || clamp_share(width, 18, 20, 32),
        |share| cells(share, width).clamp(low, high),
    ))
}

/// The conversation's width beside the object in a work area `width` wide.
fn conversation_width(width: u16, chosen: Option<u16>) -> u16 {
    let (low, high) = beside_bounds(width);
    chosen.map_or_else(
        || clamp_share(width, 48, 42, 92),
        |share| cells(share, width).clamp(low, high),
    )
}

/// The Workbench conversation's rows under the object in a work area
/// `height` tall.
fn conversation_rows(height: u16, chosen: Option<u16>) -> u16 {
    let (low, high) = beneath_bounds(height);
    chosen.map_or_else(
        || clamp_share(height, 35, low, high),
        |share| cells(share, height).clamp(low, high),
    )
}

/// The narrowest and the widest aside on a screen `width` wide: what remains
/// keeps room for a conversation beside an object.
fn aside_bounds(width: u16) -> (u16, u16) {
    let room = width.saturating_sub(CONVERSATION_MIN_WIDTH + OBJECT_MIN_WIDTH);
    let high = (width / 3).min(ASIDE_MAX).min(room);
    (ASIDE_MIN.min(high), high)
}

/// The narrowest and the widest conversation beside an object in a work area
/// `width` wide.
fn beside_bounds(width: u16) -> (u16, u16) {
    let high = width.saturating_sub(OBJECT_MIN_WIDTH);
    (CONVERSATION_MIN_WIDTH.min(high), high)
}

/// The fewest and the most rows of a Workbench conversation under the object
/// in a work area `height` tall.
fn beneath_bounds(height: u16) -> (u16, u16) {
    let high = height.saturating_sub(OBJECT_MIN_ROWS);
    (CONVERSATION_MIN_ROWS.min(high), high)
}

/// `share` thousandths of `total` cells, rounded to the nearest cell.
fn cells(share: u16, total: u16) -> u16 {
    let exact = (u32::from(share.min(WHOLE)) * u32::from(total) + 500) / 1000;
    u16::try_from(exact).unwrap_or(total)
}

/// `part` of `total` cells in thousandths, rounded; nothing of nothing.
fn thousandths(part: u16, total: u16) -> u16 {
    if total == 0 {
        return 0;
    }
    let exact = (u32::from(part.min(total)) * 1000 + u32::from(total) / 2) / u32::from(total);
    u16::try_from(exact).unwrap_or(WHOLE)
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
    /// Every size the layouts are proved at: the minimum, the matrix, and wider.
    const ALL_SIZES: [(u16, u16); 9] = [
        (60, 16),
        (80, 24),
        (99, 30),
        (100, 32),
        (119, 33),
        (120, 40),
        (160, 48),
        (180, 48),
        (240, 60),
    ];
    /// Origins a caller may draw the workspace at.
    const ORIGINS: [(u16, u16); 3] = [(0, 0), (3, 2), (17, 9)];

    fn at(width: u16, height: u16, pinned: bool) -> Geometry {
        Geometry::of(Rect::new(0, 0, width, height), pinned).expect("fits")
    }

    fn cells_of(rect: Rect) -> u32 {
        u32::from(rect.width) * u32::from(rect.height)
    }

    /// Automatic, the narrowest and the widest chosen shares, and one between.
    fn arrangements() -> Vec<Arrangement> {
        let mut all = Vec::new();
        for layout in Layout::ALL {
            for share in [None, Some(0), Some(1000), Some(431)] {
                all.push(
                    Arrangement::of(layout)
                        .with_aside_width(share)
                        .with_conversation_width(share)
                        .with_conversation_height(share),
                );
            }
        }
        all
    }

    /// Every region inside `area`, no two meeting, all of them covering it.
    fn assert_tiles(geometry: &Geometry, area: Rect, what: &str) {
        let regions = geometry.regions();
        let covered: u32 = regions.iter().copied().map(cells_of).sum();
        assert_eq!(covered, cells_of(area), "{what}: {geometry:?}");
        for (i, a) in regions.iter().enumerate() {
            assert!(!a.is_empty(), "{what}: an empty region {a:?}");
            assert_eq!(a.intersection(area), *a, "{what}: {a:?} leaves the screen");
            for b in regions.iter().skip(i + 1) {
                assert!(!a.intersects(*b), "{what}: {a:?} meets {b:?}");
            }
        }
    }

    #[test]
    fn the_regions_cover_the_screen_exactly_without_overlap() {
        for (width, height) in SIZES {
            for pinned in [false, true] {
                let geometry = at(width, height, pinned);
                let area = Rect::new(0, 0, width, height);
                assert_tiles(
                    &geometry,
                    area,
                    &format!("{width}x{height} pinned={pinned}"),
                );
            }
        }
    }

    /// Both layouts, at every size, from every origin, with any share: whole
    /// regions inside the area, none meeting another, covering it exactly;
    /// the composer's region and the object keep their minimum.
    #[test]
    fn every_arrangement_tiles_every_size_at_every_origin() {
        for (width, height) in ALL_SIZES {
            for (x, y) in ORIGINS {
                for pinned in [false, true] {
                    for arrangement in arrangements() {
                        let area = Rect::new(x, y, width, height);
                        let what =
                            format!("{width}x{height}@{x},{y} pinned={pinned} {arrangement:?}");
                        let g = Geometry::arranged(area, pinned, &arrangement).expect("fits");
                        assert_tiles(&g, area, &what);
                        assert_eq!((g.header.x, g.header.y), (x, y), "{what}");
                        assert_eq!(g.aside.is_some(), width >= ASIDE_MIN_WIDTH, "{what}");
                        if let Some(row) = g.pinned {
                            assert_eq!(row.bottom(), area.bottom(), "{what}");
                        }
                        // Restored or expanded, the conversation stands beside the
                        // object from 100 columns and under it below.
                        let side_by_side = width >= SIDE_BY_SIDE_MIN_WIDTH;
                        assert_eq!(g.stacked, !side_by_side, "{what}");
                        if g.stacked {
                            assert_eq!(g.conversation.x, g.object.x, "{what}");
                            assert_eq!(g.object.bottom(), g.conversation.y, "{what}");
                        } else {
                            assert!(g.conversation.width >= CONVERSATION_MIN_WIDTH, "{what}");
                            assert!(g.object.width >= OBJECT_MIN_WIDTH, "{what}");
                            assert_eq!(g.conversation.right(), g.object.x, "{what}");
                            assert_eq!(g.conversation.height, g.object.height, "{what}");
                        }
                        if arrangement.layout == Layout::Workbench && g.stacked {
                            assert!(g.conversation.height >= CONVERSATION_MIN_ROWS, "{what}");
                            assert!(g.object.height >= OBJECT_MIN_ROWS, "{what}");
                        }
                        if arrangement.layout == Layout::Workbench && !g.stacked {
                            // Beside the expanded object: the narrowest conversation.
                            assert_eq!(g.conversation.width, CONVERSATION_MIN_WIDTH, "{what}");
                        }
                        if let Some(aside) = g.aside {
                            assert!((ASIDE_MIN..=ASIDE_MAX).contains(&aside.width), "{what}");
                            assert!(aside.width <= width / 3, "{what}");
                        }
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
        assert_eq!(medium.conversation.width, 48);
        let large = at(120, 40, false);
        let aside = large.aside.expect("aside at 120 columns");
        assert!((20..=32).contains(&aside.width));
        assert_eq!(
            (aside.width, large.conversation.width, large.object.width),
            (21, 47, 52)
        );
        assert_eq!(aside.right(), large.conversation.x);
        assert_eq!(large.conversation.right(), large.object.x);
        assert_eq!(large.object.right(), 120);
        let wide = at(160, 48, false);
        assert_eq!((wide.conversation.width, wide.object.width), (63, 69));
        assert_eq!(wide.conversation.right(), wide.object.x);
        let roomy = at(190, 48, false);
        assert_eq!((roomy.conversation.width, roomy.object.width), (75, 83));
        let huge = at(300, 48, false);
        assert_eq!(huge.conversation.width, 92);
        let regions = large.regions();
        assert_eq!(regions[2], large.conversation);
        assert_eq!(regions[3], large.object);
    }

    #[test]
    fn a_terminal_below_the_minimum_holds_no_workspace() {
        assert_eq!(Geometry::of(Rect::new(0, 0, 59, 40), false), None);
        assert_eq!(Geometry::of(Rect::new(0, 0, 200, 15), false), None);
        assert!(Geometry::of(Rect::new(0, 0, MIN_SIZE.0, MIN_SIZE.1), true).is_some());
        assert!(fits(MIN_SIZE) && fits((80, 24)));
        assert!(!fits((59, 40)) && !fits((200, 15)));
        let workbench = Arrangement::of(Layout::Workbench);
        assert_eq!(
            Geometry::arranged(Rect::new(0, 0, 59, 40), false, &workbench),
            None
        );
    }

    #[test]
    fn an_offset_area_keeps_its_origin() {
        let geometry = Geometry::of(Rect::new(3, 2, 120, 40), true).expect("fits");
        assert_eq!((geometry.header.x, geometry.header.y), (3, 2));
        let pinned = geometry.pinned.expect("pinned row");
        assert_eq!(pinned.bottom(), 42);
    }

    /// `Geometry::of` is the Session layout with automatic shares, exactly.
    #[test]
    fn the_session_layout_is_the_default_geometry() {
        for (width, height) in ALL_SIZES {
            for pinned in [false, true] {
                let area = Rect::new(0, 0, width, height);
                let session = Arrangement::of(Layout::Session);
                assert_eq!(
                    Geometry::of(area, pinned),
                    Geometry::arranged(area, pinned, &session)
                );
                assert_eq!(
                    Geometry::of(area, pinned).map(|g| g.layout),
                    Some(Layout::Session)
                );
            }
        }
    }

    /// The expanded object at the three target sizes: above a compact
    /// conversation at 80 columns; from 100 columns wider, the conversation
    /// and its composer still beside it at their narrowest, every row kept;
    /// the project aside on the left from 120 columns, as wide as restored.
    #[test]
    fn the_expanded_object_takes_the_room_at_the_target_sizes() {
        let workbench = Arrangement::of(Layout::Workbench);
        let arranged = |width, height| {
            Geometry::arranged(Rect::new(0, 0, width, height), true, &workbench).expect("fits")
        };
        let small = arranged(80, 24);
        assert_eq!(small.aside, None);
        assert_eq!(small.object, Rect::new(0, 1, 80, 14));
        assert_eq!(small.conversation, Rect::new(0, 15, 80, 8));
        let large = arranged(120, 40);
        assert_eq!(large.aside, Some(Rect::new(0, 2, 21, 37)));
        assert_eq!(large.conversation, Rect::new(21, 2, 40, 37));
        assert_eq!(large.object, Rect::new(61, 2, 59, 37));
        let wide = arranged(180, 48);
        assert_eq!(wide.aside, Some(Rect::new(0, 2, 32, 45)));
        assert_eq!(wide.conversation, Rect::new(32, 2, 40, 45));
        assert_eq!(wide.object, Rect::new(72, 2, 108, 45));
        for (width, height) in [(80, 24), (120, 40), (180, 48)] {
            let session = at(width, height, true);
            let workbench = arranged(width, height);
            assert!(
                cells_of(workbench.object) > cells_of(session.object),
                "{width}x{height}: the expanded object is larger"
            );
            assert_eq!(
                workbench.aside, session.aside,
                "the project column never moves"
            );
            if width < SIDE_BY_SIDE_MIN_WIDTH {
                assert!(workbench.conversation.height < session.conversation.height);
            } else {
                assert!(
                    !workbench.stacked,
                    "{width}x{height}: never under the object"
                );
                assert!(workbench.conversation.width < session.conversation.width);
            }
        }
    }

    /// From 100 columns, expanding the object widens it while the
    /// conversation stays beside it, on the same rows and from the same
    /// column, at its narrowest; the object keeps its rows and its right
    /// edge. Below 100 columns the expanded object stands above the
    /// conversation, which keeps its composer's rows.
    #[test]
    fn the_expanded_object_widens_beside_the_conversation_from_100_columns() {
        let workbench = Arrangement::of(Layout::Workbench);
        let wide = [
            (100, 16),
            (100, 32),
            (119, 33),
            (120, 16),
            (120, 40),
            (160, 48),
            (180, 17),
            (180, 48),
            (240, 60),
        ];
        for (width, height) in wide {
            for pinned in [false, true] {
                let what = format!("{width}x{height} pinned={pinned}");
                let area = Rect::new(0, 0, width, height);
                let restored = at(width, height, pinned);
                let g = Geometry::arranged(area, pinned, &workbench).expect("fits");
                assert!(
                    !g.stacked,
                    "{what}: the conversation moved under the object"
                );
                assert_eq!(g.aside, restored.aside, "{what}");
                assert_eq!(g.conversation.x, restored.conversation.x, "{what}");
                assert_eq!(g.conversation.y, restored.conversation.y, "{what}");
                assert_eq!(
                    g.conversation.height, restored.conversation.height,
                    "{what}"
                );
                assert_eq!(g.conversation.width, CONVERSATION_MIN_WIDTH, "{what}");
                assert!(g.conversation.width < restored.conversation.width, "{what}");
                assert_eq!(g.conversation.right(), g.object.x, "{what}");
                assert_eq!(g.object.right(), restored.object.right(), "{what}");
                assert_eq!(
                    (g.object.y, g.object.height),
                    (restored.object.y, restored.object.height),
                    "{what}"
                );
                assert!(g.object.width > restored.object.width, "{what}");
            }
        }
        for (width, height) in [(60, 18), (80, 24), (99, 30)] {
            for pinned in [false, true] {
                let what = format!("{width}x{height} pinned={pinned}");
                let area = Rect::new(0, 0, width, height);
                let restored = at(width, height, pinned);
                let g = Geometry::arranged(area, pinned, &workbench).expect("fits");
                assert!(g.stacked && restored.stacked, "{what}");
                assert_eq!(g.object.bottom(), g.conversation.y, "{what}");
                assert!(g.conversation.height >= CONVERSATION_MIN_ROWS, "{what}");
                assert!(
                    g.conversation.height <= restored.conversation.height,
                    "{what}"
                );
            }
        }
    }

    /// A moved separator is kept as a share: the same size shows the exact
    /// cells it was moved to, another size its proportion within bounds, and
    /// the size it came back to the same cells again.
    #[test]
    fn a_moved_separator_returns_exactly_after_a_resize_and_a_switch() {
        let area = Rect::new(0, 0, 180, 48);
        let mut arrangement = Arrangement::of(Layout::Session)
            .moved(Separator::Aside, 27, area, false)
            .moved(Separator::Beside, 71, area, false);
        let wide = Geometry::arranged(area, false, &arrangement).expect("fits");
        assert_eq!(wide.extent_of(Separator::Aside), 27);
        assert_eq!(wide.extent_of(Separator::Beside), 71);
        let small = Rect::new(0, 0, 120, 40);
        let shrunk = Geometry::arranged(small, false, &arrangement).expect("fits");
        assert_eq!(shrunk.extent_of(Separator::Aside), 18, "27/180 of 120");
        assert_eq!(shrunk.extent_of(Separator::Beside), 47, "71/153 of 102");
        arrangement = arrangement.with_layout(Layout::Workbench);
        // The rows under the expanded object move where it stands above them.
        let narrow = Rect::new(0, 0, 80, 40);
        arrangement = arrangement.moved(Separator::Beneath, 20, narrow, false);
        let under = Geometry::arranged(narrow, false, &arrangement).expect("fits");
        assert_eq!(under.extent_of(Separator::Beneath), 20);
        let bench = Geometry::arranged(area, false, &arrangement).expect("fits");
        assert_eq!(bench.extent_of(Separator::Aside), 27, "the aside is shared");
        assert_eq!(
            bench.extent_of(Separator::Beside),
            CONVERSATION_MIN_WIDTH,
            "beside the expanded object the conversation is at its narrowest"
        );
        arrangement = arrangement.with_layout(Layout::Session);
        let back = Geometry::arranged(area, false, &arrangement).expect("fits");
        assert_eq!(back, wide, "Session returns exactly as it was left");
    }

    /// Every separator stops at its bounds, whatever the cells asked.
    #[test]
    fn separators_stop_at_their_bounds() {
        let area = Rect::new(0, 0, 120, 40);
        let session = Arrangement::of(Layout::Session);
        for (separator, asked, kept) in [
            (Separator::Aside, 0, ASIDE_MIN),
            (Separator::Aside, 500, 40),
            (Separator::Beside, 0, CONVERSATION_MIN_WIDTH),
            (Separator::Beside, 500, 99 - OBJECT_MIN_WIDTH),
        ] {
            let moved = session.moved(separator, asked, area, true);
            let g = Geometry::arranged(area, true, &moved).expect("fits");
            assert_eq!(g.extent_of(separator), kept, "{separator:?} asked {asked}");
        }
        // The rows under the expanded object: below 100 columns, where it
        // stands above the conversation.
        let narrow = Rect::new(0, 0, 99, 40);
        let bench = Arrangement::of(Layout::Workbench);
        for (asked, kept) in [(0, CONVERSATION_MIN_ROWS), (500, 37 - OBJECT_MIN_ROWS)] {
            let moved = bench.moved(Separator::Beneath, asked, narrow, true);
            let g = Geometry::arranged(narrow, true, &moved).expect("fits");
            assert_eq!(g.extent_of(Separator::Beneath), kept, "asked {asked}");
        }
    }

    /// A separator the frame does not show is not moved, and a restored one
    /// is automatic again.
    #[test]
    fn an_unshown_separator_stays_and_a_restored_one_is_automatic() {
        let small = Rect::new(0, 0, 80, 24);
        let session = Arrangement::of(Layout::Session);
        for separator in Separator::ALL {
            assert_eq!(
                session.moved(separator, 30, small, false),
                session,
                "{separator:?}"
            );
        }
        let bench = Arrangement::of(Layout::Workbench);
        // Beside the expanded object no rule stands under it.
        assert_eq!(
            bench.moved(Separator::Beneath, 12, Rect::new(0, 0, 180, 48), false),
            bench
        );
        let chosen = bench.moved(Separator::Beneath, 12, small, false);
        assert_ne!(chosen, bench);
        assert_eq!(chosen.restored(Separator::Beneath), bench);
    }

    /// Beside the expanded object the conversation is at its narrowest and
    /// its separator stays operable. Asking it for the narrowest again, or
    /// less, moves nothing and keeps the object expanded; asking for a wider
    /// conversation is manual sizing: the object is restored with the
    /// conversation at the cells asked, every other share kept. Expanded
    /// again, it restores to that width.
    #[test]
    fn the_separator_beside_the_expanded_object_restores_it_at_the_width_asked() {
        let area = Rect::new(0, 0, 180, 48);
        let expanded = Arrangement::of(Layout::Session)
            .moved(Separator::Aside, 27, area, false)
            .with_conversation_height(Some(300))
            .with_layout(Layout::Workbench);
        let wide = Geometry::arranged(area, false, &expanded).expect("fits");
        let talk = wide.conversation;
        assert_eq!(
            wide.handle(Separator::Beside),
            Some(Rect::new(talk.right() - 2, talk.y, 2, talk.height)),
            "an operable separator beside the expanded object"
        );
        for asked in [0, CONVERSATION_MIN_WIDTH - 1, CONVERSATION_MIN_WIDTH] {
            assert_eq!(
                expanded.moved(Separator::Beside, asked, area, false),
                expanded,
                "asked {asked}"
            );
        }
        let moved = expanded.moved(Separator::Beside, 46, area, false);
        assert_eq!(moved.layout, Layout::Session, "manual sizing restores");
        assert_eq!(
            (moved.aside_width, moved.conversation_height),
            (expanded.aside_width, expanded.conversation_height)
        );
        let restored = Geometry::arranged(area, false, &moved).expect("fits");
        assert_eq!(restored.extent_of(Separator::Beside), 46);
        assert_eq!(restored.extent_of(Separator::Aside), 27);
        let again = moved.with_layout(Layout::Workbench);
        let beside = Geometry::arranged(area, false, &again).expect("fits");
        assert_eq!(beside.extent_of(Separator::Beside), CONVERSATION_MIN_WIDTH);
        assert_eq!(
            Geometry::arranged(area, false, &again.with_layout(Layout::Session)),
            Some(restored)
        );
    }

    /// The handles: the aside's edge column, the conversation's gutter and
    /// rule beside the object, its title row under the object; none in a
    /// narrow Session, where the split is automatic.
    #[test]
    fn handles_sit_on_the_painted_separators() {
        let session = at(120, 40, true);
        let aside = session.aside.expect("aside");
        assert_eq!(
            session.handle(Separator::Aside),
            Some(Rect::new(20, 2, 1, 37))
        );
        assert_eq!(
            session.handle(Separator::Beside),
            Some(Rect::new(66, 2, 2, 37))
        );
        assert_eq!(session.handle(Separator::Beneath), None);
        assert_eq!(
            session.separator_at(Position::new(aside.right() - 1, 9)),
            Some(Separator::Aside)
        );
        assert_eq!(
            session.separator_at(Position::new(67, 30)),
            Some(Separator::Beside)
        );
        assert_eq!(session.separator_at(Position::new(65, 30)), None);
        let expanded = Arrangement::of(Layout::Workbench);
        let bench = Geometry::arranged(Rect::new(0, 0, 120, 40), true, &expanded).expect("fits");
        // Beside the expanded object: the aside's edge, and the narrowest
        // conversation's gutter and rule; no rule under the object.
        assert_eq!(
            bench.handle(Separator::Aside),
            session.handle(Separator::Aside)
        );
        assert_eq!(
            bench.handle(Separator::Beside),
            Some(Rect::new(59, 2, 2, 37))
        );
        assert_eq!(bench.handle(Separator::Beneath), None);
        assert_eq!(
            bench.separator_at(Position::new(60, 27)),
            Some(Separator::Beside)
        );
        let under = Geometry::arranged(Rect::new(0, 0, 99, 40), true, &expanded).expect("fits");
        assert_eq!(under.handle(Separator::Beside), None);
        assert_eq!(
            under.handle(Separator::Beneath),
            Some(Rect::new(0, 27, 99, 1))
        );
        assert_eq!(
            under.separator_at(Position::new(60, 27)),
            Some(Separator::Beneath)
        );
        let narrow = at(80, 24, false);
        assert!(Separator::ALL.iter().all(|s| !narrow.shows(*s)));
    }

    /// A share and its cells convert back exactly below 1000 cells.
    #[test]
    fn shares_round_trip_to_the_same_cells() {
        for total in 1..=400 {
            for part in 0..=total {
                assert_eq!(
                    cells(thousandths(part, total), total),
                    part,
                    "{part}/{total}"
                );
            }
        }
        assert_eq!(thousandths(5, 0), 0);
        assert_eq!(cells(WHOLE, 120), 120);
        assert_eq!(
            cells(u16::MAX, 120),
            120,
            "a share past the whole is the whole"
        );
        let wild = Arrangement::of(Layout::Session).with_aside_width(Some(9_999));
        assert_eq!(wild.aside_width, Some(WHOLE));
    }

    #[test]
    fn the_layouts_name_themselves_and_toggle() {
        assert_eq!(Layout::Session.label(), "Session");
        assert_eq!(Layout::Workbench.label(), "Workbench");
        for layout in Layout::ALL {
            assert_ne!(layout.toggled(), layout);
            assert_eq!(layout.toggled().toggled(), layout);
        }
    }
}
