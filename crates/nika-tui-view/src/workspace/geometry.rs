// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Where each region of the workspace screen stands at a given size, in one
//! of the two layouts of the same desk.
//!
//! **Session** (the default) keeps the conversation comfortable: at 80×24 the
//! aside is folded and the conversation sits below the object; from 100
//! columns the conversation stands beside the object; from 120 columns the
//! project aside appears on the left. **Workbench** gives the object the room:
//! it stands above a compact conversation whose composer stays in view, with
//! the aside on the left from 120 columns. Every region is a whole rectangle,
//! no two overlap, and together they cover the screen exactly, so a resize or
//! a layout switch never leaves a stale cell or cuts a region in the middle of
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
/// The fewest rows the Workbench conversation keeps under the object: its
/// title, a transcript row, its context row and the live area (status,
/// composer, hint), so the composer never leaves the screen.
pub const CONVERSATION_MIN_ROWS: u16 = 8;
/// The fewest rows the Workbench object keeps: its title and five lines.
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

/// The two layouts of the same desk: the same conversation, object, draft,
/// selection and runs, arranged for talking or for working on the object.
/// Switching between them is a view change only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Layout {
    /// The project on the left, a comfortable conversation in the middle,
    /// the object on the right (the default).
    Session,
    /// The object larger, above a compact conversation and its composer.
    Workbench,
}

impl Layout {
    /// Both layouts, in the order the switch names them.
    pub const ALL: [Self; 2] = [Self::Session, Self::Workbench];

    /// The layout's name on screen.
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
    /// of the width right of the aside.
    pub conversation_width: Option<u16>,
    /// Workbench: the conversation's height under the object, in thousandths
    /// of the height between the header and the pinned row.
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
    /// that size. Unchanged where that frame shows no such separator.
    #[must_use]
    pub fn moved(self, separator: Separator, cells: u16, area: Rect, pinned: bool) -> Self {
        let Some(geometry) = Geometry::arranged(area, pinned, &self) else {
            return self;
        };
        if !geometry.shows(separator) {
            return self;
        }
        let (low, high, total) = geometry.bounds(separator);
        let share = Some(thousandths(cells.clamp(low, high), total));
        match separator {
            Separator::Aside => self.with_aside_width(share),
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
    /// The edge between the conversation and the object beside it (Session,
    /// from 100 columns).
    Beside,
    /// The conversation's title rule under the object (Workbench).
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
    /// Session conversation keeps 40 columns beside an object of 30, the
    /// Workbench conversation 8 rows under an object of 6.
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
        let (object, conversation, stacked) = match layout {
            Layout::Workbench => {
                let talk = conversation_rows(work.height, arrangement.conversation_height);
                let (object, conversation) = split_rows(work, talk);
                (object, conversation, true)
            }
            Layout::Session if area.width < SIDE_BY_SIDE_MIN_WIDTH => {
                // The conversation keeps at least half the rows: the composer is never squeezed out.
                let (object, conversation) = split_rows(work, work.height.div_ceil(2));
                (object, conversation, true)
            }
            Layout::Session => {
                let talk = conversation_width(work.width, arrangement.conversation_width);
                let seen = work.width - talk;
                (
                    Rect::new(work.x + talk, work.y, seen, work.height),
                    Rect::new(work.x, work.y, talk, work.height),
                    false,
                )
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
            Separator::Beneath => {
                (self.layout == Layout::Workbench).then(|| Rect::new(talk.x, talk.y, talk.width, 1))
            }
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
                        let side_by_side = arrangement.layout == Layout::Session
                            && width >= SIDE_BY_SIDE_MIN_WIDTH;
                        assert_eq!(g.stacked, !side_by_side, "{what}");
                        if g.stacked {
                            assert_eq!(g.conversation.x, g.object.x, "{what}");
                            assert_eq!(g.object.bottom(), g.conversation.y, "{what}");
                        } else {
                            assert!(g.conversation.width >= CONVERSATION_MIN_WIDTH, "{what}");
                            assert!(g.object.width >= OBJECT_MIN_WIDTH, "{what}");
                            assert_eq!(g.conversation.right(), g.object.x, "{what}");
                        }
                        if arrangement.layout == Layout::Workbench {
                            assert!(g.conversation.height >= CONVERSATION_MIN_ROWS, "{what}");
                            assert!(g.object.height >= OBJECT_MIN_ROWS, "{what}");
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

    /// The Workbench at the three target sizes: the object above, larger; the
    /// conversation and its composer under it; the project aside on the left
    /// from 120 columns, as wide as in Session.
    #[test]
    fn the_workbench_gives_the_object_the_room_at_the_target_sizes() {
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
        assert_eq!(large.object, Rect::new(21, 2, 99, 25));
        assert_eq!(large.conversation, Rect::new(21, 27, 99, 12));
        let wide = arranged(180, 48);
        assert_eq!(wide.aside, Some(Rect::new(0, 2, 32, 45)));
        assert_eq!(wide.object, Rect::new(32, 2, 148, 30));
        assert_eq!(wide.conversation, Rect::new(32, 32, 148, 15));
        for (width, height) in [(80, 24), (120, 40), (180, 48)] {
            let session = at(width, height, true);
            let workbench = arranged(width, height);
            assert!(
                cells_of(workbench.object) > cells_of(session.object),
                "{width}x{height}: the Workbench object is larger"
            );
            assert!(workbench.conversation.height < session.conversation.height);
            assert_eq!(
                workbench.aside, session.aside,
                "the project column never moves"
            );
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
        arrangement = arrangement.moved(Separator::Beneath, 20, area, false);
        let bench = Geometry::arranged(area, false, &arrangement).expect("fits");
        assert_eq!(bench.extent_of(Separator::Beneath), 20);
        assert_eq!(bench.extent_of(Separator::Aside), 27, "the aside is shared");
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
        let bench = Arrangement::of(Layout::Workbench);
        for (asked, kept) in [(0, CONVERSATION_MIN_ROWS), (500, 37 - OBJECT_MIN_ROWS)] {
            let moved = bench.moved(Separator::Beneath, asked, area, true);
            let g = Geometry::arranged(area, true, &moved).expect("fits");
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
        assert_eq!(
            bench.moved(Separator::Beside, 60, Rect::new(0, 0, 180, 48), false),
            bench
        );
        let chosen = bench.moved(Separator::Beneath, 12, small, false);
        assert_ne!(chosen, bench);
        assert_eq!(chosen.restored(Separator::Beneath), bench);
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
        let bench = Geometry::arranged(
            Rect::new(0, 0, 120, 40),
            true,
            &Arrangement::of(Layout::Workbench),
        )
        .expect("fits");
        assert_eq!(bench.handle(Separator::Beside), None);
        assert_eq!(
            bench.handle(Separator::Beneath),
            Some(Rect::new(21, 27, 99, 1))
        );
        assert_eq!(
            bench.separator_at(Position::new(60, 27)),
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
