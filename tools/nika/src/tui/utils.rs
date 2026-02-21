//! TUI utility functions
//!
//! Shared formatting and helper functions used across TUI modules.

/// Format number with thousands separator
///
/// # Examples
///
/// ```ignore
/// assert_eq!(format_number(1234567), "1,234,567");
/// assert_eq!(format_number(0), "0");
/// assert_eq!(format_number(999), "999");
/// ```
pub fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format number with thousands separator (u64 version)
pub fn format_number_u64(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format number with smart suffixes (K, M, B)
///
/// # Examples
///
/// ```ignore
/// assert_eq!(format_number_compact(500), "500");
/// assert_eq!(format_number_compact(1500), "1.5K");
/// assert_eq!(format_number_compact(1500000), "1.5M");
/// ```
pub fn format_number_compact(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(123), "123");
        assert_eq!(format_number(1234), "1,234");
        assert_eq!(format_number(12345), "12,345");
        assert_eq!(format_number(123456), "123,456");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_format_number_u64() {
        assert_eq!(format_number_u64(0), "0");
        assert_eq!(format_number_u64(1234567890), "1,234,567,890");
    }

    #[test]
    fn test_format_number_compact() {
        assert_eq!(format_number_compact(500), "500");
        assert_eq!(format_number_compact(1000), "1.0K");
        assert_eq!(format_number_compact(1500), "1.5K");
        assert_eq!(format_number_compact(1000000), "1.0M");
        assert_eq!(format_number_compact(1500000), "1.5M");
        assert_eq!(format_number_compact(1000000000), "1.0B");
    }
}
