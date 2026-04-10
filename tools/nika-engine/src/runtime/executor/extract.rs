// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Thin wrapper over `nika-extract` (Constellation Session 12).
//!
//! The full 1327 LOC extraction pipeline lives in the `nika-extract` L2
//! crate. This module is a transitional wrapper that adapts the crate's
//! `ExtractError` to engine-side `NikaError`, so existing call sites that
//! use `crate::runtime::executor::extract::apply_extract_with_base` keep
//! working until Session 12 commit F8 deletes this file and routes callers
//! directly through `nika_extract::extract`.

use nika_core::ast::extract::ExtractMode;

use crate::error::NikaError;

/// Apply extraction with an optional base URL for resolving relative links.
///
/// Delegates to [`nika_extract::extract`] and maps [`nika_extract::ExtractError`]
/// into [`NikaError`] via the `From` impl defined in `crate::error`.
pub(crate) fn apply_extract_with_base(
    body: &str,
    extract: Option<ExtractMode>,
    selector: Option<&str>,
    base_url: Option<&str>,
) -> Result<String, NikaError> {
    nika_extract::extract(body, extract, selector, base_url).map_err(Into::into)
}

/// Re-export the hreflang Link header parser for engine call sites.
#[allow(unused_imports)]
pub(crate) use nika_extract::parse_link_header_hreflang;

/// Test-only convenience: apply extraction without a base URL.
///
/// Engine test files still import this symbol from the old path. The
/// wrapper delegates to `nika_extract::extract` with `base_url = None`.
#[cfg(test)]
pub(crate) fn apply_extract(
    body: &str,
    extract: Option<ExtractMode>,
    selector: Option<&str>,
) -> Result<String, NikaError> {
    apply_extract_with_base(body, extract, selector, None)
}
