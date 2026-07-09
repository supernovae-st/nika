//! The compact chart spec — the operator-facing intent surface.
//! Mirrors the master plan §3 `with:` grammar (house keys · Flint-shaped).

/// Run-domain semantic types (closed v1 · additive minors only).
/// The Flint lever specialized: semantics drive formatting + palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Semantic {
    /// Money in USD · micro-grain honesty (cost-intelligence rules).
    Usd,
    /// Milliseconds · humanized (8ms · 1.2s · 2m05s).
    DurationMs,
    /// Token counts · k/M compaction.
    Tokens,
    /// Plain counts.
    Count,
    /// Signed change · diverging palette anchored at 0.
    Delta,
    /// Ratio in `[0,1]` rendered as percent.
    Percent,
    /// Unix epoch milliseconds · short date/time labels.
    Timestamp,
    /// Categorical string.
    Category,
}

/// v1 chart types (5 · each pinned to a run-surface consumer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChartType {
    /// Per-category quantity (per-node duration).
    Bar,
    /// Ordered quantity over x (run-over-run cost).
    Line,
    /// Quantile band + median line (forecast p50/p90) + optional actual.
    AreaBand,
    /// Two quantities (cost vs duration outliers).
    Scatter,
    /// Category × category → quantity color (step × run flakiness).
    Heatmap,
}

/// Field binding + semantic for one encoding channel.
#[derive(Debug, Clone)]
pub struct Channel {
    pub field: String,
    pub semantic: Semantic,
}

impl Channel {
    #[must_use]
    pub fn new(field: &str, semantic: Semantic) -> Self {
        Self {
            field: field.to_owned(),
            semantic,
        }
    }
}

/// The compact spec. `y_lo`/`y_hi` only serve `AreaBand` (p50/p90 bounds);
/// `y` is the median (or actual overlay when `y2` is present).
#[derive(Debug, Clone)]
pub struct ChartSpec {
    pub chart: ChartType,
    pub title: String,
    pub x: Channel,
    pub y: Channel,
    /// `AreaBand`: lower quantile bound (e.g. p50).
    pub y_lo: Option<Channel>,
    /// `AreaBand`: upper quantile bound (e.g. p90).
    pub y_hi: Option<Channel>,
    /// Optional second series overlay (`AreaBand`: the actual line).
    pub y2: Option<Channel>,
    /// Optional series/color split (Line: one line per category).
    pub color: Option<Channel>,
    pub width: u32,
    pub height: u32,
}

impl ChartSpec {
    /// Model-facing canvas ceiling (SQ-N · a hallucinated dimension must
    /// die typed, never allocate): generous for any report, fatal for `DoS`.
    pub const MAX_DIM: u32 = 4096;

    /// Minimal well-formedness (structure only · data checks live in compile).
    /// # Errors
    /// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
    pub fn validate(&self) -> Result<(), crate::error::ChartError> {
        use crate::error::ChartError::InvalidSpec;
        if self.width < 120 || self.height < 90 {
            return Err(InvalidSpec {
                reason: format!("canvas {}x{} below 120x90 floor", self.width, self.height),
            });
        }
        if self.width > Self::MAX_DIM || self.height > Self::MAX_DIM {
            return Err(InvalidSpec {
                reason: format!(
                    "canvas {}x{} above the {}x{} ceiling (a hallucinated dimension \
                     must fail typed, never allocate)",
                    self.width,
                    self.height,
                    Self::MAX_DIM,
                    Self::MAX_DIM
                ),
            });
        }
        if self.chart == ChartType::AreaBand && (self.y_lo.is_none() || self.y_hi.is_none()) {
            return Err(InvalidSpec {
                reason: "area_band requires y_lo and y_hi channels".to_owned(),
            });
        }
        if self.title.len() > 200 {
            return Err(InvalidSpec {
                reason: "title exceeds 200 chars".to_owned(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod ceiling_tests {
    use super::*;

    fn spec(w: u32, h: u32) -> ChartSpec {
        ChartSpec {
            chart: ChartType::Bar,
            title: "t".to_owned(),
            x: Channel::new("a", Semantic::Category),
            y: Channel::new("b", Semantic::Count),
            y_lo: None,
            y_hi: None,
            y2: None,
            color: None,
            width: w,
            height: h,
        }
    }

    #[test]
    fn hallucinated_dimension_dies_typed() {
        let e = spec(999_999_999, 300).validate();
        let msg = format!("{}", e.expect_err("must fail"));
        assert!(msg.contains("ceiling"));
        assert!(msg.starts_with("NIKA-BUILTIN-CHART-004"));
        assert!(spec(300, 999_999_999).validate().is_err());
    }

    #[test]
    fn ceiling_boundary_is_inclusive() {
        assert!(
            spec(ChartSpec::MAX_DIM, ChartSpec::MAX_DIM)
                .validate()
                .is_ok()
        );
        assert!(spec(ChartSpec::MAX_DIM + 1, 300).validate().is_err());
    }
}
