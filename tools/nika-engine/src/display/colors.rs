//! Color helpers — formatting, syntax highlighting, sparklines.

use colored::{ColoredString, Colorize};
use unicode_width::UnicodeWidthChar;

/// Format a duration with color based on speed.
///
/// - `< 1ms`: microseconds (green)
/// - `< 1s`: milliseconds (green)
/// - `1-5s`: seconds (yellow)
/// - `> 5s`: seconds or minutes (red)
pub fn duration(secs: f32) -> ColoredString {
    let text = if secs < 0.001 {
        format!("{:.0}\u{00B5}s", secs * 1_000_000.0) // µs
    } else if secs < 1.0 {
        format!("{:.0}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.1}s", secs)
    } else {
        let m = (secs / 60.0).floor() as u32;
        let s = secs % 60.0;
        format!("{}m{:.1}s", m, s)
    };
    if secs < 1.0 {
        text.green()
    } else if secs < 5.0 {
        text.yellow()
    } else {
        text.red()
    }
}

/// Format a token count as a compact string (e.g. `1.2k`, `42k`).
///
/// Returns `String` — callers should use `.as_str().dimmed()` if coloring is needed,
/// since `String` does not implement `Colorize` the same way `&str` does.
pub fn tokens(n: u64) -> String {
    if n < 1000 {
        n.to_string()
    } else if n < 10_000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else if n < 1_000_000 {
        format!("{}k", n / 1000)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

/// Pad a colored string to `width` visible characters.
///
/// ANSI escape sequences are not counted toward width.
#[cfg(test)]
pub(crate) fn pad_colored(cs: &ColoredString, width: usize) -> String {
    let visible_len = stripped_len(&cs.to_string());
    let pad = width.saturating_sub(visible_len);
    format!("{}{}", cs, " ".repeat(pad))
}

/// Count visible terminal columns in a string, ignoring ANSI escape sequences.
///
/// Uses `unicode_width` to correctly count CJK characters and wide emoji as 2 columns.
pub(crate) fn stripped_len(s: &str) -> usize {
    let mut len = 0;
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip ANSI escape: ESC + everything up to first letter
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() || ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            len += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    len
}

/// Render a sparkline bar for `value` relative to `max`.
pub(crate) fn sparkline(value: u64, max: u64) -> ColoredString {
    const CHARS: &[char] = &[
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];
    let ratio = if max == 0 {
        0.0
    } else {
        value as f64 / max as f64
    };
    let idx = (ratio * 7.0).round().min(7.0) as usize;
    let bar: String = (0..8)
        .map(|i| if i <= idx { CHARS[idx] } else { '\u{2591}' })
        .collect();
    bar.blue()
}

/// Render a budget usage bar with percentage.
pub(crate) fn budget_bar(pct: f32, width: usize) -> String {
    let pct = pct.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar = format!("{}{}", "\u{2593}".repeat(filled), "\u{2591}".repeat(empty));
    let colored_bar = if pct < 70.0 {
        bar.green()
    } else if pct < 90.0 {
        bar.yellow()
    } else {
        bar.red()
    };
    let pct_str = format!("{}%", pct.round() as u32);
    let colored_pct = if pct < 70.0 {
        pct_str.green()
    } else if pct < 90.0 {
        pct_str.yellow()
    } else {
        pct_str.red()
    };
    format!("{} {}", colored_bar, colored_pct)
}

/// Format a USD cost with color based on magnitude.
pub fn cost(usd: f64) -> ColoredString {
    if usd < 0.001 {
        format!("${:.4}", usd).dimmed()
    } else if usd < 0.01 {
        format!("${:.3}", usd).green()
    } else if usd < 0.10 {
        format!("${:.2}", usd).yellow()
    } else {
        format!("${:.2}", usd).red().bold()
    }
}

/// Format time-to-first-token with color.
pub(crate) fn ttft(ms: u64) -> ColoredString {
    let text = format!("{}ms", ms);
    if ms < 200 {
        text.green()
    } else if ms < 500 {
        text.yellow()
    } else {
        text.red()
    }
}

/// Find the largest byte index `<= i` that is a valid char boundary.
///
/// Equivalent to `str::floor_char_boundary` (stable since 1.91) but works on
/// our MSRV (1.86).
pub(crate) fn floor_char_boundary(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    let mut pos = i;
    // Walk backward until we find a byte that is NOT a UTF-8 continuation byte
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Syntax-highlight a JSON string with ANSI colors, truncated to `max_chars`.
///
/// Uses a safe char-boundary truncation to avoid splitting multi-byte characters.
pub(crate) fn json_preview(json: &str, max_chars: usize) -> String {
    let truncated = if stripped_len(json) > max_chars {
        let end = floor_char_boundary(json, max_chars);
        format!("{}\u{2026}", &json[..end]) // … ellipsis
    } else {
        json.to_string()
    };

    let mut result = String::with_capacity(truncated.len() * 2);
    let mut in_key = false;
    let mut in_string = false;
    let mut after_colon = false;
    let mut escaped = false;

    for ch in truncated.chars() {
        // Track backslash escaping inside strings
        if in_string && escaped {
            escaped = false;
            result.push(ch);
            continue;
        }
        if in_string && ch == '\\' {
            escaped = true;
            result.push(ch);
            continue;
        }
        match ch {
            '"' if !in_string && !after_colon => {
                in_key = true;
                in_string = true;
                result.push_str("\x1b[34m\""); // blue for keys
            }
            '"' if !in_string && after_colon => {
                in_string = true;
                result.push_str("\x1b[32m\""); // green for string values
            }
            '"' if in_string => {
                result.push('"');
                result.push_str("\x1b[0m");
                in_string = false;
                if in_key {
                    in_key = false;
                }
                after_colon = false;
            }
            ':' if !in_string => {
                after_colon = true;
                result.push_str("\x1b[0m:");
            }
            ',' | '{' | '}' | '[' | ']' if !in_string => {
                after_colon = false;
                result.push_str(&format!("\x1b[0m{}", ch));
            }
            c if !in_string && (c.is_ascii_digit() || c == '.' || c == '-') => {
                result.push_str(&format!("\x1b[33m{}\x1b[0m", c)); // yellow for numbers
            }
            _ => result.push(ch),
        }
    }
    result.push_str("\x1b[0m");
    result
}

/// Render a Markdown preview with basic header highlighting.
pub(crate) fn markdown_preview(md: &str, max_lines: usize) -> Vec<String> {
    md.lines()
        .take(max_lines)
        .map(|line| {
            if line.starts_with('#') {
                format!("{}", line.bold().white())
            } else {
                line.to_string()
            }
        })
        .collect()
}
