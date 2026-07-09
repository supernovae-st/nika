//! Deterministic formatting.
//!
//! Coordinates NEVER go through float `Display` (no cross-version guarantee ·
//! CHT §3bis law 1): fixed-decimal via integer math, trailing zeros trimmed.
//! Tick labels are semantic-typed (the Flint lever · run-domain).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)] // fixed-decimal integer math: f64→i64 rounding is the design
use std::fmt::Write as _;

use crate::spec::Semantic;
use crate::ticks::pow10;

/// Fixed decimals via integer math (dp ≤ 9) · trailing zeros trimmed.
pub(crate) fn fixed_trim(v: f64, dp: u32) -> String {
    let scale = pow10(dp as i32);
    let scaled = (v * scale).round() as i64;
    let neg = scaled < 0;
    let abs = scaled.unsigned_abs();
    let base = pow10(dp as i32) as u64;
    let int = if dp == 0 { abs } else { abs / base };
    let mut frac = if dp == 0 { 0 } else { abs % base };
    let mut fdigits = dp;
    while fdigits > 0 && frac % 10 == 0 {
        frac /= 10;
        fdigits -= 1;
    }
    let mut s = String::with_capacity(12);
    if neg && (int > 0 || frac > 0) {
        s.push('-');
    }
    s.push_str(&int.to_string());
    if fdigits > 0 {
        s.push('.');
        let fs = frac.to_string();
        for _ in 0..(fdigits as usize - fs.len()) {
            s.push('0');
        }
        s.push_str(&fs);
    }
    s
}

/// USD ladder (cost-intelligence grain honesty · micro-USD visible).
fn usd(v: f64) -> String {
    let a = v.abs();
    let dp = if a >= 100.0 {
        0
    } else if a >= 1.0 {
        2
    } else if a >= 0.01 {
        4
    } else {
        6
    };
    if v == 0.0 {
        return "0".to_owned();
    }
    let body = fixed_trim(v.abs(), dp);
    let sign = if v < 0.0 { "-" } else { "" };
    format!("{sign}${body}")
}

/// Milliseconds humanized: 8ms · 1.2s · 2m05s.
fn duration_ms(v: f64) -> String {
    let a = v.abs().round() as i64;
    let sign = if v < 0.0 && a != 0 { "-" } else { "" };
    if a == 0 {
        "0".to_owned()
    } else if a < 1000 {
        format!("{sign}{a}ms")
    } else if a < 60_000 {
        format!("{sign}{}s", fixed_trim(v.abs() / 1000.0, 1))
    } else {
        let total_s = a / 1000;
        let m = total_s / 60;
        let s = total_s % 60;
        format!("{sign}{m}m{s:02}s")
    }
}

/// Token counts: 850 · 1.2k · 3.4M.
fn tokens(v: f64) -> String {
    let a = v.abs();
    if a < 1000.0 {
        fixed_trim(v, 0)
    } else if a < 1e6 {
        format!("{}k", fixed_trim(v / 1000.0, 1))
    } else {
        format!("{}M", fixed_trim(v / 1e6, 1))
    }
}

/// Epoch-ms → "MM-DD" (civil-from-days · Hinnant integer algorithm).
fn date_label(epoch_ms: f64) -> String {
    let days = (epoch_ms / 86_400_000.0).floor() as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{m:02}-{d:02}")
}

/// JSON string escape (manifest + Vega-Lite emitters · zero-dep).
pub(crate) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Semantic tick label.
#[must_use]
pub fn label(v: f64, sem: Semantic) -> String {
    match sem {
        Semantic::Usd => usd(v),
        Semantic::DurationMs => duration_ms(v),
        Semantic::Tokens => tokens(v),
        Semantic::Count => fixed_trim(v, u32::from(v.fract() != 0.0)),
        Semantic::Delta => {
            let body = fixed_trim(v.abs(), u32::from(v.fract() != 0.0));
            if v > 0.0 {
                format!("+{body}")
            } else if v < 0.0 {
                format!("-{body}")
            } else {
                body
            }
        }
        Semantic::Percent => format!("{}%", fixed_trim(v * 100.0, 1)),
        Semantic::Timestamp => date_label(v),
        Semantic::Category => fixed_trim(v, 0),
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
mod tests {
    use super::*;

    #[test]
    fn usd_ladder() {
        assert_eq!(label(1.25, Semantic::Usd), "$1.25");
        assert_eq!(label(0.000062, Semantic::Usd), "$0.000062");
        assert_eq!(label(120.0, Semantic::Usd), "$120");
        assert_eq!(label(0.0234, Semantic::Usd), "$0.0234");
        assert_eq!(label(-0.5, Semantic::Usd), "-$0.5");
    }

    #[test]
    fn duration_ladder() {
        assert_eq!(label(8.0, Semantic::DurationMs), "8ms");
        assert_eq!(label(0.0, Semantic::DurationMs), "0");
        assert_eq!(label(0.0, Semantic::Usd), "0");
        assert_eq!(label(1200.0, Semantic::DurationMs), "1.2s");
        assert_eq!(label(2000.0, Semantic::DurationMs), "2s");
        assert_eq!(label(125_000.0, Semantic::DurationMs), "2m05s");
        assert_eq!(label(-500.0, Semantic::DurationMs), "-500ms");
        assert_eq!(label(-1200.0, Semantic::DurationMs), "-1.2s");
    }

    #[test]
    fn tokens_ladder() {
        assert_eq!(label(850.0, Semantic::Tokens), "850");
        assert_eq!(label(1200.0, Semantic::Tokens), "1.2k");
        assert_eq!(label(3_400_000.0, Semantic::Tokens), "3.4M");
    }

    #[test]
    fn delta_signs() {
        assert_eq!(label(3.0, Semantic::Delta), "+3");
        assert_eq!(label(-2.5, Semantic::Delta), "-2.5");
        assert_eq!(label(0.0, Semantic::Delta), "0");
    }

    #[test]
    fn date_from_epoch() {
        // 2026-07-09 00:00:00 UTC = 20643 days × 86400000 = 1783555200000 ms.
        assert_eq!(label(1_783_555_200_000.0, Semantic::Timestamp), "07-09");
        // Epoch day zero.
        assert_eq!(label(0.0, Semantic::Timestamp), "01-01");
    }
}
