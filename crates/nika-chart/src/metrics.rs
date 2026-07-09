//! Embedded text metrics (CHT §3bis law 6) — em-relative per-ASCII advance
//! widths, Helvetica/Arial-biased, ×1.12 slack. No font I/O ever (the
//! vl-convert system-font trap). Estimates feed LAYOUT only; viewers may
//! substitute fonts — slack + anchors keep drift cosmetic.

/// Em-relative width for one char (Helvetica-class approximation).
fn char_w(c: char) -> f64 {
    match c {
        'i' | 'l' | 'j' | 'I' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' | '`' => 0.30,
        'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '{' | '}' | ' ' | '/' | '\\' => 0.36,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.89,
        'A'..='Z' => 0.68,
        '+' | '-' | '=' | '<' | '>' | '~' | '^' | '*' => 0.58,
        '$' | '#' | '&' | '?' => 0.60,
        _ => 0.56, // lowercase body + digits (near-tabular in Helvetica-class)
    }
}

const SLACK: f64 = 1.12;

/// Estimated pixel width of `text` at `px` font size (slack included).
pub fn measure(text: &str, px: f64) -> f64 {
    let em: f64 = text.chars().map(char_w).sum();
    em * px * SLACK
}

/// Deterministic ellipsis truncation to fit `max_px` (never overlapping).
#[must_use]
pub fn truncate(text: &str, px: f64, max_px: f64) -> String {
    if measure(text, px) <= max_px {
        return text.to_owned();
    }
    let ell = '…';
    let ell_w = 0.9 * px * SLACK;
    let mut out = String::new();
    let mut w = 0.0;
    for c in text.chars() {
        let cw = char_w(c) * px * SLACK;
        if w + cw + ell_w > max_px {
            break;
        }
        w += cw;
        out.push(c);
    }
    out.push(ell);
    out
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
    fn wide_beats_narrow() {
        assert!(measure("mmm", 12.0) > measure("iii", 12.0));
    }

    #[test]
    fn truncate_fits() {
        let t = truncate("aggregate-metrics-collector", 11.0, 80.0);
        assert!(t.ends_with('…'));
        assert!(measure(&t, 11.0) <= 80.0 + 0.9 * 11.0 * SLACK);
    }

    #[test]
    fn short_text_untouched() {
        assert_eq!(truncate("jq", 11.0, 80.0), "jq");
    }
}
