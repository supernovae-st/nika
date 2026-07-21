// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parser's key vocabularies, exposed as ONE read surface.
//!
//! The vocabularies AND this door DESCENDED to [`nika_vocab::keys`] at
//! the C2 flag-day (the 15k prod-LOC wall — key vocabularies are
//! vocabulary, the vocabulary crate is their home). This shim keeps the
//! `nika_schema::keysets::known_child_keys` path (the LSP's completion
//! door) byte-stable.

pub use nika_vocab::keys::known_child_keys;
