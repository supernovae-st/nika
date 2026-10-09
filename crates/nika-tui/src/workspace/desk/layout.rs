// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The desk's arrangement: whether the object is expanded (the internal v1
//! `Workbench` layout; `Session` is the object restored), where the
//! separators stand, and the one geometry every reader of the frame takes
//! (drawing, the keys, the pointer, the transcript's scroll).
//!
//! Expanding or restoring the object (`F4`, the object's own action) and
//! moving a separator (the pointer, or `+` · `-` · `0` in the aside or the
//! object) change the view only: the draft, the keyboard focus, the object in
//! view and its face, runs and conversation scroll stay where they were, and
//! nothing is sent, approved or saved. The object expands only where that
//! enlarges it, and a kept expansion always restores. A change settles once
//! no separator is held; the shell takes it then ([`Desk::take_settled`]) for
//! its host to keep. A current decision the restored stacked split cannot
//! show folds the object to its strip for as long as it is current
//! ([`Desk::folds`]); nothing of that is kept.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Rect};

use super::{Desk, Route};
use crate::model::{Presentation, UiState};
use crate::render::question;
use crate::workspace::cards;
use crate::workspace::cards::review::{self, Review};
use crate::workspace::focus::Region;
use crate::workspace::geometry::{Arrangement, Geometry, Layout, Separator};
use crate::workspace::project::{self, Opened};
use crate::workspace::screen;

/// The rows of the exchange that led to a homed question or a reviewed
/// proposal its restored stacked conversation keeps above the decision at
/// rest; short of them, the object folds to its strip.
pub(super) const DECISION_CONTEXT: usize = 3;

/// The columns a separator key moves the aside or the conversation by.
const COLUMN_STEP: u16 = 2;
/// The rows a separator key moves the conversation under the expanded object by.
const ROW_STEP: u16 = 1;

/// A separator the pointer moves: which one, where its button went down, and
/// the cells its region held then (the motion moves it from there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Drag {
    separator: Separator,
    from: Position,
    cells: u16,
}

impl Desk {
    /// The regions of a terminal of `size` as the desk arranges them, the
    /// stacked object folded while a decision asks it ([`Desk::folds`]);
    /// `None` below the minimum. Every reader of the frame takes this one.
    #[must_use]
    pub(crate) fn geometry(&self, size: (u16, u16)) -> Option<Geometry> {
        let geometry = self.arranged(size)?;
        Some(if self.folds(size) {
            screen::folded(geometry)
        } else {
            geometry
        })
    }

    /// The regions of a terminal of `size` as the arrangement alone lays them out.
    fn arranged(&self, size: (u16, u16)) -> Option<Geometry> {
        let area = Rect::new(0, 0, size.0, size.1);
        Geometry::arranged(area, self.pins(), &self.arrangement)
    }

    /// Whether a current decision folds the stacked object to its strip on a
    /// terminal of `size` ([`screen::folded`]): decided once per frame
    /// ([`Desk::prepare_for`]) and held while the size, the arrangement and
    /// the pin it was decided for hold; never while the aside holds the keys,
    /// its overlay taking the object's restored rows.
    #[must_use]
    pub(crate) fn folds(&self, size: (u16, u16)) -> bool {
        self.fold == Some((size, self.arrangement, self.pins()))
            && self.focus.region != Region::Aside
    }

    /// Whether the current typed decision overflows the restored stacked
    /// conversation of the workspace: the proposal the conversation reviews
    /// or the question its live card carries (never a gate, a choice, a draft
    /// or set-aside candidate, or another identity), from its piece to the
    /// end, beside the live area at rest as that review reads it
    /// ([`screen::rest_transcript`]); a reviewed proposal and a homed question
    /// also ask [`DECISION_CONTEXT`] rows of the exchange that led to them.
    pub(super) fn overflowed(&self, state: &UiState, review: Option<&Review>) -> bool {
        let current = review::summarized(state, review);
        let reviewed = current.map(|(at, _)| at);
        let asked = question::asked_block(state);
        let (Some(from), Some(geometry)) = (reviewed.or(asked), self.arranged(state.size)) else {
            return false;
        };
        if state.presentation != Presentation::Workspace
            || self.arrangement.layout != Layout::Session
            || !geometry.stacked
        {
            return false;
        }
        let label = self.shown().map(Opened::label);
        let thread = project::thread(self.view.as_ref(), label.as_deref());
        let consent = current.map(|(_, review)| review);
        let transcript = screen::rest_transcript(&geometry, state, &thread, consent);
        let context = if reviewed.is_some() || question::rest_homed(state) {
            DECISION_CONTEXT
        } else {
            0
        };
        let decision = cards::rows_from(state, transcript, (review, asked), from);
        decision + context > usize::from(transcript.height)
    }

    /// The layout in view and the separators' shares (the shell reads it
    /// through [`Desk::toggled`] and [`Desk::take_settled`]).
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn arrangement(&self) -> Arrangement {
        self.arrangement
    }

    /// Show `arrangement`, as its host kept it: nothing is left to keep.
    pub(crate) fn arrange(&mut self, arrangement: Arrangement) {
        self.arrangement = Arrangement::of(arrangement.layout)
            .with_aside_width(arrangement.aside_width)
            .with_conversation_width(arrangement.conversation_width)
            .with_conversation_height(arrangement.conversation_height);
        self.drag = None;
        self.unsettled = false;
    }

    /// The arrangement once, after it changed and no separator is held: the
    /// moment for the shell's host to keep it.
    pub(crate) fn take_settled(&mut self) -> Option<Arrangement> {
        (self.drag.is_none() && std::mem::take(&mut self.unsettled)).then_some(self.arrangement)
    }

    /// What `F4` and the object's own action show next on a terminal of
    /// `size` ([`screen::toggled`]); `None` where they change nothing.
    #[must_use]
    pub(crate) fn toggled(&self, size: (u16, u16)) -> Option<Layout> {
        let area = Rect::new(0, 0, size.0, size.1);
        screen::toggled(area, self.pins(), &self.arrangement, self.folds(size))
    }

    /// Expand or restore the object (`F4`, its own action) on a terminal of
    /// `size`; `false` where that would change nothing (the object cannot
    /// grow there): the arrangement stays as it is.
    pub(crate) fn toggle_layout(&mut self, size: (u16, u16)) -> bool {
        self.toggled(size).is_some_and(|next| self.set_layout(next))
    }

    /// Show `layout`; `false` when it is already in view. The shares, the
    /// draft, focus, object and conversation scroll stay. A graph-format
    /// change restarts its object reading at the top.
    pub(crate) fn set_layout(&mut self, layout: Layout) -> bool {
        self.drag = None;
        self.rearranged(self.arrangement.with_layout(layout))
    }

    /// A separator key where a region other than the composer holds the
    /// keys: `+` (or `=`) widens or heightens that region, `-` (or `_`)
    /// narrows it, `0` gives it its automatic share (beside the expanded
    /// object, where the conversation is at its narrowest, it already has
    /// it). `None` leaves the key to the region (the composer's region types
    /// these characters).
    pub(crate) fn separator_key(&mut self, key: KeyEvent, size: (u16, u16)) -> Option<Route> {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        let grow = match key.code {
            KeyCode::Char('+' | '=') => Some(true),
            KeyCode::Char('-' | '_') => Some(false),
            KeyCode::Char('0') => None,
            _ => return None,
        };
        let geometry = self.geometry(size)?;
        let separator = match self.focus.region {
            Region::Aside => Separator::Aside,
            Region::Object if geometry.stacked => Separator::Beneath,
            Region::Object => Separator::Beside,
            _ => return None,
        };
        if !geometry.shows(separator) {
            return Some(Route::Nothing);
        }
        let expanded = self.arrangement.layout == Layout::Workbench;
        let next = match grow {
            None if expanded && separator == Separator::Beside => self.arrangement,
            None => self.arrangement.restored(separator),
            Some(grow) => {
                // The aside grows with its separator; the object grows as the
                // conversation beside or under it gives cells away.
                let (step, wider) = match separator {
                    Separator::Aside => (COLUMN_STEP, grow),
                    Separator::Beside => (COLUMN_STEP, !grow),
                    Separator::Beneath => (ROW_STEP, !grow),
                    _ => return Some(Route::Nothing),
                };
                let now = geometry.extent_of(separator);
                let cells = if wider {
                    now.saturating_add(step)
                } else {
                    now.saturating_sub(step)
                };
                let area = Rect::new(0, 0, size.0, size.1);
                self.arrangement.moved(separator, cells, area, self.pins())
            }
        };
        Some(if self.rearranged(next) {
            Route::Repaint
        } else {
            Route::Nothing
        })
    }

    /// The pointer's button went down at `point`: when it is on a separator,
    /// the separator follows the pointer until the button comes up. Any move
    /// held before is let go.
    pub(crate) fn press(&mut self, point: Position, size: (u16, u16)) -> Option<Separator> {
        self.drag = None;
        let geometry = self.geometry(size)?;
        let separator = geometry.separator_at(point)?;
        self.drag = Some(Drag {
            separator,
            from: point,
            cells: geometry.extent_of(separator),
        });
        Some(separator)
    }

    /// The pointer moved to `point` with its button down: the held separator
    /// follows within its bounds; `true` when it moved.
    pub(crate) fn drag_to(&mut self, point: Position, size: (u16, u16)) -> bool {
        let Some(drag) = self.drag else {
            return false;
        };
        let moved = match drag.separator {
            Separator::Aside | Separator::Beside => i32::from(point.x) - i32::from(drag.from.x),
            // The conversation is under its rule: up gives it rows.
            Separator::Beneath => i32::from(drag.from.y) - i32::from(point.y),
            _ => return false,
        };
        let cells = u16::try_from((i32::from(drag.cells) + moved).max(0)).unwrap_or(u16::MAX);
        let area = Rect::new(0, 0, size.0, size.1);
        let next = (self.arrangement).moved(drag.separator, cells, area, self.pins());
        self.rearranged(next)
    }

    /// The pointer's button came up (or moved without it): the held
    /// separator stays where it is; `true` when one was held.
    pub(crate) fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// The separator the pointer holds.
    #[must_use]
    pub(crate) fn dragging(&self) -> Option<Separator> {
        self.drag.map(|drag| drag.separator)
    }

    /// Take `next`; `true` when it differs from the arrangement in view.
    fn rearranged(&mut self, next: Arrangement) -> bool {
        if next == self.arrangement {
            return false;
        }
        self.arrangement = next;
        self.unsettled = true;
        true
    }
}
