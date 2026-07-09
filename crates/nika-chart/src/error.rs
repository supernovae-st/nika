//! Typed failures — the future `NIKA-BUILTIN-CHART-NNN` sub-namespace.
//! Zero-dep: hand-rolled Display, no thiserror.

use std::fmt;

/// Compile/render failures. Every variant maps to a stable spec code so the
/// engine wiring (W2) can surface `NIKA-BUILTIN-CHART-NNN` without remapping.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChartError {
    /// CHART-001 · the data set is empty.
    EmptyData,
    /// CHART-002 · an encoding references a field absent from the rows.
    UnknownField { field: String },
    /// CHART-003 · a numeric field carries a non-finite value (NaN/±inf).
    NonFinite { field: String, row: usize },
    /// CHART-004 · the spec is structurally invalid for the chart type.
    InvalidSpec { reason: String },
    /// CHART-005 · the numeric domain collapsed (min == max after padding).
    DomainCollapsed { field: String },
    /// CHART-006 · a field expected numeric carried a string (or reverse).
    TypeMismatch { field: String, row: usize },
    /// CHART-007 · too many points for the declared soft cap.
    TooManyPoints { got: usize, cap: usize },
}

impl ChartError {
    /// The stable 3-digit code (spec 4-segment tail).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyData => "001",
            Self::UnknownField { .. } => "002",
            Self::NonFinite { .. } => "003",
            Self::InvalidSpec { .. } => "004",
            Self::DomainCollapsed { .. } => "005",
            Self::TypeMismatch { .. } => "006",
            Self::TooManyPoints { .. } => "007",
        }
    }
}

impl fmt::Display for ChartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NIKA-BUILTIN-CHART-{} · ", self.code())?;
        match self {
            Self::EmptyData => write!(f, "data has zero rows"),
            Self::UnknownField { field } => {
                write!(f, "encoding references unknown field `{field}`")
            }
            Self::NonFinite { field, row } => {
                write!(f, "non-finite value in field `{field}` at row {row}")
            }
            Self::InvalidSpec { reason } => write!(f, "invalid spec: {reason}"),
            Self::DomainCollapsed { field } => {
                write!(f, "numeric domain collapsed for field `{field}`")
            }
            Self::TypeMismatch { field, row } => {
                write!(f, "type mismatch in field `{field}` at row {row}")
            }
            Self::TooManyPoints { got, cap } => {
                write!(f, "{got} points exceed the soft cap {cap}")
            }
        }
    }
}

impl std::error::Error for ChartError {}
