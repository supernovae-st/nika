// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shared media core — vocabulary and scaffold the media-class
//! builtin families (`image/` · `tts/` · future `video/`) draw from.
//!
//! Seeded per the 2026-07-06 architecture review (P1.1): the second
//! family already IMPORTED from the first instead of copying, so the
//! shared core exists either way — this module gives it a home with the
//! dependency direction pointing the right way (families depend on
//! `media`, never on each other). Sub-module boundaries align with the
//! reserved `nika-media-*` crate split, so graduation is a `git mv`.
//!
//! Grows by RE-HOMING only (behavior-free moves pinned by the mock's
//! byte-stable goldens) — never by speculation.

pub(crate) mod png;
pub(crate) mod time;
pub(crate) mod wire;
