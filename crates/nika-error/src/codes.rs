// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIKA-XXX code registry — dual wire format + typed struct.
//!
//! Each error code is a [`NikaCode`] with a numeric id, [`Category`],
//! [`Severity`], and a kebab-case slug for documentation URLs.
//!
//! Wire format: `"NIKA-001"` (Display impl, stable across versions).

use std::fmt;

/// Structured error code combining numeric id, category, severity, and slug.
///
/// # Display
///
/// Formats as `"NIKA-{num:03}"` for wire stability:
/// ```
/// use nika_error::codes::NIKA_001;
/// assert_eq!(format!("{NIKA_001}"), "NIKA-001");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NikaCode {
    /// Numeric identifier (1..=9999). Unique across the entire registry.
    pub num: u16,
    /// Functional category for grouping and routing.
    pub category: Category,
    /// Severity level.
    pub severity: Severity,
    /// Kebab-case slug for documentation URLs (e.g. `"validation-failed"`).
    pub slug: &'static str,
}

impl fmt::Display for NikaCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NIKA-{:03}", self.num)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for NikaCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for NikaCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(deserializer)?;
        lookup(s).ok_or_else(|| serde::de::Error::custom(format!("unknown code: {s}")))
    }
}

/// Functional category for error grouping and routing.
///
/// Numeric ranges are a convention (not enforced at type level):
/// Core 001-049, Shell 050-099, `FileIo` 100-139, Http 140-189,
/// Auth 190-229, Mcp 230-279, Schema 280-329, Binding 330-379,
/// Provider 380-429, Verb 430-479, Runtime 480-529,
/// Memory 600-649, `WasmPlugin` 700-749, Sandbox 750-799,
/// Observability 800-819.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum Category {
    Core,
    Shell,
    FileIo,
    Http,
    Auth,
    Mcp,
    Schema,
    Binding,
    Provider,
    Verb,
    Runtime,
    /// Memory subsystem (600-649).
    Memory,
    /// WASM plugin host execution (700-749).
    WasmPlugin,
    /// Capability-based sandbox (750-799).
    Sandbox,
    /// Observability/telemetry sinks (800-819).
    Observability,
}

/// Severity level for an error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
}

// ─── Phase 1 codes (L0 only) ──────────────────────────────────────────────

/// NIKA-001: Input validation failed.
pub const NIKA_001: NikaCode = NikaCode {
    num: 1,
    category: Category::Core,
    severity: Severity::Error,
    slug: "validation-failed",
};

/// NIKA-002: Referenced item not found.
pub const NIKA_002: NikaCode = NikaCode {
    num: 2,
    category: Category::Core,
    severity: Severity::Error,
    slug: "not-found",
};

/// NIKA-003: Feature not supported.
pub const NIKA_003: NikaCode = NikaCode {
    num: 3,
    category: Category::Core,
    severity: Severity::Error,
    slug: "unsupported",
};

// ─── Catalog validation codes (200-299) ──────────────────────────────────

/// NIKA-230: Catalog TOML parse failure.
pub const NIKA_230: NikaCode = NikaCode {
    num: 230,
    category: Category::Core,
    severity: Severity::Error,
    slug: "catalog-toml-parse",
};

/// NIKA-231: Catalog schema version mismatch.
pub const NIKA_231: NikaCode = NikaCode {
    num: 231,
    category: Category::Core,
    severity: Severity::Error,
    slug: "catalog-schema-mismatch",
};

/// NIKA-232: Conflicting capability rules.
pub const NIKA_232: NikaCode = NikaCode {
    num: 232,
    category: Category::Core,
    severity: Severity::Error,
    slug: "capability-rule-conflict",
};

/// NIKA-233: Pricing axis value out of range.
pub const NIKA_233: NikaCode = NikaCode {
    num: 233,
    category: Category::Core,
    severity: Severity::Error,
    slug: "pricing-axis-out-of-range",
};

/// NIKA-234: Context window invariant violated (`max_output` > `context_window`).
pub const NIKA_234: NikaCode = NikaCode {
    num: 234,
    category: Category::Core,
    severity: Severity::Error,
    slug: "context-window-invariant",
};

/// NIKA-235: Unrecognised JSON mode value.
pub const NIKA_235: NikaCode = NikaCode {
    num: 235,
    category: Category::Core,
    severity: Severity::Error,
    slug: "json-mode-unknown",
};

// ─── Reserved subsystem codes (600+) ─────────────────────────────────────

/// NIKA-600: Memory subsystem error (range placeholder 600-649).
pub const NIKA_600: NikaCode = NikaCode {
    num: 600,
    category: Category::Memory,
    severity: Severity::Error,
    slug: "memory",
};

/// NIKA-700: WASM plugin error (range placeholder 700-749).
pub const NIKA_700: NikaCode = NikaCode {
    num: 700,
    category: Category::WasmPlugin,
    severity: Severity::Error,
    slug: "wasm-plugin",
};

/// NIKA-750: Sandbox error (range placeholder 750-799).
pub const NIKA_750: NikaCode = NikaCode {
    num: 750,
    category: Category::Sandbox,
    severity: Severity::Error,
    slug: "sandbox",
};

/// NIKA-800: Observability error (range placeholder 800-819).
pub const NIKA_800: NikaCode = NikaCode {
    num: 800,
    category: Category::Observability,
    severity: Severity::Error,
    slug: "observability",
};

/// NIKA-999: Internal error (catch-all).
pub const NIKA_999: NikaCode = NikaCode {
    num: 999,
    category: Category::Core,
    severity: Severity::Error,
    slug: "internal",
};

/// All registered codes. Used for uniqueness validation and iteration.
pub const ALL: &[NikaCode] = &[
    NIKA_001, NIKA_002, NIKA_003, NIKA_230, NIKA_231, NIKA_232, NIKA_233, NIKA_234, NIKA_235,
    NIKA_600, NIKA_700, NIKA_750, NIKA_800, NIKA_999,
];

/// Returns an actionable help message for a given code.
///
/// Every registered code has a help string. This is used by miette's
/// `help()` diagnostic and by the display layer for user-facing output.
#[must_use]
pub fn code_help(code: NikaCode) -> &'static str {
    match code.num {
        1 => "Check your workflow YAML syntax and field values.",
        2 => "Referenced item not found in catalogs or task outputs.",
        3 => "Feature not supported in current configuration.",
        230 => "Catalog TOML is malformed. Check syntax near the reported line and column.",
        231 => {
            "Catalog schema version does not match the expected version. Update the `schema` field."
        }
        232 => {
            "Two capability rules conflict for the same scope. Check rule ordering in model-capabilities.toml."
        }
        233 => {
            "A pricing axis value is out of valid range. Ensure rates are non-negative and finite."
        }
        234 => "max_output_tokens exceeds context_window_tokens. Fix the model capability rule.",
        235 => "Unrecognised json_mode value. Valid values: none, object, schema.",
        600..=649 => {
            "Memory subsystem reported an error. Check store availability, embedding provider, and tenant quotas."
        }
        700..=749 => {
            "WASM plugin host reported an error. Check plugin manifest and capability grants."
        }
        750..=799 => "Sandbox denied or failed. Verify capability allowlist and platform support.",
        800..=819 => "Observability sink rejected the event. Check exporter configuration.",
        999 => "Internal error. Please report at github.com/supernovae-st/nika/issues",
        _ => "Unknown error code. Check documentation for details.",
    }
}

/// Look up a [`NikaCode`] by its wire string (e.g. `"NIKA-001"`).
///
/// Returns `None` if the string doesn't match any registered code.
#[must_use]
pub fn lookup(wire: &str) -> Option<NikaCode> {
    let num_str = wire.strip_prefix("NIKA-")?;
    let num: u16 = num_str.parse().ok()?;
    ALL.iter().copied().find(|c| c.num == num)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format_three_digit_padding() {
        assert_eq!(format!("{NIKA_001}"), "NIKA-001");
        assert_eq!(format!("{NIKA_999}"), "NIKA-999");
    }

    #[test]
    fn display_format_no_extra_padding_above_999() {
        let big = NikaCode {
            num: 1234,
            category: Category::Core,
            severity: Severity::Error,
            slug: "test",
        };
        assert_eq!(format!("{big}"), "NIKA-1234");
    }

    #[test]
    fn nika_001_is_core_validation() {
        assert_eq!(NIKA_001.category, Category::Core);
        assert_eq!(NIKA_001.severity, Severity::Error);
        assert_eq!(NIKA_001.slug, "validation-failed");
    }

    #[test]
    fn all_codes_unique_nums() {
        for (i, a) in ALL.iter().enumerate() {
            for b in ALL.iter().skip(i + 1) {
                assert_ne!(a.num, b.num, "duplicate num: {a} and {b}");
            }
        }
    }

    #[test]
    fn all_codes_unique_slugs() {
        for (i, a) in ALL.iter().enumerate() {
            for b in ALL.iter().skip(i + 1) {
                assert_ne!(a.slug, b.slug, "duplicate slug: {} and {}", a.slug, b.slug);
            }
        }
    }

    #[test]
    fn code_help_returns_non_empty_for_registered() {
        for code in ALL {
            let help = code_help(*code);
            assert!(!help.is_empty(), "empty help for {code}");
            assert!(
                !help.contains("Unknown"),
                "registered code {code} got fallback help — add specific entry to code_help()"
            );
        }
    }

    #[test]
    fn code_help_specific_values() {
        assert!(code_help(NIKA_001).contains("YAML"));
        assert!(code_help(NIKA_002).contains("not found"));
        assert!(code_help(NIKA_003).contains("not supported"));
        assert!(code_help(NIKA_999).contains("Internal"));
    }

    #[test]
    fn code_help_unknown_returns_fallback() {
        let unknown = NikaCode {
            num: 8888,
            category: Category::Core,
            severity: Severity::Error,
            slug: "unknown",
        };
        assert!(code_help(unknown).contains("Unknown"));
    }

    #[test]
    fn lookup_valid_codes() {
        assert_eq!(lookup("NIKA-001"), Some(NIKA_001));
        assert_eq!(lookup("NIKA-999"), Some(NIKA_999));
    }

    #[test]
    fn lookup_invalid_returns_none() {
        assert_eq!(lookup("NIKA-555"), None);
        assert_eq!(lookup("INVALID"), None);
        assert_eq!(lookup(""), None);
    }

    #[test]
    fn lookup_roundtrip_via_display() {
        for code in ALL {
            let wire = format!("{code}");
            let found = lookup(&wire);
            assert_eq!(found, Some(*code), "roundtrip failed for {code}");
        }
    }

    #[cfg(feature = "serde")]
    mod serde_tests {
        use super::*;

        #[test]
        fn nika_code_serializes_as_wire_string() {
            let json = serde_json::to_string(&NIKA_001).expect("serialize");
            assert_eq!(json, "\"NIKA-001\"");
        }

        #[test]
        fn nika_code_deserializes_from_wire_string() {
            let code: NikaCode = serde_json::from_str("\"NIKA-001\"").expect("deserialize");
            assert_eq!(code, NIKA_001);
        }

        #[test]
        fn nika_code_serde_roundtrip() {
            for code in ALL {
                let json = serde_json::to_string(code).expect("serialize");
                let back: NikaCode = serde_json::from_str(&json).expect("deserialize");
                assert_eq!(back, *code);
            }
        }

        #[test]
        fn category_serializes_kebab_case() {
            let json = serde_json::to_string(&Category::FileIo).expect("serialize");
            assert_eq!(json, "\"file-io\"");
        }

        #[test]
        fn unknown_code_deser_fails() {
            let result: Result<NikaCode, _> = serde_json::from_str("\"NIKA-555\"");
            assert!(result.is_err());
        }
    }

    mod proptest_codes {
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn display_always_starts_with_nika(num in 1u16..=9999u16) {
                let code = super::NikaCode {
                    num,
                    category: super::Category::Core,
                    severity: super::Severity::Error,
                    slug: "test",
                };
                let display = format!("{code}");
                prop_assert!(display.starts_with("NIKA-"));
                prop_assert!(display.len() >= 8); // "NIKA-" + at least 3 digits
            }
        }

        #[test]
        fn all_registered_codes_have_unique_nums() {
            let nums: Vec<u16> = super::ALL.iter().map(|c| c.num).collect();
            let mut sorted = nums.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(nums.len(), sorted.len(), "duplicate nums in ALL registry");
        }
    }
}
