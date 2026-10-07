// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! How one clause of a request reads above the reader. Pure over the words: nothing here asks a
//! judge, reads a candidate, calls a model or grants authority.
//!
//! - [`parts`] · the parts of a request a verifier asks alone, each an exact excerpt of it, and
//!   how each reads: whether it restricts, whether it may ask an operation of its own.
//! - [`prohibition`] · whether a clause forbids by negation alone, demands by a negation of
//!   forgetting, or states an operation of its own beside a law or a negation.
//! - [`words`] · the word tables of the proposal merge and their classifiers: the key a clause
//!   is folded by, a draft that is no language work, a format kept by construction.
//! - [`spellings`] · the literals a clause states that the host observed spelled with other bytes.
//!
//! A size-cap member of the `nika-onboard` unit (ADR-145 · D-2026-07-09-N1 · the ADR-142
//! precedent), placed above the reader and below the seats' doors: `nika-compile-cognition` →
//! `nika-compile-clauses` → `nika-compile-reader`, never back. The prohibition reading and the
//! proposal merge's words ascended from the reader, the parts from the cognition's verifier and
//! the stated spellings from `nika-compile`'s surface, at the 15k prod-LOC wall. The reader's
//! modules are bound at the paths the moved readings have always used (`super::stages`).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod parts;
pub mod prohibition;
pub mod spellings;
pub mod words;

use nika_compile_reader::{lexicon, rule_tokens, rules, stages};
