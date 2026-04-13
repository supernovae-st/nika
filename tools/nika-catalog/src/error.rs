// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Catalog-specific errors implementing [`NikaErrorCode`].

use nika_error::prelude::*;

/// Errors originating from catalog validation.
///
/// These are only raised during cross-reference validation (e.g. a provider's
/// `default_model` references a model with no pricing entry). Normal lookups
/// return `Option`, not errors.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[non_exhaustive]
pub enum CatalogError {
    /// A provider's default or cheap model has no pricing entry.
    #[error("provider `{provider}` references model `{model}` with no pricing")]
    #[diagnostic(help("add a pricing entry for `{model}` in data/models.rs"))]
    MissingPricing {
        /// Provider id.
        provider: &'static str,
        /// Model name that was not found.
        model: &'static str,
    },

    /// Duplicate entry detected in a catalog.
    #[error("duplicate {catalog} entry: `{name}`")]
    #[diagnostic(help("remove the duplicate from data/{catalog}.rs"))]
    DuplicateEntry {
        /// Catalog name (e.g. "providers", "builtins").
        catalog: &'static str,
        /// Duplicated entry name.
        name: String,
    },

    /// Sorted array is not actually sorted (compile-time invariant broken).
    #[error("{catalog} array is not sorted at index {index}: `{prev}` > `{current}`")]
    #[diagnostic(help("entries in data/{catalog}.rs must be sorted alphabetically by name"))]
    NotSorted {
        /// Catalog name.
        catalog: &'static str,
        /// Index where the ordering violation was found.
        index: usize,
        /// Previous entry name.
        prev: String,
        /// Current entry name (should be > prev).
        current: String,
    },
}

impl NikaErrorCode for CatalogError {
    fn nika_code(&self) -> NikaCode {
        // All catalog errors fall under Core/validation (NIKA-001)
        // because they indicate invalid static data, not runtime failures.
        codes::NIKA_001
    }
}
