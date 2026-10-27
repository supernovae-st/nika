// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The bill of a paid media call (C-D3): the `cost_usd` a returned image or
//! speech call reports, and how it was computed. A provider that reports its
//! exact spend (xAI) is taken at its word; otherwise the dated media table
//! (`nika_catalog::media_rates`) prices the call from what the provider
//! reported or billed: its rates applied to the reported token usage, its
//! per-image price times the images delivered when an image response reports
//! no usage, its per-character rate times the characters sent. A model the
//! table does not price reports no `cost_usd`: unknown, never free.

/// How a media call's `cost_usd` was computed: the output's `cost_basis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CostBasis {
    /// The provider reported its exact spend (xAI's cost ticks).
    ProviderReported,
    /// The dated catalog rates applied to the token usage the provider
    /// reported.
    RateTimesUsage,
    /// No usage reported: the dated per-image price × the images delivered.
    PriceTimesImages,
    /// The dated per-character rate × the characters the call sent.
    RateTimesCharacters,
}

impl CostBasis {
    /// The wire word.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
            Self::RateTimesUsage => "catalog_rate_x_usage",
            Self::PriceTimesImages => "catalog_price_x_images",
            Self::RateTimesCharacters => "catalog_rate_x_characters",
        }
    }
}

/// A returned media call's spend: USD, how it was computed, and the date of
/// the catalog rates it read (`None` when the provider reported it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MediaBill {
    pub(crate) usd: f64,
    pub(crate) basis: CostBasis,
    pub(crate) as_of: Option<&'static str>,
}

impl MediaBill {
    /// The provider's own report of what it billed.
    pub(crate) fn reported(usd: f64) -> Self {
        Self {
            usd,
            basis: CostBasis::ProviderReported,
            as_of: None,
        }
    }

    /// A bill read from the dated table's rates.
    pub(crate) fn catalog(usd: f64, basis: CostBasis, as_of: &'static str) -> Self {
        Self {
            usd,
            basis,
            as_of: Some(as_of),
        }
    }

    /// Write the basis beside `cost_usd` on an output or a manifest:
    /// `cost_basis`, and `cost_as_of` (the rates' date) for a catalog bill.
    pub(crate) fn annotate(self, record: &mut serde_json::Value) {
        if let serde_json::Value::Object(map) = record {
            map.insert("cost_basis".into(), self.basis.id().into());
            if let Some(as_of) = self.as_of {
                map.insert("cost_as_of".into(), as_of.into());
            }
        }
    }
}

/// The `cost_usd` of an optional bill (absent stays absent).
pub(crate) fn usd(bill: Option<MediaBill>) -> Option<f64> {
    bill.map(|bill| bill.usd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bill_names_its_basis_and_the_date_of_its_rates() {
        let mut record = serde_json::json!({ "cost_usd": 0.067 });
        MediaBill::catalog(0.067, CostBasis::PriceTimesImages, "2026-10-27").annotate(&mut record);
        assert_eq!(record["cost_basis"], "catalog_price_x_images");
        assert_eq!(record["cost_as_of"], "2026-10-27");
        let mut record = serde_json::json!({ "cost_usd": 0.02 });
        MediaBill::reported(0.02).annotate(&mut record);
        assert_eq!(record["cost_basis"], "provider_reported");
        assert!(record.get("cost_as_of").is_none(), "no catalog date");
        assert_eq!(usd(None), None);
        for basis in [CostBasis::RateTimesUsage, CostBasis::RateTimesCharacters] {
            assert!(basis.id().starts_with("catalog_rate_x_"), "{basis:?}");
        }
    }
}
