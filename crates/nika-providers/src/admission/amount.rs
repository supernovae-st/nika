// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use nika_types::cost::Cost;

/// Convert the already validated binary number downward without another
/// monetary grammar or an upward-rounded floating multiplication.
/// # Errors
/// Negative, nonfinite and overflowing amounts refuse.
pub fn allowance(amount: f64) -> Result<Cost, String> {
    if !amount.is_finite() || amount < 0.0 {
        return Err("invalid catalog allowance".into());
    }
    let bits = amount.to_bits();
    let raw_exp = i32::try_from((bits >> 52) & 0x7ff).map_err(|e| e.to_string())?;
    let fraction = bits & ((1u64 << 52) - 1);
    let mantissa = if raw_exp == 0 {
        fraction
    } else {
        fraction | (1u64 << 52)
    };
    let exponent = if raw_exp == 0 { -1074 } else { raw_exp - 1075 };
    let scaled = u128::from(mantissa) * 1_000_000_000;
    let nanos = if exponent < 0 {
        let shift = u32::try_from(-exponent).map_err(|e| e.to_string())?;
        scaled.checked_shr(shift).unwrap_or(0)
    } else {
        let shift = u32::try_from(exponent).map_err(|e| e.to_string())?;
        1u128
            .checked_shl(shift)
            .and_then(|n| scaled.checked_mul(n))
            .ok_or("catalog allowance overflow")?
    };
    i128::try_from(nanos)
        .map(Cost::new)
        .map_err(|_| "catalog allowance overflow".into())
}
