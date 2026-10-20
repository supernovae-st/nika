// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workspace screen, drawn on the full terminal (T-nika-tui-layout).
//!
//! The screen answers four questions without opening a panel: where am I
//! (the header names the host, the active project and its location), what am I
//! looking at (the preview on the right: a workflow, a file, a run, a proof),
//! which conversation do I write to (its thread and composer, side by side
//! with the object on a wide terminal, below it on a narrow one) and which run
//! asks for my attention (the pinned activity row). The project aside lists
//! what the project holds when the width allows it.
//!
//! This module decides geometry and paints only facts it is given: every
//! value comes from the view the conversation lends ([`project`], read-only,
//! a projection of what its Session observed), never from a guess. Nothing
//! here reads a file, a clock or the environment, and nothing grants an
//! access: a project, a file or a connection on screen is never an authority.
//! `Ctrl+T` opens it from inline when the terminal holds it (the crate-private
//! `desk` keeps its state and routes its keys); the inline presentation and
//! the plain loop stay as they are.

pub mod aside;
pub mod candidate;
pub(crate) mod cards;
pub mod conversation;
pub(crate) mod desk;
pub(crate) use nika_tui_view::workspace::fired;
pub mod focus;
pub mod geometry;
pub mod header;
pub mod inspect;
/// The live run in view, relocated to [`nika_tui_view::workspace::live`] (ADR-143): this path
/// keeps the renderer's callers unchanged. Its tests stay here, beside the desk that drives it.
pub use nika_tui_view::workspace::live;
pub mod object;
pub mod pinned;
pub mod project;
pub mod screen;
mod text;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod live_tests;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod live_task_tests;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod live_child_tests;
