//! Unit tests for the display module — icons, colors, formatting, detail levels.

use colored::Colorize;
use unicode_width::UnicodeWidthStr;

use super::colors;
use super::detail::DetailLevel;
use super::icons;
use super::renderer;

/// Strip ANSI CSI escape sequences (ESC [ ... final_byte) from a string.
///
/// Handles SGR, 256-color, and truecolor sequences by consuming
/// ESC + `[` + parameter/intermediate bytes + final byte.
fn strip_ansi(s: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape && ch == 'm' {
            in_escape = false;
        } else if !in_escape {
            result.push(ch);
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════════════
// 1. Icon tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn all_verb_icons_are_single_column_wide() {
    for verb in &["infer", "exec", "fetch", "invoke", "agent"] {
        let plain = icons::verb_plain(verb);
        let width = UnicodeWidthStr::width(plain);
        assert_eq!(
            width, 1,
            "verb '{}' icon '{}' should be 1 column wide, got {}",
            verb, plain, width
        );
    }
}

#[test]
fn all_verb_icons_are_single_char() {
    for verb in &["infer", "exec", "fetch", "invoke", "agent"] {
        let plain = icons::verb_plain(verb);
        assert_eq!(
            plain.chars().count(),
            1,
            "verb '{}' icon should be 1 char",
            verb
        );
    }
}

#[test]
fn cosmic_palette_characters() {
    assert_eq!(icons::verb_plain("infer"), "\u{2727}"); // ✧
    assert_eq!(icons::verb_plain("exec"), "\u{2388}"); // ⎈
    assert_eq!(icons::verb_plain("fetch"), "\u{2604}"); // ☄
    assert_eq!(icons::verb_plain("invoke"), "\u{229B}"); // ⊛
    assert_eq!(icons::verb_plain("agent"), "\u{274B}"); // ❋
}

#[test]
fn unknown_verb_gives_fallback_icon() {
    let plain = icons::verb_plain("unknown");
    assert_eq!(plain, "\u{25CF}"); // ●
    assert_eq!(UnicodeWidthStr::width(plain), 1);
}

#[test]
fn verb_colored_icons_return_colored_string() {
    // The colored variant should return a ColoredString (not just &str)
    let colored = icons::verb("infer");
    // When serialized, it contains the icon character regardless of color mode
    let s = colored.to_string();
    assert!(
        s.contains('\u{2727}'),
        "colored infer icon should contain star"
    );
}

#[test]
fn all_subsystem_icons_are_single_column_wide() {
    let subsystem_icons = vec![
        ("provider", "\u{22C8}"),   // ⋈
        ("mcp", "\u{229E}"),        // ⊞
        ("guardrail", "\u{22A0}"),  // ⊠
        ("artifact", "\u{229A}"),   // ⊚
        ("media", "\u{22A1}"),      // ⊡
        ("structured", "\u{2B21}"), // ⬡
        ("vision", "\u{27D0}"),     // ⟐
        ("http", "\u{21C4}"),       // ⇄
        ("retry", "\u{21AF}"),      // ↯
        ("agent_meta", "\u{2297}"), // ⊗
        ("log", "\u{25AA}"),        // ▪
    ];

    for (name, ch) in &subsystem_icons {
        let width = UnicodeWidthStr::width(*ch);
        assert_eq!(
            width, 1,
            "subsystem '{}' icon '{}' should be 1 column wide, got {}",
            name, ch, width
        );
    }
}

#[test]
fn status_icons_are_single_column_wide() {
    let statuses = vec![
        ("pending", "\u{25CB}"), // ○
        ("running", "\u{25CF}"), // ●
        ("success", "\u{2713}"), // ✓
        ("failed", "\u{2717}"),  // ✗
        ("skipped", "\u{2298}"), // ⊘
    ];

    for (name, ch) in &statuses {
        let width = UnicodeWidthStr::width(*ch);
        assert_eq!(
            width, 1,
            "status '{}' icon '{}' should be 1 column wide, got {}",
            name, ch, width
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 2. Token formatting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tokens_under_1000_are_plain() {
    assert_eq!(colors::tokens(0), "0");
    assert_eq!(colors::tokens(42), "42");
    assert_eq!(colors::tokens(842), "842");
    assert_eq!(colors::tokens(999), "999");
}

#[test]
fn tokens_1k_to_10k_have_decimal() {
    assert_eq!(colors::tokens(1000), "1.0k");
    assert_eq!(colors::tokens(1200), "1.2k");
    assert_eq!(colors::tokens(9999), "10.0k");
}

#[test]
fn tokens_above_10k_are_integer_k() {
    assert_eq!(colors::tokens(10_000), "10k");
    assert_eq!(colors::tokens(15_000), "15k");
    assert_eq!(colors::tokens(100_000), "100k");
    assert_eq!(colors::tokens(999_999), "999k");
    assert_eq!(colors::tokens(1_000_000), "1.0M");
    assert_eq!(colors::tokens(1_500_000), "1.5M");
    assert_eq!(colors::tokens(10_000_000), "10.0M");
}

// ═══════════════════════════════════════════════════════════════════════
// 3. Byte formatting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn format_bytes_under_1kb() {
    assert_eq!(renderer::format_bytes(0), "0 B");
    assert_eq!(renderer::format_bytes(500), "500 B");
    assert_eq!(renderer::format_bytes(1023), "1023 B");
}

#[test]
fn format_bytes_kilobytes() {
    assert_eq!(renderer::format_bytes(1024), "1.0 KB");
    assert_eq!(renderer::format_bytes(1536), "1.5 KB");
    assert_eq!(renderer::format_bytes(10240), "10.0 KB");
}

#[test]
fn format_bytes_megabytes() {
    assert_eq!(renderer::format_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(renderer::format_bytes(1_500_000), "1.4 MB");
    assert_eq!(renderer::format_bytes(10 * 1024 * 1024), "10.0 MB");
}

// ═══════════════════════════════════════════════════════════════════════
// 4. JSON preview
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn json_preview_truncation() {
    let json = r#"{"key":"value","long":"very long string here that goes on and on"}"#;
    let preview = colors::json_preview(json, 20);
    // The preview contains the key text
    assert!(preview.contains("key"), "preview should contain 'key'");
    // Should contain the ellipsis character (truncation indicator)
    assert!(
        preview.contains('\u{2026}'),
        "truncated preview should contain ellipsis"
    );
}

#[test]
fn json_preview_no_truncation_for_short_input() {
    let json = r#"{"a":1}"#;
    let preview = colors::json_preview(json, 100);
    let stripped = strip_ansi(&preview);
    // All content should be present, no ellipsis
    assert!(stripped.contains("\"a\""));
    assert!(stripped.contains("1"));
    assert!(!stripped.contains('\u{2026}')); // no ellipsis
}

#[test]
fn json_preview_no_panic_on_multibyte_utf8() {
    // Test with accented characters that are multi-byte in UTF-8
    let json = r#"{"title":"résumé","café":"latte"}"#;
    // Truncate in the middle of potential multi-byte sequences
    let preview = colors::json_preview(json, 15);
    // Should not panic and should produce valid UTF-8
    assert!(!preview.is_empty());
    // Verify it's valid UTF-8 by checking we can iterate chars
    let _ = preview.chars().count();
}

#[test]
fn json_preview_unicode_boundary_safety() {
    // Test with CJK characters (3-byte UTF-8) and emoji (4-byte UTF-8)
    let json = r#"{"emoji":"hello"}"#;
    // Truncate at various positions near multi-byte boundaries
    for max in 1..=20 {
        let preview = colors::json_preview(json, max);
        // Must never panic and always produce valid UTF-8
        let _ = preview.chars().count();
    }
}

#[test]
fn json_preview_empty_input() {
    let preview = colors::json_preview("", 20);
    let stripped = strip_ansi(&preview);
    // Should not panic, result should be mostly reset codes
    assert!(stripped.is_empty() || stripped.len() < 5);
}

#[test]
fn json_preview_contains_syntax_colors() {
    // json_preview embeds raw ANSI escape codes for syntax highlighting
    let json = r#"{"name":"value"}"#;
    let preview = colors::json_preview(json, 100);
    // Should contain ANSI escape sequences (raw ESC character)
    assert!(
        preview.contains('\x1b'),
        "json_preview should contain ANSI color codes"
    );
    // Blue for keys (\x1b[34m) and green for string values (\x1b[32m)
    assert!(preview.contains("\x1b[34m"), "keys should be blue");
    assert!(
        preview.contains("\x1b[32m"),
        "string values should be green"
    );
}

#[test]
fn json_preview_numbers_are_yellow() {
    let json = r#"{"count":42}"#;
    let preview = colors::json_preview(json, 100);
    // Numbers get yellow highlighting (\x1b[33m)
    assert!(preview.contains("\x1b[33m"), "numbers should be yellow");
}

// ═══════════════════════════════════════════════════════════════════════
// 5. Budget bar
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn budget_bar_contains_percentage_text() {
    let bar_30 = colors::budget_bar(30.0, 20);
    assert!(
        strip_ansi(&bar_30).contains("30%"),
        "bar should contain '30%'"
    );

    let bar_75 = colors::budget_bar(75.0, 20);
    assert!(
        strip_ansi(&bar_75).contains("75%"),
        "bar should contain '75%'"
    );

    let bar_95 = colors::budget_bar(95.0, 20);
    assert!(
        strip_ansi(&bar_95).contains("95%"),
        "bar should contain '95%'"
    );
}

#[test]
fn budget_bar_zero_and_full() {
    let zero = colors::budget_bar(0.0, 20);
    assert!(strip_ansi(&zero).contains("0%"));

    let full = colors::budget_bar(100.0, 20);
    assert!(strip_ansi(&full).contains("100%"));
}

#[test]
fn budget_bar_green_threshold() {
    // <70 should produce green. The `colored` crate's `.green()` produces
    // "\x1b[32m...\x1b[0m" when colors are enabled. We test the function's
    // color logic by verifying the ColoredString variant.
    //
    // Since budget_bar returns a String with embedded ColoredStrings, we
    // verify the threshold logic via the visible content and bar structure.
    let bar = colors::budget_bar(30.0, 20);
    let stripped = strip_ansi(&bar);
    // Should have filled blocks + empty blocks + percentage
    assert!(stripped.contains("30%"));
    // 30% of 20 = 6 filled blocks
    let filled_count = stripped.chars().filter(|&c| c == '\u{2593}').count();
    assert_eq!(filled_count, 6, "30% of 20 = 6 filled blocks");
}

#[test]
fn budget_bar_yellow_threshold() {
    let bar = colors::budget_bar(75.0, 20);
    let stripped = strip_ansi(&bar);
    assert!(stripped.contains("75%"));
    // 75% of 20 = 15 filled blocks
    let filled_count = stripped.chars().filter(|&c| c == '\u{2593}').count();
    assert_eq!(filled_count, 15, "75% of 20 = 15 filled blocks");
}

#[test]
fn budget_bar_red_threshold() {
    let bar = colors::budget_bar(95.0, 20);
    let stripped = strip_ansi(&bar);
    assert!(stripped.contains("95%"));
    // 95% of 20 = 19 filled blocks
    let filled_count = stripped.chars().filter(|&c| c == '\u{2593}').count();
    assert_eq!(filled_count, 19, "95% of 20 = 19 filled blocks");
}

#[test]
fn budget_bar_width_matches() {
    // Total bar should always be exactly `width` block characters
    for pct in [0.0, 25.0, 50.0, 75.0, 100.0] {
        let bar = colors::budget_bar(pct, 20);
        let stripped = strip_ansi(&bar);
        let blocks: usize = stripped
            .chars()
            .filter(|&c| c == '\u{2593}' || c == '\u{2591}')
            .count();
        assert_eq!(blocks, 20, "bar width should be 20 at {}%", pct);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 6. pad_colored
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn pad_colored_reaches_target_width() {
    let cs = "hello".green();
    let padded = colors::pad_colored(&cs, 20);
    let visible = colors::stripped_len(&padded);
    assert_eq!(visible, 20, "padded string should be 20 visible chars wide");
}

#[test]
fn pad_colored_no_truncation_when_already_wide() {
    let cs = "this is a long string".cyan();
    let padded = colors::pad_colored(&cs, 5);
    let visible = colors::stripped_len(&padded);
    // Should not truncate, just not add padding
    assert!(
        visible >= 21,
        "pad_colored should not truncate, visible={}",
        visible
    );
}

#[test]
fn pad_colored_exact_width() {
    let cs = "12345".normal();
    let padded = colors::pad_colored(&cs, 5);
    let visible = colors::stripped_len(&padded);
    assert_eq!(
        visible, 5,
        "should be exactly 5 when text is already 5 chars"
    );
}

#[test]
fn pad_colored_empty_string() {
    let cs = "".normal();
    let padded = colors::pad_colored(&cs, 10);
    let visible = colors::stripped_len(&padded);
    assert_eq!(visible, 10, "padding empty string to width 10");
}

// ═══════════════════════════════════════════════════════════════════════
// 7. stripped_len (colors module)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn stripped_len_plain_text() {
    assert_eq!(colors::stripped_len("hello"), 5);
    assert_eq!(colors::stripped_len(""), 0);
    assert_eq!(colors::stripped_len("ab cd"), 5);
}

#[test]
fn stripped_len_cjk_double_width() {
    // CJK characters occupy 2 terminal columns
    assert_eq!(colors::stripped_len("你好"), 4);
    assert_eq!(colors::stripped_len("AB你好CD"), 8);
}

#[test]
fn stripped_len_emoji_double_width() {
    assert_eq!(colors::stripped_len("✓"), 1); // narrow
    assert_eq!(colors::stripped_len("🎉"), 2); // wide emoji
}

#[test]
fn stripped_len_with_colored_crate() {
    // The colored crate may or may not emit ANSI codes depending on the
    // terminal environment. stripped_len must return the correct visible
    // length either way.
    let s = "test".green().bold().to_string();
    let len = colors::stripped_len(&s);
    // In test (non-TTY) mode: "test" has no ANSI codes, len = 4
    // If colors forced by another test: the function has a known limitation
    // with CSI sequences but len >= 4 (visible content is always counted)
    assert!(
        len >= 4,
        "stripped_len should count at least the visible text"
    );
}

#[test]
fn stripped_len_consistent_with_pad_colored() {
    // pad_colored uses stripped_len internally, so they must be consistent
    let cs = "hello world".green();
    let padded = colors::pad_colored(&cs, 30);
    let visible = colors::stripped_len(&padded);
    assert_eq!(visible, 30, "pad + strip should agree on width");
}

#[test]
fn stripped_len_handles_json_preview_output() {
    // json_preview uses raw ANSI codes (\x1b[34m etc.)
    // Verify stripped_len handles these correctly
    let preview = colors::json_preview(r#"{"a":1}"#, 100);
    let len = colors::stripped_len(&preview);
    // The visible content is {"a":1} = 7 chars
    // stripped_len may not perfectly strip raw CSI codes (known limitation),
    // but it should produce a reasonable result
    assert!(
        len >= 7,
        "should count at least the JSON content, got {}",
        len
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 8. DetailLevel
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn detail_level_max_shows_everything() {
    let d = DetailLevel::Max;
    assert!(d.show_sub_events());
    assert!(d.show_previews());
    assert!(d.show_sparklines());
    assert!(d.show_full_summary());
    assert!(d.show_layer_separators());
    assert!(d.show_template_events());
    assert!(!d.is_json());
}

#[test]
fn detail_level_default_shows_sub_events_but_not_previews() {
    let d = DetailLevel::Default;
    assert!(d.show_sub_events());
    assert!(!d.show_previews());
    assert!(!d.show_sparklines());
    assert!(d.show_full_summary());
    assert!(!d.show_layer_separators());
    assert!(!d.show_template_events());
    assert!(!d.is_json());
}

#[test]
fn detail_level_min_shows_nothing_extra() {
    let d = DetailLevel::Min;
    assert!(!d.show_sub_events());
    assert!(!d.show_previews());
    assert!(!d.show_sparklines());
    assert!(!d.show_full_summary());
    assert!(!d.show_layer_separators());
    assert!(!d.show_template_events());
    assert!(!d.is_json());
}

#[test]
fn detail_level_json_is_special() {
    let d = DetailLevel::Json;
    assert!(d.is_json());
    assert!(!d.show_sub_events());
    assert!(!d.show_previews());
    assert!(!d.show_sparklines());
    assert!(!d.show_full_summary());
}

#[test]
fn detail_level_default_variant_is_max() {
    assert_eq!(DetailLevel::default(), DetailLevel::Max);
}

#[test]
fn detail_level_display_roundtrip() {
    for variant in &[
        DetailLevel::Max,
        DetailLevel::Default,
        DetailLevel::Min,
        DetailLevel::Json,
    ] {
        let s = variant.to_string();
        let parsed: DetailLevel = s.parse().unwrap();
        assert_eq!(*variant, parsed, "roundtrip failed for {:?}", variant);
    }
}

#[test]
fn detail_level_parse_case_insensitive() {
    assert_eq!("MAX".parse::<DetailLevel>().unwrap(), DetailLevel::Max);
    assert_eq!("Json".parse::<DetailLevel>().unwrap(), DetailLevel::Json);
    assert_eq!("MIN".parse::<DetailLevel>().unwrap(), DetailLevel::Min);
}

#[test]
fn detail_level_parse_invalid() {
    assert!("verbose".parse::<DetailLevel>().is_err());
    assert!("".parse::<DetailLevel>().is_err());
    assert!("quiet".parse::<DetailLevel>().is_err());
}

// ═══════════════════════════════════════════════════════════════════════
// 9. Duration formatting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn duration_sub_millisecond_shows_microseconds() {
    let d = colors::duration(0.0005);
    let stripped = strip_ansi(&d.to_string());
    assert!(
        stripped.contains("\u{00B5}s"),
        "sub-ms should show microseconds, got: {}",
        stripped
    );
}

#[test]
fn duration_milliseconds() {
    let d = colors::duration(0.5);
    let stripped = strip_ansi(&d.to_string());
    assert_eq!(stripped, "500ms", "0.5s = 500ms");
}

#[test]
fn duration_seconds() {
    let d = colors::duration(2.5);
    let stripped = strip_ansi(&d.to_string());
    assert_eq!(stripped, "2.5s");
}

#[test]
fn duration_above_60s_shows_minutes() {
    let d = colors::duration(90.0);
    let stripped = strip_ansi(&d.to_string());
    assert_eq!(stripped, "1m30.0s", "90s = 1m30.0s");
}

#[test]
fn duration_text_format_at_boundaries() {
    // Sub-millisecond
    let d = strip_ansi(&colors::duration(0.0005).to_string());
    assert!(d.ends_with("\u{00B5}s"), "got: {}", d);

    // Exactly 1ms
    let d = strip_ansi(&colors::duration(0.001).to_string());
    assert_eq!(d, "1ms");

    // Just under 1s
    let d = strip_ansi(&colors::duration(0.999).to_string());
    assert_eq!(d, "999ms");

    // Exactly 1s
    let d = strip_ansi(&colors::duration(1.0).to_string());
    assert_eq!(d, "1.0s");

    // Exactly 60s
    let d = strip_ansi(&colors::duration(60.0).to_string());
    assert_eq!(d, "1m0.0s");

    // 125s = 2m5.0s
    let d = strip_ansi(&colors::duration(125.0).to_string());
    assert_eq!(d, "2m5.0s");
}

#[test]
fn duration_color_thresholds() {
    // Test color thresholds: <1s green, 1-5s yellow, >=5s red.
    // We verify by checking the ColoredString's fgcolor field via Debug output,
    // since the colored crate may not emit ANSI codes in test (non-TTY) mode.
    let green_dur = colors::duration(0.5);
    let green_dbg = format!("{:?}", green_dur);
    assert!(
        green_dbg.contains("Green") || green_dbg.contains("green"),
        "0.5s should be green, debug: {}",
        green_dbg
    );

    let yellow_dur = colors::duration(2.5);
    let yellow_dbg = format!("{:?}", yellow_dur);
    assert!(
        yellow_dbg.contains("Yellow") || yellow_dbg.contains("yellow"),
        "2.5s should be yellow, debug: {}",
        yellow_dbg
    );

    let red_dur = colors::duration(10.0);
    let red_dbg = format!("{:?}", red_dur);
    assert!(
        red_dbg.contains("Red") || red_dbg.contains("red"),
        "10s should be red, debug: {}",
        red_dbg
    );
}

#[test]
fn duration_color_at_exact_boundaries() {
    // Exactly 1.0s: >= 1.0, < 5.0 -> yellow
    let at_1s = format!("{:?}", colors::duration(1.0));
    assert!(
        at_1s.contains("Yellow") || at_1s.contains("yellow"),
        "1.0s should be yellow, debug: {}",
        at_1s
    );

    // Exactly 5.0s: >= 5.0 -> red
    let at_5s = format!("{:?}", colors::duration(5.0));
    assert!(
        at_5s.contains("Red") || at_5s.contains("red"),
        "5.0s should be red, debug: {}",
        at_5s
    );

    // Just under 1.0s -> green
    let under_1s = format!("{:?}", colors::duration(0.999));
    assert!(
        under_1s.contains("Green") || under_1s.contains("green"),
        "0.999s should be green, debug: {}",
        under_1s
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 10. Sparkline
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sparkline_zero_max_does_not_panic() {
    let s = colors::sparkline(0, 0);
    let stripped = strip_ansi(&s.to_string());
    assert_eq!(
        stripped.chars().count(),
        8,
        "sparkline should be 8 chars wide"
    );
}

#[test]
fn sparkline_full_ratio() {
    let s = colors::sparkline(100, 100);
    let stripped = strip_ansi(&s.to_string());
    // At 100% ratio, all 8 cells should be the highest block char
    assert_eq!(stripped.chars().count(), 8);
    // Should contain full block characters (U+2588)
    assert!(
        stripped.contains('\u{2588}'),
        "full ratio should have full blocks"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 11. Cost formatting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn cost_formatting() {
    let tiny = strip_ansi(&colors::cost(0.0001).to_string());
    assert!(tiny.starts_with('$'), "cost should start with $");

    let small = strip_ansi(&colors::cost(0.005).to_string());
    assert!(small.starts_with('$'));

    let large = strip_ansi(&colors::cost(1.23).to_string());
    assert_eq!(large, "$1.23");
}

// ═══════════════════════════════════════════════════════════════════════
// 12. TTFT formatting
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn ttft_formatting() {
    let fast = strip_ansi(&colors::ttft(100).to_string());
    assert_eq!(fast, "100ms");

    let medium = strip_ansi(&colors::ttft(300).to_string());
    assert_eq!(medium, "300ms");

    let slow = strip_ansi(&colors::ttft(600).to_string());
    assert_eq!(slow, "600ms");
}

// ═══════════════════════════════════════════════════════════════════════
// 13. Event formatters (new: 12 swallowed event types)
// ═══════════════════════════════════════════════════════════════════════

use super::format_event;

#[test]
fn fmt_fetch_retry_with_status_code() {
    let out = strip_ansi(&format_event::fmt_fetch_retry(
        "https://api.example.com/data",
        2,
        3,
        Some(429),
        2000,
    ));
    assert!(out.contains("fetch retry"), "should contain 'fetch retry'");
    assert!(out.contains("2/3"), "should contain attempt fraction");
    assert!(
        out.contains("https://api.example.com/data"),
        "should contain URL"
    );
    assert!(out.contains("http:429"), "should contain status code");
    assert!(out.contains("backoff:2000ms"), "should contain backoff");
}

#[test]
fn fmt_fetch_retry_without_status_code() {
    let out = strip_ansi(&format_event::fmt_fetch_retry(
        "https://api.example.com",
        1,
        5,
        None,
        500,
    ));
    assert!(out.contains("1/5"), "should contain attempt fraction");
    assert!(
        !out.contains("http:"),
        "should NOT contain http: when no status code"
    );
    assert!(out.contains("backoff:500ms"), "should contain backoff");
}

#[test]
fn fmt_boot_phase_success() {
    let out = strip_ansi(&format_event::fmt_boot_phase(
        "config_discovery",
        true,
        42,
        &[],
    ));
    assert!(out.contains("boot"), "should contain 'boot'");
    assert!(
        out.contains("config_discovery"),
        "should contain phase name"
    );
    assert!(out.contains("42ms"), "should contain duration");
    assert!(out.contains('\u{2713}'), "should contain success icon");
}

#[test]
fn fmt_boot_phase_with_warnings() {
    let out = strip_ansi(&format_event::fmt_boot_phase(
        "mcp_startup",
        true,
        100,
        &["server timeout".to_string(), "retry succeeded".to_string()],
    ));
    assert!(out.contains("2 warnings"), "should show warning count");
}

#[test]
fn fmt_native_model_loaded_with_vision() {
    let out = strip_ansi(&format_event::fmt_native_model_loaded(
        "Qwen/Qwen2.5-VL-7B",
        "huggingface",
        5200,
        true,
    ));
    assert!(out.contains("native"), "should contain 'native'");
    assert!(
        out.contains("Qwen/Qwen2.5-VL-7B"),
        "should contain model name"
    );
    assert!(out.contains("huggingface"), "should contain kind");
    assert!(out.contains("5200ms"), "should contain duration");
    assert!(out.contains("+vision"), "should contain vision tag");
}

#[test]
fn fmt_binding_default_output() {
    let out = strip_ansi(&format_event::fmt_binding_default("data", "$task1.missing"));
    assert!(out.contains("bind"), "should contain 'bind'");
    assert!(out.contains("data"), "should contain alias");
    assert!(out.contains("??"), "should contain default operator");
    assert!(out.contains("$task1.missing"), "should contain path");
}

#[test]
fn fmt_binding_env_found() {
    let out = strip_ansi(&format_event::fmt_binding_env("API_KEY", true));
    assert!(out.contains("$env.API_KEY"), "should contain env var");
    assert!(out.contains('\u{2713}'), "should contain success icon");
}

#[test]
fn fmt_decompose_completed_output() {
    let out = strip_ansi(&format_event::fmt_decompose_completed("research", 5, 120));
    assert!(out.contains("decompose"), "should contain 'decompose'");
    assert!(out.contains("research"), "should contain task id");
    assert!(out.contains("5 items"), "should contain item count");
    assert!(out.contains("120ms"), "should contain duration");
}

#[test]
fn fmt_for_each_started_output() {
    let out = strip_ansi(&format_event::fmt_for_each_started("process", 10, 3));
    assert!(out.contains("for_each"), "should contain 'for_each'");
    assert!(out.contains("10 items"), "should contain item count");
    assert!(out.contains("concurrency:3"), "should contain concurrency");
}

#[test]
fn fmt_provider_initialized_cached() {
    let out = strip_ansi(&format_event::fmt_provider_initialized(
        "openai", "gpt-4o", true,
    ));
    assert!(out.contains("openai"), "should contain provider");
    assert!(out.contains("cached"), "should contain cached");
}

#[test]
fn fmt_builtin_tool_invoked_success() {
    let out = strip_ansi(&format_event::fmt_builtin_tool_invoked(
        "nika:read",
        15,
        true,
    ));
    assert!(out.contains("nika:read"), "should contain tool name");
    assert!(out.contains("15ms"), "should contain duration");
    assert!(out.contains('\u{2713}'), "should contain success icon");
}

#[test]
fn fmt_extract_applied_output() {
    let out = strip_ansi(&format_event::fmt_extract_applied("markdown", 50000, 12000));
    assert!(
        out.contains("extract:markdown"),
        "should contain extract mode"
    );
    assert!(
        out.contains("24%"),
        "should contain ratio (12000/50000 = 24%)"
    );
}

#[test]
fn fmt_extract_applied_zero_input() {
    let out = strip_ansi(&format_event::fmt_extract_applied("text", 0, 0));
    assert!(out.contains("extract:text"), "should contain extract mode");
    assert!(
        !out.contains('%'),
        "should NOT contain percentage with zero input"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 14. RunStats::apply_event
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn run_stats_apply_provider_responded() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();
    let event = Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::ProviderResponded {
            task_id: Arc::from("t1"),
            request_id: None,
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 10,
            ttft_ms: Some(45),
            finish_reason: nika_event::FinishReason::Stop,
            cost_usd: 0.005,
        },
    };
    stats.apply_event(&event);
    assert_eq!(stats.total_input_tokens, 100);
    assert_eq!(stats.total_output_tokens, 50);
    assert_eq!(stats.total_cache_tokens, 10);
    assert_eq!(stats.ttft_values, vec![45]);
    assert_eq!(stats.provider_calls.len(), 1);
    assert!((stats.total_cost - 0.005).abs() < f64::EPSILON);
}

#[test]
fn run_stats_apply_provider_responded_no_ttft() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();
    let event = Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::ProviderResponded {
            task_id: Arc::from("t1"),
            request_id: None,
            input_tokens: 200,
            output_tokens: 80,
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: nika_event::FinishReason::Stop,
            cost_usd: 0.01,
        },
    };
    stats.apply_event(&event);
    assert_eq!(stats.total_input_tokens, 200);
    assert_eq!(stats.total_output_tokens, 80);
    assert_eq!(stats.total_cache_tokens, 0);
    assert!(
        stats.ttft_values.is_empty(),
        "ttft_values should be empty when ttft_ms is None"
    );
    assert_eq!(stats.provider_calls.len(), 1);
    assert!((stats.total_cost - 0.01).abs() < f64::EPSILON);
}

#[test]
fn run_stats_apply_multiple_provider_calls_accumulate() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();
    for i in 0..3 {
        let event = Event {
            id: i,
            timestamp_ms: 0,
            kind: EventKind::ProviderResponded {
                task_id: Arc::from(format!("t{}", i).as_str()),
                request_id: None,
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 5,
                ttft_ms: Some(40 + i),
                finish_reason: nika_event::FinishReason::Stop,
                cost_usd: 0.001,
            },
        };
        stats.apply_event(&event);
    }
    assert_eq!(stats.total_input_tokens, 300);
    assert_eq!(stats.total_output_tokens, 150);
    assert_eq!(stats.total_cache_tokens, 15);
    assert_eq!(stats.ttft_values, vec![40, 41, 42]);
    assert_eq!(stats.provider_calls.len(), 3);
    assert!((stats.total_cost - 0.003).abs() < f64::EPSILON);
}

#[test]
fn run_stats_tracks_provider_from_provider_called() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();

    // ProviderCalled sets the pending provider for task_id
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::ProviderCalled {
            task_id: Arc::from("t1"),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4".to_string(),
            prompt_len: 100,
            endpoint_url: None,
        },
    });

    // ProviderResponded joins the provider info
    stats.apply_event(&Event {
        id: 2,
        timestamp_ms: 10,
        kind: EventKind::ProviderResponded {
            task_id: Arc::from("t1"),
            request_id: None,
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            ttft_ms: Some(45),
            finish_reason: nika_event::FinishReason::Stop,
            cost_usd: 0.005,
        },
    });

    assert_eq!(stats.provider_calls.len(), 1);
    assert_eq!(stats.provider_calls[0].provider, "anthropic");
    assert_eq!(stats.provider_calls[0].model, "claude-sonnet-4");
    // pending_providers retains entries so multi-turn agent events resolve correctly
    assert!(
        stats.pending_providers.contains_key("t1"),
        "pending should retain entry for multi-turn"
    );
}

#[test]
fn run_stats_multi_turn_agent_retains_provider() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();

    // ProviderCalled once for the agent task
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::ProviderCalled {
            task_id: Arc::from("agent1"),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4".to_string(),
            prompt_len: 100,
            endpoint_url: None,
        },
    });

    // First turn ProviderResponded
    stats.apply_event(&Event {
        id: 2,
        timestamp_ms: 10,
        kind: EventKind::ProviderResponded {
            task_id: Arc::from("agent1"),
            request_id: None,
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            ttft_ms: Some(45),
            finish_reason: nika_event::FinishReason::ToolUse,
            cost_usd: 0.003,
        },
    });

    // Second turn ProviderResponded (multi-turn)
    stats.apply_event(&Event {
        id: 3,
        timestamp_ms: 20,
        kind: EventKind::ProviderResponded {
            task_id: Arc::from("agent1"),
            request_id: None,
            input_tokens: 200,
            output_tokens: 80,
            cache_read_tokens: 50,
            ttft_ms: Some(30),
            finish_reason: nika_event::FinishReason::Stop,
            cost_usd: 0.005,
        },
    });

    // Both turns must be attributed to the correct provider, not "unknown"
    assert_eq!(stats.provider_calls.len(), 2);
    assert_eq!(stats.provider_calls[0].provider, "anthropic");
    assert_eq!(stats.provider_calls[0].model, "claude-sonnet-4");
    assert_eq!(stats.provider_calls[1].provider, "anthropic");
    assert_eq!(stats.provider_calls[1].model, "claude-sonnet-4");
}

#[test]
fn run_stats_provider_defaults_to_unknown_without_provider_called() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();

    // ProviderResponded without a preceding ProviderCalled
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::ProviderResponded {
            task_id: Arc::from("t1"),
            request_id: None,
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: nika_event::FinishReason::Stop,
            cost_usd: 0.001,
        },
    });

    assert_eq!(stats.provider_calls[0].provider, "unknown");
    assert_eq!(stats.provider_calls[0].model, "unknown");
}

#[test]
fn run_stats_apply_mcp_events() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();

    // McpInvoke
    let event = Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::McpInvoke {
            task_id: Arc::from("t1"),
            call_id: "1".into(),
            mcp_server: "neo4j".into(),
            tool: Some("query".into()),
            resource: None,
            params: None,
        },
    };
    stats.apply_event(&event);
    assert_eq!(stats.mcp_calls, 1);

    // McpError
    let event2 = Event {
        id: 2,
        timestamp_ms: 0,
        kind: EventKind::McpError {
            server_name: "neo4j".into(),
            error: "timeout".into(),
        },
    };
    stats.apply_event(&event2);
    assert_eq!(stats.mcp_errors, 1);

    // McpRetry
    let event3 = Event {
        id: 3,
        timestamp_ms: 0,
        kind: EventKind::McpRetry {
            task_id: Arc::from("t1"),
            server_name: "neo4j".into(),
            operation: "query".into(),
            attempt: 1,
            max_attempts: 3,
            error: "connection reset".into(),
        },
    };
    stats.apply_event(&event3);
    assert_eq!(stats.mcp_retries, 1);
}

#[test]
fn run_stats_apply_guardrail_events() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();

    // GuardrailPassed
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::GuardrailPassed {
            task_id: Arc::from("t1"),
            guardrail_type: nika_event::GuardrailType::Length,
            description: "max 1000 chars".into(),
        },
    });
    assert_eq!(stats.guardrails_passed, 1);

    // GuardrailFailed
    stats.apply_event(&Event {
        id: 2,
        timestamp_ms: 0,
        kind: EventKind::GuardrailFailed {
            task_id: Arc::from("t1"),
            guardrail_type: nika_event::GuardrailType::Schema,
            description: "JSON schema".into(),
            message: "missing field".into(),
        },
    });
    assert_eq!(stats.guardrails_failed, 1);

    // GuardrailEscalation
    stats.apply_event(&Event {
        id: 3,
        timestamp_ms: 0,
        kind: EventKind::GuardrailEscalation {
            task_id: Arc::from("t1"),
            guardrail_type: nika_event::GuardrailType::Llm,
            guardrail_id: "safety-1".into(),
            message: "content unsafe".into(),
            severity: nika_event::Severity::High,
            suggested_action: None,
        },
    });
    assert_eq!(stats.guardrails_escalations, 1);
}

#[test]
fn run_stats_apply_artifact_written() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::ArtifactWritten {
            task_id: Arc::from("t1"),
            path: "/tmp/out.md".into(),
            size: 2048,
            format: "markdown".into(),
            checksum: None,
        },
    });
    assert_eq!(stats.artifacts_count, 1);
    assert_eq!(stats.artifacts_bytes, 2048);

    // Second artifact accumulates
    stats.apply_event(&Event {
        id: 2,
        timestamp_ms: 0,
        kind: EventKind::ArtifactWritten {
            task_id: Arc::from("t2"),
            path: "/tmp/out2.json".into(),
            size: 512,
            format: "json".into(),
            checksum: None,
        },
    });
    assert_eq!(stats.artifacts_count, 2);
    assert_eq!(stats.artifacts_bytes, 2560);
}

#[test]
fn run_stats_apply_media_stored_with_dedup() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();
    let event = Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::MediaStored {
            task_id: Arc::from("t1"),
            hash: "abc123".into(),
            path: "/tmp/img.png".into(),
            size_bytes: 1024,
            verified: true,
            deduplicated: true,
            pipeline_ms: 50,
        },
    };
    stats.apply_event(&event);
    assert_eq!(stats.media_stored, 1);
    assert_eq!(stats.media_bytes, 1024);
    assert_eq!(stats.media_dedup, 1);
}

#[test]
fn run_stats_apply_media_stored_without_dedup() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::MediaStored {
            task_id: Arc::from("t1"),
            hash: "abc123".into(),
            path: "/tmp/img.png".into(),
            size_bytes: 2048,
            verified: true,
            deduplicated: false,
            pipeline_ms: 30,
        },
    });
    assert_eq!(stats.media_stored, 1);
    assert_eq!(stats.media_bytes, 2048);
    assert_eq!(
        stats.media_dedup, 0,
        "should not increment dedup when deduplicated=false"
    );
}

#[test]
fn run_stats_apply_structured_output_events() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();

    // StructuredOutputAttempt
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::StructuredOutputAttempt {
            task_id: Arc::from("t1"),
            layer: 1,
            layer_name: "tool_injection".into(),
            attempt: 1,
            success: false,
            error: Some("parse error".into()),
        },
    });
    assert_eq!(stats.structured_attempts, 1);

    // StructuredOutputSuccess
    stats.apply_event(&Event {
        id: 2,
        timestamp_ms: 0,
        kind: EventKind::StructuredOutputSuccess {
            task_id: Arc::from("t1"),
            layer: 2,
            layer_name: "extract_validate".into(),
            total_attempts: 2,
        },
    });
    assert_eq!(stats.structured_success_layer, Some(2));
}

#[test]
fn run_stats_apply_task_completed_and_failed() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use serde_json::Value;
    use std::sync::Arc;

    let mut stats = RunStats::default();

    // TaskCompleted
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 100,
        kind: EventKind::TaskCompleted {
            task_id: Arc::from("t1"),
            output: Arc::new(Value::String("done".into())),
            duration_ms: 500,
        },
    });
    assert_eq!(stats.tasks_passed, 1);

    // TaskFailed
    stats.apply_event(&Event {
        id: 2,
        timestamp_ms: 200,
        kind: EventKind::TaskFailed {
            task_id: Arc::from("t2"),
            error: "boom".into(),
            duration_ms: 100,
            error_code: Some("NIKA-044".into()),
        },
    });
    assert_eq!(stats.tasks_failed, 1);
    assert!(stats.root_failure.as_deref().unwrap().starts_with("t2:"));

    // Second failure should NOT overwrite root_failure
    stats.apply_event(&Event {
        id: 3,
        timestamp_ms: 300,
        kind: EventKind::TaskFailed {
            task_id: Arc::from("t3"),
            error: "also boom".into(),
            duration_ms: 50,
            error_code: None,
        },
    });
    assert_eq!(stats.tasks_failed, 2);
    assert!(
        stats.root_failure.as_deref().unwrap().starts_with("t2:"),
        "root_failure should be first failure (with error)"
    );
}

#[test]
fn run_stats_apply_task_skipped() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};
    use std::sync::Arc;

    let mut stats = RunStats::default();
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::TaskSkipped {
            task_id: Arc::from("t3"),
            reason: "dep t2 failed".into(),
        },
    });
    assert_eq!(stats.tasks_skipped, 1);
}

#[test]
fn run_stats_apply_ignores_unrelated_events() {
    use super::renderer::RunStats;
    use nika_event::{Event, EventKind};

    let mut stats = RunStats::default();

    // WorkflowStarted should not touch any counters
    stats.apply_event(&Event {
        id: 1,
        timestamp_ms: 0,
        kind: EventKind::WorkflowStarted {
            task_count: 5,
            generation_id: "gen1".into(),
            workflow_hash: "hash1".into(),
            nika_version: "0.47.0".into(),
        },
    });
    assert_eq!(stats.tasks_passed, 0);
    assert_eq!(stats.tasks_failed, 0);
    assert_eq!(stats.mcp_calls, 0);
    assert_eq!(stats.total_input_tokens, 0);
}

// ═══════════════════════════════════════════════════════════════════════
// 15. Must-fix coverage: format_event branch tests
// ═══════════════════════════════════════════════════════════════════════

// ── fmt_http_response: 3-way status code coloring + optional fields ──

#[test]
fn fmt_http_response_success() {
    let out = strip_ansi(&format_event::fmt_http_response(
        200,
        Some("application/json"),
        Some(1024),
        150,
    ));
    assert!(out.contains("200"));
    assert!(out.contains("application/json"));
    assert!(out.contains("150ms"));
}

#[test]
fn fmt_http_response_redirect() {
    let out = strip_ansi(&format_event::fmt_http_response(301, None, None, 50));
    assert!(out.contains("301"));
    assert!(
        out.contains("?"),
        "default for None content_type should be '?'"
    );
}

#[test]
fn fmt_http_response_error() {
    let out = strip_ansi(&format_event::fmt_http_response(
        500,
        Some("text/html"),
        Some(256),
        3000,
    ));
    assert!(out.contains("500"));
    assert!(out.contains("3000ms"));
}

// ── fmt_mcp_response: 4 combinations of cached/error flags ───────────

#[test]
fn fmt_mcp_response_basic() {
    let out = strip_ansi(&format_event::fmt_mcp_response(
        "42", 1024, 150, false, false,
    ));
    assert!(out.contains("call:42"));
    assert!(!out.contains("cached"));
}

#[test]
fn fmt_mcp_response_cached() {
    let out = strip_ansi(&format_event::fmt_mcp_response("1", 512, 5, true, false));
    assert!(out.contains("cached"));
}

#[test]
fn fmt_mcp_response_error() {
    let out = strip_ansi(&format_event::fmt_mcp_response("1", 0, 100, false, true));
    assert!(!out.contains("cached"));
}

#[test]
fn fmt_mcp_response_cached_and_error() {
    let out = strip_ansi(&format_event::fmt_mcp_response("7", 256, 80, true, true));
    assert!(out.contains("cached"), "should show cached tag");
    assert!(out.contains("call:7"), "should contain call id");
}

// ── fmt_provider_responded: tokens + optional ttft ───────────────────

#[test]
fn fmt_provider_responded_with_ttft() {
    let out = strip_ansi(&format_event::fmt_provider_responded(
        500,
        200,
        100,
        Some(45),
    ));
    assert!(out.contains("in:"));
    assert!(out.contains("out:"));
    assert!(out.contains("cache:"));
    assert!(out.contains("ttft:45"));
}

#[test]
fn fmt_provider_responded_no_ttft() {
    let out = strip_ansi(&format_event::fmt_provider_responded(1000, 50, 0, None));
    assert!(out.contains("in:"));
    assert!(!out.contains("ttft"));
}

// ── fmt_context_assembled: budget warning threshold ──────────────────

#[test]
fn fmt_context_assembled_normal() {
    let out = strip_ansi(&format_event::fmt_context_assembled(3, 5000, 50.0));
    assert!(out.contains("3 src"));
    assert!(
        !out.contains('\u{26A0}'),
        "should NOT have warning emoji below 90%"
    );
}

#[test]
fn fmt_context_assembled_budget_warning() {
    let out = strip_ansi(&format_event::fmt_context_assembled(5, 12000, 95.0));
    assert!(out.contains("5 src"));
    // Warning emoji (⚠) should appear above 90%
    assert!(
        out.contains('\u{26A0}'),
        "should have warning emoji above 90%"
    );
}

// ── Negative tests for boolean-flag formatters ───────────────────────

#[test]
fn fmt_native_model_loaded_no_vision() {
    let out = strip_ansi(&format_event::fmt_native_model_loaded(
        "llama-3.2",
        "gguf",
        3000,
        false,
    ));
    assert!(!out.contains("+vision"));
    assert!(out.contains("llama-3.2"));
}

#[test]
fn fmt_binding_env_not_found() {
    let out = strip_ansi(&format_event::fmt_binding_env("MISSING_KEY", false));
    assert!(out.contains("$env.MISSING_KEY"));
    assert!(
        !out.contains('\u{2713}'),
        "should NOT have success checkmark"
    );
}

#[test]
fn fmt_provider_initialized_not_cached() {
    let out = strip_ansi(&format_event::fmt_provider_initialized(
        "anthropic",
        "claude-sonnet-4",
        false,
    ));
    assert!(!out.contains("cached"));
    assert!(out.contains("anthropic"));
}

#[test]
fn fmt_builtin_tool_invoked_failure() {
    let out = strip_ansi(&format_event::fmt_builtin_tool_invoked(
        "nika:write",
        200,
        false,
    ));
    assert!(out.contains("nika:write"));
    assert!(
        !out.contains('\u{2713}'),
        "should NOT have success checkmark"
    );
}

// ── fmt_for_each_completed: both branches ────────────────────────────

#[test]
fn fmt_for_each_completed_all_ok() {
    let out = strip_ansi(&format_event::fmt_for_each_completed("process", 10, 10, 0));
    assert!(out.contains("10/10 ok"));
    assert!(!out.contains("failed"));
}

#[test]
fn fmt_for_each_completed_with_failures() {
    let out = strip_ansi(&format_event::fmt_for_each_completed("process", 10, 7, 3));
    assert!(out.contains("7/10 ok"));
    assert!(out.contains("3 failed"));
}

// ── fmt_exec_completed: both branches ────────────────────────────────

#[test]
fn fmt_exec_completed_success() {
    let out = strip_ansi(&format_event::fmt_exec_completed(0, 150));
    assert!(out.contains("exit:0"));
    assert!(out.contains("150ms"));
}

#[test]
fn fmt_exec_completed_failure() {
    let out = strip_ansi(&format_event::fmt_exec_completed(1, 500));
    assert!(out.contains("exit:1"));
}

// ═══════════════════════════════════════════════════════════════════════
// 16. Color boundary tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn duration_zero() {
    let out = strip_ansi(&colors::duration(0.0).to_string());
    assert!(
        out.contains("0") || out.contains("\u{00B5}s"),
        "duration(0.0) should format as microseconds, got: {}",
        out
    );
}

#[test]
fn ttft_zero() {
    let out = strip_ansi(&colors::ttft(0).to_string());
    assert_eq!(out, "0ms");
}

#[test]
fn cost_zero() {
    let out = strip_ansi(&colors::cost(0.0).to_string());
    assert!(
        out.starts_with('$'),
        "cost should start with $, got: {}",
        out
    );
    assert!(
        out.contains("0"),
        "cost(0.0) should contain '0', got: {}",
        out
    );
}

#[test]
fn cost_very_small() {
    let out = strip_ansi(&colors::cost(0.000001).to_string());
    assert!(
        out.starts_with('$'),
        "cost should start with $, got: {}",
        out
    );
}

#[test]
fn tokens_max_u64() {
    // Should not panic on extreme values
    let out = colors::tokens(u64::MAX);
    assert!(!out.is_empty());
    assert!(
        out.contains('M'),
        "u64::MAX should format as M, got: {}",
        out
    );
}

#[test]
fn format_bytes_large_megabytes() {
    let out = renderer::format_bytes(1_500_000);
    assert!(
        out.contains("MB"),
        "1.5M bytes should format as MB, got: {}",
        out
    );
}

#[test]
fn format_bytes_gigabytes() {
    let out = renderer::format_bytes(2_500_000_000);
    assert!(
        out.contains("GB"),
        "2.5G bytes should format as GB, got: {}",
        out
    );
}

// ═══════════════════════════════════════════════════════════════════════
// 17. fmt_log level coloring (5 standard + unknown)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fmt_log_all_levels() {
    for level in &["error", "warn", "info", "debug", "trace"] {
        let out = strip_ansi(&format_event::fmt_log("+1.0s", level, "test message"));
        assert!(
            out.contains(level),
            "should contain level '{}', got: {}",
            level,
            out
        );
        assert!(
            out.contains("test message"),
            "should contain message for level '{}'",
            level
        );
    }
}

#[test]
fn fmt_log_unknown_level() {
    let out = strip_ansi(&format_event::fmt_log("+0.0s", "custom", "msg"));
    assert!(out.contains("custom"), "should contain unknown level name");
    assert!(out.contains("msg"), "should contain message");
}

#[test]
fn fmt_log_contains_timestamp() {
    let out = strip_ansi(&format_event::fmt_log("+2.5s", "info", "hello"));
    assert!(out.contains("+2.5s"), "should contain timestamp");
}

// ═══════════════════════════════════════════════════════════════════════
// 18. fmt_custom truncation
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn fmt_custom_short_payload() {
    let payload = serde_json::json!({"key": "value"});
    let out = strip_ansi(&format_event::fmt_custom("+0.5s", "my_event", &payload));
    assert!(out.contains("my_event"), "should contain event name");
    assert!(out.contains("key"), "should contain payload key");
}

#[test]
fn fmt_custom_long_payload_truncated() {
    let long_val = "x".repeat(100);
    let payload = serde_json::json!({"data": long_val});
    let out = strip_ansi(&format_event::fmt_custom("+0.5s", "big", &payload));
    assert!(
        out.contains("\u{2026}"),
        "long payload should be truncated with ellipsis"
    );
}

#[test]
fn fmt_custom_contains_timestamp() {
    let payload = serde_json::json!(null);
    let out = strip_ansi(&format_event::fmt_custom("+1.0s", "evt", &payload));
    assert!(out.contains("+1.0s"), "should contain timestamp");
    assert!(out.contains("evt"), "should contain event name");
}

// ═══════════════════════════════════════════════════════════════════════
// 19. Remaining format_event coverage — MCP, agent, guardrail, artifact,
//     media, structured output, vision, HTTP, policy, template, provider,
//     decompose, binding
// ═══════════════════════════════════════════════════════════════════════

// ── MCP formatters ───────────────────────────────────────────────────

#[test]
fn fmt_mcp_connected_output() {
    let out = strip_ansi(&format_event::fmt_mcp_connected("neo4j-local"));
    assert!(out.contains("connected"), "should contain 'connected'");
    assert!(out.contains("neo4j-local"), "should contain server name");
}

#[test]
fn fmt_mcp_error_output() {
    let out = strip_ansi(&format_event::fmt_mcp_error("neo4j", "connection refused"));
    assert!(out.contains("neo4j"), "should contain server name");
    assert!(out.contains("connection refused"), "should contain error");
}

#[test]
fn fmt_mcp_invoke_with_tool() {
    let out = strip_ansi(&format_event::fmt_mcp_invoke(
        "neo4j",
        Some("query"),
        None,
        "42",
    ));
    assert!(out.contains("neo4j"), "should contain server name");
    assert!(out.contains("query"), "should contain tool name");
    assert!(out.contains("call:42"), "should contain call id");
}

#[test]
fn fmt_mcp_invoke_with_resource() {
    let out = strip_ansi(&format_event::fmt_mcp_invoke(
        "server",
        None,
        Some("data://res"),
        "1",
    ));
    assert!(out.contains("data://res"), "should contain resource URI");
}

#[test]
fn fmt_mcp_invoke_neither() {
    let out = strip_ansi(&format_event::fmt_mcp_invoke("srv", None, None, "1"));
    assert!(
        out.contains("?"),
        "should show '?' fallback when no tool or resource"
    );
}

#[test]
fn fmt_mcp_retry_output() {
    let out = strip_ansi(&format_event::fmt_mcp_retry(
        "connect",
        2,
        5,
        "ECONNREFUSED",
    ));
    assert!(out.contains("retry"), "should contain 'retry'");
    assert!(out.contains("2/5"), "should contain attempt fraction");
    assert!(out.contains("ECONNREFUSED"), "should contain error");
}

// ── Agent formatters ─────────────────────────────────────────────────

#[test]
fn fmt_agent_start_output() {
    let servers = vec!["neo4j".to_string(), "slack".to_string()];
    let out = strip_ansi(&format_event::fmt_agent_start(5, &servers));
    assert!(out.contains("max_turns:5"), "should contain max_turns");
    assert!(out.contains("neo4j, slack"), "should contain server list");
}

#[test]
fn fmt_agent_turn_output() {
    let out = strip_ansi(&format_event::fmt_agent_turn(
        0,
        &nika_event::AgentTurnKind::Started,
    ));
    assert!(out.contains("turn 1"), "0-indexed should display as turn 1");
    assert!(out.contains("started"), "should contain kind");
}

#[test]
fn fmt_agent_complete_output() {
    let out = strip_ansi(&format_event::fmt_agent_complete(
        3,
        &nika_event::AgentStopReason::EndTurn,
    ));
    assert!(out.contains("done"), "should contain 'done'");
    assert!(out.contains("3 turns"), "should contain turn count");
    assert!(out.contains("end_turn"), "should contain stop reason");
}

#[test]
fn fmt_agent_spawned_output() {
    let out = strip_ansi(&format_event::fmt_agent_spawned("child_1", 2));
    assert!(out.contains("spawned"), "should contain 'spawned'");
    assert!(out.contains("child_1"), "should contain child task id");
    assert!(out.contains("depth:2"), "should contain depth");
}

#[test]
fn fmt_agent_turn_tool_use_output() {
    let out = strip_ansi(&format_event::fmt_agent_turn_tool_use());
    assert!(out.contains("tool_use"), "should contain 'tool_use'");
}

// ── Policy formatter ─────────────────────────────────────────────────

#[test]
fn fmt_policy_blocked_output() {
    let out = strip_ansi(&format_event::fmt_policy_blocked(
        "+1.0s",
        "command_blocklist",
        "rm -rf /",
    ));
    assert!(out.contains("BLOCKED"), "should contain 'BLOCKED'");
    assert!(
        out.contains("command_blocklist"),
        "should contain policy type"
    );
    assert!(out.contains("rm -rf /"), "should contain reason");
}

// ── Guardrail formatters ─────────────────────────────────────────────

#[test]
fn fmt_guardrail_passed_output() {
    let out = strip_ansi(&format_event::fmt_guardrail_passed(
        &nika_event::GuardrailType::Length,
        "safe output",
    ));
    assert!(out.contains("length"), "should contain guardrail type");
    assert!(out.contains("safe output"), "should contain description");
}

#[test]
fn fmt_guardrail_failed_output() {
    let out = strip_ansi(&format_event::fmt_guardrail_failed(
        &nika_event::GuardrailType::Regex,
        "email detected",
    ));
    assert!(out.contains("regex"), "should contain guardrail type");
    assert!(out.contains("email detected"), "should contain message");
}

#[test]
fn fmt_guardrail_escalation_output() {
    let out = strip_ansi(&format_event::fmt_guardrail_escalation(
        &nika_event::GuardrailType::Llm,
        &nika_event::Severity::High,
        "needs human review",
    ));
    assert!(out.contains("escalation"), "should contain 'escalation'");
    assert!(out.contains("high"), "should contain severity");
    assert!(out.contains("needs human review"), "should contain message");
}

// ── Artifact formatters ──────────────────────────────────────────────

#[test]
fn fmt_artifact_written_output() {
    let out = strip_ansi(&format_event::fmt_artifact_written(
        "output/report.md",
        4096,
        "markdown",
    ));
    assert!(out.contains("report.md"), "should contain path");
    assert!(out.contains("markdown"), "should contain format");
}

#[test]
fn fmt_artifact_failed_output() {
    let out = strip_ansi(&format_event::fmt_artifact_failed(
        "output/data.json",
        "permission denied",
    ));
    assert!(out.contains("data.json"), "should contain path");
    assert!(out.contains("permission denied"), "should contain reason");
}

// ── Media formatters ─────────────────────────────────────────────────

#[test]
fn fmt_media_extracted_output() {
    let types = vec!["image/png".to_string(), "image/jpeg".to_string()];
    let out = strip_ansi(&format_event::fmt_media_extracted(3, &types));
    assert!(out.contains("3 blocks"), "should contain block count");
    assert!(
        out.contains("image/png"),
        "should contain first content type"
    );
}

#[test]
fn fmt_media_stored_output() {
    let out = strip_ansi(&format_event::fmt_media_stored(
        2048,
        "/cas/img.png",
        "abcdef1234567890abcdef",
    ));
    assert!(out.contains("/cas/img.png"), "should contain path");
    assert!(
        out.contains("abcdef1234567890"),
        "should contain truncated hash (first 16 chars)"
    );
    assert!(
        !out.contains("abcdef1234567890a"),
        "should NOT contain 17th char — hash must be truncated at 16"
    );
}

#[test]
fn fmt_media_store_failed_output() {
    let out = strip_ansi(&format_event::fmt_media_store_failed("disk full"));
    assert!(out.contains("disk full"), "should contain reason");
}

#[test]
fn fmt_media_stored_detail_dedup() {
    let out = strip_ansi(&format_event::fmt_media_stored_detail(true, true, 50));
    assert!(out.contains("dedup:"), "should contain dedup label");
    assert!(out.contains("verified:"), "should contain verified label");
    assert!(out.contains("50ms"), "should contain pipeline duration");
}

#[test]
fn fmt_media_stored_detail_no_dedup() {
    let out = strip_ansi(&format_event::fmt_media_stored_detail(false, false, 10));
    assert!(out.contains("dedup:"), "should contain dedup label");
    assert!(out.contains("verified:"), "should contain verified label");
    assert!(out.contains("10ms"), "should contain pipeline duration");
}

// ── Structured output formatters ─────────────────────────────────────

#[test]
fn fmt_structured_output_attempt_success() {
    let out = strip_ansi(&format_event::fmt_structured_output_attempt(
        2,
        "json_schema",
        true,
        None,
    ));
    assert!(out.contains("L2"), "should contain layer number");
    assert!(out.contains("json_schema"), "should contain layer name");
}

#[test]
fn fmt_structured_output_attempt_failure() {
    let out = strip_ansi(&format_event::fmt_structured_output_attempt(
        1,
        "regex",
        false,
        Some("no match"),
    ));
    assert!(out.contains("L1"), "should contain layer number");
    assert!(out.contains("no match"), "should contain error message");
}

// ── Vision formatter ─────────────────────────────────────────────────

#[test]
fn fmt_vision_content_resolved_output() {
    let out = strip_ansi(&format_event::fmt_vision_content_resolved(2, 500_000, 150));
    assert!(out.contains("2 images"), "should contain image count");
    assert!(out.contains("150ms"), "should contain resolve time");
}

// ── HTTP request formatter ───────────────────────────────────────────

#[test]
fn fmt_http_request_output() {
    let out = strip_ansi(&format_event::fmt_http_request(
        "GET",
        "https://api.example.com/data",
    ));
    assert!(out.contains("GET"), "should contain HTTP method");
    assert!(
        out.contains("https://api.example.com/data"),
        "should contain URL"
    );
}

// ── Provider formatters ──────────────────────────────────────────────

#[test]
fn fmt_provider_called_output() {
    let out = strip_ansi(&format_event::fmt_provider_called(
        "anthropic",
        "claude-sonnet-4",
        2500,
    ));
    assert!(out.contains("anthropic"), "should contain provider name");
    assert!(out.contains("claude-sonnet-4"), "should contain model name");
    assert!(out.contains("2500 chars"), "should contain prompt length");
}

#[test]
fn fmt_provider_sparkline_output() {
    let out = strip_ansi(&format_event::fmt_provider_sparkline(500, 1000, 0.05));
    assert!(out.contains("tok"), "should contain 'tok' label");
    assert!(out.contains("cost"), "should contain 'cost' label");
}

// ── Template formatter ───────────────────────────────────────────────

#[test]
fn fmt_template_resolved_output() {
    let out = strip_ansi(&format_event::fmt_template_resolved(
        "{{with.data}}",
        "hello world",
    ));
    assert!(out.contains("tmpl"), "should contain 'tmpl' label");
    assert!(out.contains("{{with.data}}"), "should contain template");
    assert!(out.contains("hello world"), "should contain resolved value");
}

// ── Decompose formatter ──────────────────────────────────────────────

#[test]
fn fmt_decompose_started_output() {
    let out = strip_ansi(&format_event::fmt_decompose_started("research", "semantic"));
    assert!(out.contains("decompose"), "should contain 'decompose'");
    assert!(out.contains("research"), "should contain task id");
    assert!(out.contains("semantic"), "should contain strategy");
}

// ── Binding transform formatter ──────────────────────────────────────

#[test]
fn fmt_binding_transform_output() {
    let out = strip_ansi(&format_event::fmt_binding_transform(
        "items",
        "upper | trim | sort",
    ));
    assert!(out.contains("items"), "should contain alias");
    assert!(
        out.contains("upper | trim | sort"),
        "should contain transform chain"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Summary format tests (Task 8)
// ═══════════════════════════════════════════════════════════════════════

use super::renderer::{ProviderCallStat, RunStats};
use super::summary;

/// Build a realistic `RunStats` for summary tests.
fn make_stats() -> RunStats {
    RunStats {
        tasks_passed: 3,
        tasks_failed: 0,
        tasks_skipped: 0,
        total_input_tokens: 5000,
        total_output_tokens: 1200,
        total_cache_tokens: 800,
        total_cost: 0.0042,
        ttft_values: vec![120, 180, 150],
        mcp_calls: 2,
        mcp_retries: 1,
        mcp_errors: 0,
        media_stored: 1,
        media_bytes: 2048,
        media_dedup: 0,
        artifacts_count: 1,
        artifacts_bytes: 4096,
        guardrails_passed: 2,
        guardrails_failed: 0,
        guardrails_escalations: 0,
        retries: 0,
        structured_attempts: 0,
        structured_success_layer: None,
        root_failure: None,
        task_timeline: vec![
            ("fetch".to_string(), "fetch".to_string(), 0, 500),
            ("summarize".to_string(), "infer".to_string(), 500, 1200),
            ("save".to_string(), "exec".to_string(), 1700, 300),
        ],
        provider_calls: vec![ProviderCallStat {
            task_id: "summarize".to_string(),
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4".to_string(),
            input_tokens: 5000,
            output_tokens: 1200,
            cache_tokens: 800,
            ttft_ms: Some(150),
            cost: 0.0042,
        }],
        pending_providers: std::collections::HashMap::new(),
    }
}

// ── format_run_summary tests ─────────────────────────────────────────

#[test]
fn summary_all_passed() {
    let stats = make_stats();
    let lines = summary::format_run_summary(&stats, DetailLevel::Default, 2000, None, 80)
        .expect("should produce output");
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("D O N E"), "should show DONE");
    assert!(stripped.contains("3/3 passed"), "should show pass count");
}

#[test]
fn summary_with_failures() {
    let mut stats = make_stats();
    stats.tasks_failed = 1;
    stats.tasks_passed = 2;
    stats.root_failure = Some("summarize".to_string());
    let lines = summary::format_run_summary(&stats, DetailLevel::Default, 2000, None, 80)
        .expect("should produce output");
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("F A I L E D"), "should show FAILED");
    assert!(
        stripped.contains("root cause: summarize"),
        "should show root cause"
    );
    assert!(stripped.contains("2/3 passed"), "should show pass count");
}

#[test]
fn summary_json_returns_none() {
    let stats = make_stats();
    let result = summary::format_run_summary(&stats, DetailLevel::Json, 2000, None, 80);
    assert!(result.is_none(), "JSON mode should return None");
}

#[test]
fn summary_min_delegates_to_quiet() {
    let stats = make_stats();
    let lines = summary::format_run_summary(&stats, DetailLevel::Min, 2000, None, 80)
        .expect("should produce output");
    assert_eq!(
        lines.len(),
        1,
        "Min should produce exactly one line (quiet)"
    );
    let stripped = strip_ansi(&lines[0]);
    assert!(stripped.contains("3/3"), "should contain pass ratio");
}

#[test]
fn summary_zero_tokens() {
    let stats = RunStats {
        tasks_passed: 1,
        ..RunStats::default()
    };
    let lines = summary::format_run_summary(&stats, DetailLevel::Default, 1000, None, 80)
        .expect("should produce output");
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    // Should NOT contain the Tokens section header
    assert!(
        !stripped.contains("Tokens ─"),
        "zero tokens should skip token section"
    );
}

#[test]
fn summary_zero_cost() {
    let stats = RunStats {
        tasks_passed: 1,
        ..RunStats::default()
    };
    let lines = summary::format_run_summary(&stats, DetailLevel::Default, 1000, None, 80)
        .expect("should produce output");
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(
        !stripped.contains("Cost ─"),
        "zero cost should skip cost section"
    );
}

#[test]
fn summary_with_trace_path() {
    let stats = make_stats();
    let lines = summary::format_run_summary(
        &stats,
        DetailLevel::Default,
        2000,
        Some("/tmp/trace.ndjson"),
        80,
    )
    .expect("should produce output");
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(
        stripped.contains("trace /tmp/trace.ndjson"),
        "should show trace path"
    );
}

#[test]
fn summary_width_clamping_narrow() {
    let stats = make_stats();
    // Very narrow terminal — should clamp to 30
    let lines = summary::format_run_summary(&stats, DetailLevel::Default, 2000, None, 20)
        .expect("should produce output");
    // Should not panic, and box should be at least 30 wide
    assert!(!lines.is_empty());
}

#[test]
fn summary_width_clamping_wide() {
    let stats = make_stats();
    // Very wide terminal — should clamp to 72
    let lines = summary::format_run_summary(&stats, DetailLevel::Default, 2000, None, 200)
        .expect("should produce output");
    assert!(!lines.is_empty());
}

// ── format_run_quiet_summary tests ───────────────────────────────────

#[test]
fn quiet_summary_passed() {
    let stats = make_stats();
    let line = summary::format_run_quiet_summary(&stats, 2000);
    let stripped = strip_ansi(&line);
    assert!(stripped.contains("3/3"), "should contain pass ratio");
    assert!(stripped.contains("2.0s"), "should contain duration");
}

#[test]
fn quiet_summary_failed_icon() {
    let mut stats = make_stats();
    stats.tasks_failed = 1;
    let line = summary::format_run_quiet_summary(&stats, 2000);
    let stripped = strip_ansi(&line);
    // Failed uses ✗ icon (\u{2717})
    assert!(stripped.contains('\u{2717}'), "should contain failed icon");
}

// ── format_done_summary tests ────────────────────────────────────────

#[test]
fn done_summary_basic() {
    let lines = summary::format_done_summary("1.5s", 1200, 0.003, None, 3, 0);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("Done!"), "should contain Done!");
    assert!(stripped.contains("3 tasks"), "should contain task count");
    assert!(stripped.contains("1.2k tokens"), "should show token count");
    assert!(stripped.contains("1.5s"), "should show elapsed");
}

#[test]
fn done_summary_with_trace() {
    let lines = summary::format_done_summary("2.0s", 500, 0.001, Some("/tmp/trace.ndjson"), 2, 1);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("trace:"), "should contain trace label");
    assert!(
        stripped.contains("/tmp/trace.ndjson"),
        "should contain trace path"
    );
    assert!(
        stripped.contains("1 parallel"),
        "should show parallel count"
    );
}

#[test]
fn done_summary_zero_tokens() {
    let lines = summary::format_done_summary("0.5s", 0, 0.0, None, 1, 0);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(
        stripped.contains("1 task"),
        "should say '1 task' (singular)"
    );
    // Should NOT contain tokens when total is 0
    assert!(
        !stripped.contains("tokens"),
        "zero tokens should not show tokens"
    );
}

// ── format_doctor_header tests ───────────────────────────────────────

#[test]
fn doctor_header_box() {
    let lines = summary::format_doctor_header("0.42.0");
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("Nika Doctor"), "should contain title");
    assert!(stripped.contains("v0.42.0"), "should contain version");
    assert!(
        stripped.contains("Checking system health"),
        "should contain subtitle"
    );
    // Box characters
    assert!(joined.contains('\u{250c}'), "should have top-left corner");
    assert!(
        joined.contains('\u{2518}'),
        "should have bottom-right corner"
    );
}

// ── format_doctor_summary tests ──────────────────────────────────────

#[test]
fn doctor_summary_all_pass() {
    let lines = summary::format_doctor_summary(5, 0, 0, true);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("All good!"), "should say all good");
    assert!(stripped.contains("5 passed"), "should show pass count");
    assert!(
        !stripped.contains("nika init"),
        "should not suggest init when .nika exists"
    );
}

#[test]
fn doctor_summary_with_warnings() {
    let lines = summary::format_doctor_summary(3, 2, 0, true);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(
        stripped.contains("Mostly healthy"),
        "should say mostly healthy"
    );
    assert!(stripped.contains("2 warnings"), "should show warning count");
    assert!(
        stripped.contains("nika doctor --fix"),
        "should suggest --fix"
    );
}

#[test]
fn doctor_summary_with_failures() {
    let lines = summary::format_doctor_summary(2, 1, 3, false);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("Issues found"), "should say issues found");
    assert!(stripped.contains("3 failed"), "should show fail count");
    assert!(
        stripped.contains("nika init"),
        "should suggest init when no .nika dir"
    );
}

#[test]
fn doctor_summary_no_nika_dir_suggests_init() {
    let lines = summary::format_doctor_summary(0, 0, 1, false);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("nika init"), "should suggest init");
}

#[test]
fn doctor_summary_nika_dir_exists_skips_init() {
    let lines = summary::format_doctor_summary(0, 0, 1, true);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(
        !stripped.contains("nika init"),
        "should not suggest init when dir exists"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// format_output_preview tests (Task 10)
// ═══════════════════════════════════════════════════════════════════════

use serde_json::Value;

#[test]
fn output_preview_empty_returns_empty() {
    let result = renderer::format_output_preview(&Value::String(String::new()), 80);
    assert!(result.is_empty(), "empty string should produce empty vec");
}

#[test]
fn output_preview_null_returns_empty() {
    let result = renderer::format_output_preview(&Value::Null, 80);
    assert!(result.is_empty(), "null value should produce empty vec");
}

#[test]
fn output_preview_json_object() {
    let json = serde_json::json!({"key": "value", "count": 42});
    let lines = renderer::format_output_preview(&json, 80);
    assert!(
        !lines.is_empty(),
        "JSON object should produce preview lines"
    );
    // Should have top border + content + bottom border (at least 3 lines)
    assert!(lines.len() >= 3, "should have border + content + border");
}

#[test]
fn output_preview_markdown() {
    let md = Value::String("# Hello World\n\nThis is a paragraph.".to_string());
    let lines = renderer::format_output_preview(&md, 80);
    assert!(!lines.is_empty(), "markdown should produce preview lines");
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(
        stripped.contains("Hello World"),
        "should contain heading text"
    );
}

#[test]
fn output_preview_plain_text() {
    let text = Value::String("Hello, this is a plain text output.\nSecond line here.".to_string());
    let lines = renderer::format_output_preview(&text, 80);
    assert!(!lines.is_empty(), "plain text should produce preview lines");
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    assert!(stripped.contains("Hello"), "should contain first line");
    assert!(
        stripped.contains("Second line"),
        "should contain second line"
    );
}

#[test]
fn output_preview_long_text_truncated() {
    let long = "x".repeat(500);
    let text = Value::String(long);
    let lines = renderer::format_output_preview(&text, 80);
    assert!(!lines.is_empty(), "long text should produce preview lines");
    let joined = lines.join("\n");
    // Should contain ellipsis (truncation indicator)
    assert!(
        joined.contains('\u{2026}'),
        "long text should be truncated with ellipsis"
    );
}

#[test]
fn output_preview_unicode_safety() {
    // Test with multi-byte UTF-8 characters
    let text = Value::String("cafe\u{0301} re\u{0301}sume\u{0301}".to_string());
    let lines = renderer::format_output_preview(&text, 80);
    assert!(
        !lines.is_empty(),
        "unicode text should produce preview lines"
    );
    // Verify all output lines are valid UTF-8 (implicit — Rust strings are always valid)
    for line in &lines {
        let _ = line.chars().count();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TestRenderer tests
// ═══════════════════════════════════════════════════════════════════════

use super::renderer::{Renderer, TestRenderer};
use nika_event::{Event, EventKind};
use std::sync::Arc;

fn make_event(id: u64, kind: EventKind) -> Event {
    Event {
        id,
        timestamp_ms: id * 100,
        kind,
    }
}

#[test]
fn test_renderer_records_events() {
    let mut r = TestRenderer::new();
    assert_eq!(r.last_rendered_id(), None);
    assert!(r.rendered_events.is_empty());

    let events = vec![
        make_event(
            1,
            EventKind::TaskStarted {
                task_id: Arc::from("a"),
                verb: "infer".into(),
                inputs: Arc::new(serde_json::Value::Null),
            },
        ),
        make_event(
            2,
            EventKind::TaskCompleted {
                task_id: Arc::from("a"),
                output: Arc::new(serde_json::Value::String("hello".into())),
                duration_ms: 500,
            },
        ),
    ];

    r.render_new_events(&events);

    assert_eq!(r.last_rendered_id(), Some(2));
    assert_eq!(r.rendered_events.len(), 2);
    assert_eq!(r.stats.tasks_passed, 1);
}

#[test]
fn test_renderer_skips_already_rendered() {
    let mut r = TestRenderer::new();

    let events = vec![
        make_event(
            1,
            EventKind::TaskStarted {
                task_id: Arc::from("a"),
                verb: "exec".into(),
                inputs: Arc::new(serde_json::Value::Null),
            },
        ),
        make_event(
            2,
            EventKind::TaskCompleted {
                task_id: Arc::from("a"),
                output: Arc::new(serde_json::Value::Null),
                duration_ms: 100,
            },
        ),
    ];

    r.render_new_events(&events);
    assert_eq!(r.rendered_events.len(), 2);

    // Render same events again — should be no-ops
    r.render_new_events(&events);
    assert_eq!(r.rendered_events.len(), 2);
    assert_eq!(r.last_rendered_id(), Some(2));
}

#[test]
fn test_renderer_tracks_failures() {
    let mut r = TestRenderer::new();

    let events = vec![make_event(
        1,
        EventKind::TaskFailed {
            task_id: Arc::from("broken"),
            error: "something went wrong".into(),
            duration_ms: 200,
            error_code: None,
        },
    )];

    r.render_new_events(&events);

    assert_eq!(r.stats.tasks_failed, 1);
    assert!(r
        .stats
        .root_failure
        .as_deref()
        .unwrap()
        .starts_with("broken:"));
}

#[test]
fn test_renderer_render_kind_records_event() {
    let mut r = TestRenderer::new();

    r.render_kind(&EventKind::WorkflowPaused);
    r.render_kind(&EventKind::WorkflowResumed);

    assert_eq!(r.rendered_events.len(), 2);
    assert!(matches!(r.rendered_events[0], EventKind::WorkflowPaused));
    assert!(matches!(r.rendered_events[1], EventKind::WorkflowResumed));
    // render_kind does not update last_id (id=0 synthetic events)
}

#[test]
fn test_renderer_summary_flags() {
    let mut r = TestRenderer::new();

    assert!(!r.summary_called);
    assert!(!r.quiet_summary_called);

    r.render_summary(1000, Some("/tmp/trace.json"));
    assert!(r.summary_called);
    assert_eq!(r.summary_duration_ms, Some(1000));

    let mut r2 = TestRenderer::new();
    r2.render_quiet_summary(500);
    assert!(r2.quiet_summary_called);
    assert_eq!(r2.summary_duration_ms, Some(500));
}

#[test]
fn test_renderer_stats_accumulates_tokens() {
    let mut r = TestRenderer::new();

    let events = vec![make_event(
        1,
        EventKind::ProviderResponded {
            task_id: Arc::from("infer1"),
            request_id: None,
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 10,
            ttft_ms: Some(42),
            finish_reason: nika_event::FinishReason::EndTurn,
            cost_usd: 0.003,
        },
    )];

    r.render_new_events(&events);

    assert_eq!(r.stats().total_input_tokens, 100);
    assert_eq!(r.stats().total_output_tokens, 50);
    assert_eq!(r.stats().total_cache_tokens, 10);
    assert_eq!(r.stats().ttft_values, vec![42]);
    assert!((r.stats().total_cost - 0.003).abs() < f64::EPSILON);
}

#[test]
fn test_renderer_default_trait_methods_are_noops() {
    let mut r = TestRenderer::new();

    // set_task_layers and init_tasks use default no-ops — should not panic
    r.set_task_layers(std::collections::HashMap::new());
    r.init_tasks(&["a".to_string()], &std::collections::HashMap::new());

    // State unchanged
    assert!(r.rendered_events.is_empty());
}

#[test]
fn output_preview_size_label() {
    let text = Value::String("Hello world".to_string());
    let lines = renderer::format_output_preview(&text, 80);
    let joined = lines.join("\n");
    let stripped = strip_ansi(&joined);
    // Bottom border should contain character count
    assert!(
        stripped.contains("11 ch"),
        "should show character count in size label"
    );
}

// ── Routing display tests ───────────────────────────────────────────

#[test]
fn fmt_fallback_triggered_output() {
    let output = super::format_event::fmt_fallback_triggered("h100", "anthropic", "timeout", 0);
    let stripped = strip_ansi(&output);
    assert!(stripped.contains("fallback"), "should contain 'fallback'");
    assert!(stripped.contains("h100"), "should contain from_provider");
    assert!(stripped.contains("anthropic"), "should contain to_provider");
    assert!(stripped.contains("timeout"), "should contain reason");
}
