// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The pure parts of the workspace screen, shared with the renderer that owns
//! the screen (`nika-tui`, ADR-143): where each region stands ([`geometry`]),
//! the header ([`header`]), the project aside ([`aside`]), the pinned run
//! ([`pinned`]), the object in view ([`object`]) and the conversation's own
//! rows ([`conversation`]), with the fitting of words into cells they share
//! ([`text`]).
//!
//! Each is a pure function of what it is handed: facts the Session projected,
//! bytes or lines already held, a region's cells, the elapsed time the caller
//! read. Out come styled lines, cells or rectangles. Nothing here reads a
//! file, a clock or the environment, spawns, stores or owns the terminal, and
//! nothing grants an access: the desk, the keys, the Session and its host stay
//! in the renderer.

pub mod aside;
pub mod conversation;
pub mod geometry;
pub mod header;
pub mod object;
pub mod pinned;
pub mod text;
