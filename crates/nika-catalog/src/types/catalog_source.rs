// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Catalog data source abstraction for overlay composition.
//!
//! Defines [`CatalogDataSource`] (read-only catalog access) and
//! [`OverlayOrigin`] (provenance tracking). Lives in nika-catalog (L0)
//! rather than nika-kernel (L0.5) to avoid an L0 → L0.5 dependency cycle
//! — nika-catalog owns the concrete types these traits return.
//!
//! The runtime (L3) composes sources with `User > Pck > Core` precedence.

use core::fmt;

#[cfg(feature = "capabilities")]
use super::model::ModelCapabilities;
#[cfg(feature = "pricing")]
use super::model::{CostEstimate, ModelPricing};

/// Where a catalog overlay originates.
///
/// Precedence: `User > Pck > Core` — a user override always wins.
///
/// # Example
///
/// ```
/// use nika_catalog::types::catalog_source::OverlayOrigin;
///
/// assert!(OverlayOrigin::User > OverlayOrigin::Pck);
/// assert!(OverlayOrigin::Pck > OverlayOrigin::Core);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum OverlayOrigin {
    /// Built-in static catalog (this crate's generated data).
    Core = 0,
    /// Package-provided overlay (`nika-pck` registry).
    Pck = 1,
    /// User-local override (config file or runtime API).
    User = 2,
}

impl fmt::Display for OverlayOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core => f.write_str("core"),
            Self::Pck => f.write_str("pck"),
            Self::User => f.write_str("user"),
        }
    }
}

/// Read-only access to catalog data.
///
/// **Contract**: sync, `Send + Sync`, object-safe. All methods return
/// references or values — no `Future`, no allocation on the hot path.
///
/// The built-in implementation ([`StaticCatalogDataSource`]) delegates to
/// the crate's generated statics. Overlay sources (pck, user config)
/// implement this trait to layer on top.
///
/// # Feature-gated methods
///
/// Methods behind `#[cfg(feature = "...")]` are only present in the vtable
/// when the corresponding feature is enabled. Implementations MUST use
/// matching `#[cfg]` guards. Under the default `full` feature set, all
/// methods are present and the trait is object-safe.
pub trait CatalogDataSource: Send + Sync {
    /// Resolve model capabilities for a (provider, model) pair.
    #[cfg(feature = "capabilities")]
    fn model_capabilities(&self, provider: &str, model: &str) -> ModelCapabilities;

    /// Find pricing for a model (unscoped, 2-pass matching).
    #[cfg(feature = "pricing")]
    fn find_pricing(&self, model: &str) -> Option<&ModelPricing>;

    /// Find pricing scoped to a specific provider.
    #[cfg(feature = "pricing")]
    fn find_pricing_scoped(&self, provider: &str, model: &str) -> Option<&ModelPricing>;

    /// Estimate cost for a model invocation.
    #[cfg(feature = "pricing")]
    fn estimate_cost(
        &self,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<CostEstimate>;

    /// The origin of this data source (for precedence resolution).
    fn origin(&self) -> OverlayOrigin;
}

/// The built-in static catalog — delegates to generated data.
///
/// Zero-sized, `Copy`, free to construct. All methods forward to the
/// crate-level functions (`model_capabilities`, `find_pricing`, etc.).
///
/// # Example
///
/// ```
/// use nika_catalog::types::catalog_source::{CatalogDataSource, StaticCatalogDataSource, OverlayOrigin};
///
/// let ds = StaticCatalogDataSource;
/// assert_eq!(ds.origin(), OverlayOrigin::Core);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct StaticCatalogDataSource;

impl CatalogDataSource for StaticCatalogDataSource {
    #[cfg(feature = "capabilities")]
    fn model_capabilities(&self, provider: &str, model: &str) -> ModelCapabilities {
        crate::data::models::model_capabilities(provider, model)
    }

    #[cfg(feature = "pricing")]
    fn find_pricing(&self, model: &str) -> Option<&ModelPricing> {
        crate::data::models::find_pricing(model)
    }

    #[cfg(feature = "pricing")]
    fn find_pricing_scoped(&self, provider: &str, model: &str) -> Option<&ModelPricing> {
        crate::data::models::find_pricing_scoped(provider, model)
    }

    #[cfg(feature = "pricing")]
    fn estimate_cost(
        &self,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<CostEstimate> {
        crate::data::models::estimate_cost(model, input_tokens, output_tokens)
    }

    fn origin(&self) -> OverlayOrigin {
        OverlayOrigin::Core
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_origin_precedence_order() {
        assert!(OverlayOrigin::User > OverlayOrigin::Pck);
        assert!(OverlayOrigin::Pck > OverlayOrigin::Core);
        assert!(OverlayOrigin::User > OverlayOrigin::Core);
    }

    #[test]
    fn overlay_origin_display() {
        assert_eq!(OverlayOrigin::Core.to_string(), "core");
        assert_eq!(OverlayOrigin::Pck.to_string(), "pck");
        assert_eq!(OverlayOrigin::User.to_string(), "user");
    }

    #[test]
    fn static_source_is_core_origin() {
        let ds = StaticCatalogDataSource;
        assert_eq!(ds.origin(), OverlayOrigin::Core);
    }

    #[test]
    #[cfg(feature = "capabilities")]
    fn static_source_resolves_capabilities() {
        let ds = StaticCatalogDataSource;
        let caps = ds.model_capabilities("openai", "gpt-4o");
        assert!(caps.supports_temperature);
    }

    #[test]
    #[cfg(feature = "pricing")]
    fn static_source_resolves_pricing() {
        let ds = StaticCatalogDataSource;
        let p = ds.find_pricing("gpt-4o").unwrap();
        assert_eq!(p.provider, "OpenAI");
    }

    #[test]
    #[cfg(feature = "pricing")]
    fn static_source_scoped_pricing() {
        let ds = StaticCatalogDataSource;
        let p = ds.find_pricing_scoped("anthropic", "sonnet-4").unwrap();
        assert_eq!(p.provider, "Anthropic");
    }

    #[test]
    #[cfg(feature = "pricing")]
    fn static_source_estimate_cost() {
        let ds = StaticCatalogDataSource;
        let est = ds.estimate_cost("gpt-4o", 1000, 500).unwrap();
        assert!(est.usd > 0.0);
        assert_eq!(est.provider, "OpenAI");
    }

    /// Object-safety requires the full feature set (all cfg-gated methods
    /// present). Under `extension-author` (no capabilities/pricing), the
    /// vtable has fewer methods but is still object-safe — just untested.
    #[test]
    #[cfg(all(feature = "capabilities", feature = "pricing"))]
    fn trait_is_object_safe_full_features() {
        fn accepts_dyn(ds: &dyn CatalogDataSource) -> OverlayOrigin {
            ds.origin()
        }
        let ds = StaticCatalogDataSource;
        assert_eq!(accepts_dyn(&ds), OverlayOrigin::Core);
    }

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StaticCatalogDataSource>();
    }
}
