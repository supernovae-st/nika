// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The crate test dir-module — the F-P8 suites, relocated out of
//! `tests/` so the mutation gate (`cargo mutants -- --lib`, Gate 5)
//! exercises them: integration targets never run under `--lib`, and a
//! gate that runs zero tests kills zero mutants. `src/tests.rs` is the
//! prod-LOC-exempt convention the hygiene vectors share (`rs_prod_files`
//! excludes the basename); each submodule carries its own
//! `#![allow(clippy::expect_used)]` header, the skip the unwrap vector
//! keys on. `crate::` resolves to the crate root — semantics unchanged.

#[cfg(test)]
mod acceptance;
#[cfg(test)]
mod api;
#[cfg(test)]
mod tamper_property;
