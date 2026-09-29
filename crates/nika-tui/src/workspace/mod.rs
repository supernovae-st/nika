// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workspace screen, drawn on the full terminal (T-nika-tui-layout).
//!
//! The screen answers four questions without opening a panel: where am I
//! (the header names the host, the active project and its location), what am I
//! looking at (the object in the centre: a workflow, a file, a run, a proof),
//! which conversation do I write to (its thread and composer, side by side
//! with the object on a wide terminal, below it on a narrow one) and which run
//! asks for my attention (the pinned activity row). The project aside lists
//! what the project holds when the width allows it.
//!
//! This module decides geometry and paints only facts it is given: every
//! value comes from a view the Session projects (UI-STATE), never from a
//! guess. Nothing here reads a file, a clock or the environment, and nothing
//! grants an access: a project, a file or a connection on screen is never an
//! authority. The inline presentation and the plain loop stay as they are.

pub mod aside;
pub mod conversation;
pub mod geometry;
pub mod header;
pub mod object;
pub mod pinned;
pub mod screen;
mod text;
