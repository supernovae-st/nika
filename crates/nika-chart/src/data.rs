//! Row model + extraction. Vec-of-BTreeMap keeps field order canonical and
//! iteration deterministic (`HashMap` is clippy-banned crate-wide).

use std::collections::BTreeMap;

use crate::error::ChartError;

/// A cell value. Numbers are f64 (validated finite at extraction).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    Num(f64),
    Str(String),
}

/// One data row · field → value.
pub type Row = BTreeMap<String, Value>;

/// Convenience row constructor for tests/demos.
#[must_use]
pub fn row(pairs: &[(&str, Value)]) -> Row {
    let mut r = Row::new();
    for (k, v) in pairs {
        r.insert((*k).to_owned(), v.clone());
    }
    r
}

/// Extract a numeric column in row order · rejects missing/typed/non-finite.
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn numeric_column(rows: &[Row], field: &str) -> Result<Vec<f64>, ChartError> {
    let mut out = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        match r.get(field) {
            None => {
                return Err(ChartError::UnknownField {
                    field: field.to_owned(),
                });
            }
            Some(Value::Str(_)) => {
                return Err(ChartError::TypeMismatch {
                    field: field.to_owned(),
                    row: i,
                });
            }
            Some(Value::Num(v)) => {
                if !v.is_finite() {
                    return Err(ChartError::NonFinite {
                        field: field.to_owned(),
                        row: i,
                    });
                }
                out.push(*v);
            }
        }
    }
    Ok(out)
}

/// Extract a string column in row order (numbers are refused — categorical
/// channels demand strings · keeps semantics honest).
/// # Errors
/// Typed [`crate::error::ChartError`] (`NIKA-BUILTIN-CHART-*`) on invalid spec or data.
pub fn string_column(rows: &[Row], field: &str) -> Result<Vec<String>, ChartError> {
    let mut out = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        match r.get(field) {
            None => {
                return Err(ChartError::UnknownField {
                    field: field.to_owned(),
                });
            }
            Some(Value::Num(_)) => {
                return Err(ChartError::TypeMismatch {
                    field: field.to_owned(),
                    row: i,
                });
            }
            Some(Value::Str(s)) => out.push(s.clone()),
        }
    }
    Ok(out)
}

/// Min/max of a finite column (caller guarantees non-empty via compile gate).
#[must_use]
pub fn extent(values: &[f64]) -> Option<(f64, f64)> {
    let first = *values.first()?;
    let mut lo = first;
    let mut hi = first;
    for v in values {
        if *v < lo {
            lo = *v;
        }
        if *v > hi {
            hi = *v;
        }
    }
    Some((lo, hi))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unreadable_literal
)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nan() {
        let rows = vec![row(&[("v", Value::Num(f64::NAN))])];
        let e = numeric_column(&rows, "v");
        assert!(matches!(e, Err(ChartError::NonFinite { .. })));
    }

    #[test]
    fn rejects_missing_field() {
        let rows = vec![row(&[("v", Value::Num(1.0))])];
        assert!(matches!(
            numeric_column(&rows, "w"),
            Err(ChartError::UnknownField { .. })
        ));
    }

    #[test]
    fn extent_finds_bounds() {
        assert_eq!(extent(&[3.0, -1.0, 7.5]), Some((-1.0, 7.5)));
    }
}
