// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Catalog-specific errors implementing [`NikaErrorCode`].

use nika_error::prelude::*;

/// Errors originating from catalog validation.
///
/// These are only raised during cross-reference validation (e.g. a provider's
/// `default_model` references a model with no pricing entry). Normal lookups
/// return `Option`, not errors.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[non_exhaustive]
pub enum CatalogError {
    /// A provider's default or cheap model has no pricing entry.
    #[error("provider `{provider}` references model `{model}` with no pricing")]
    #[diagnostic(help("add a pricing entry for `{model}` to the catalog"))]
    MissingPricing {
        /// Provider id.
        provider: &'static str,
        /// Model name that was not found.
        model: &'static str,
    },

    /// Duplicate entry detected in a catalog.
    #[error("duplicate {catalog} entry: `{name}`")]
    #[diagnostic(help("remove the duplicate `{name}` from the {catalog} catalog"))]
    DuplicateEntry {
        /// Catalog name (e.g. "providers", "builtins").
        catalog: &'static str,
        /// Duplicated entry name.
        name: String,
    },

    /// Sorted array is not actually sorted (compile-time invariant broken).
    #[error("{catalog} array is not sorted at index {index}: `{prev}` > `{current}`")]
    #[diagnostic(help("entries in the {catalog} catalog must be sorted alphabetically by name"))]
    NotSorted {
        /// Catalog name.
        catalog: &'static str,
        /// Index where the ordering violation was found.
        index: usize,
        /// Previous entry name.
        prev: String,
        /// Current entry name (should be > prev).
        current: String,
    },

    // ── NIKA-230..235 (Session 4a structural) ──────────────────────────
    /// NIKA-230: Catalog TOML file failed to parse.
    #[error("catalog TOML parse error at {path}:{line}:{col}: {msg}")]
    #[diagnostic(help("check TOML syntax near {path} line {line}"))]
    TomlParse {
        /// Path to the TOML file.
        path: String,
        /// 1-based line number.
        line: usize,
        /// 1-based column number.
        col: usize,
        /// Parser error message.
        msg: String,
    },

    /// NIKA-231: Schema version in TOML does not match expected version.
    #[error("catalog schema mismatch in {path}: expected `{expected}`, found `{actual}`")]
    #[diagnostic(help("update the `schema` field in {path} to `{expected}`"))]
    SchemaMismatch {
        /// Expected schema string (e.g. `"nika/model-capabilities@1.0"`).
        expected: &'static str,
        /// Actual schema string found in file.
        actual: String,
        /// Path to the TOML file.
        path: String,
    },

    /// NIKA-232: Two capability rules claim conflicting patches for the same scope.
    #[error("capability rule conflict between `{rule_a}` and `{rule_b}` in scope `{scope}`")]
    #[diagnostic(help("reorder or merge the conflicting rules in model-capabilities.toml"))]
    CapabilityRuleConflict {
        /// Name of the first rule.
        rule_a: String,
        /// Name of the second rule.
        rule_b: String,
        /// The scope where the conflict was detected.
        scope: String,
    },

    /// NIKA-233: A pricing axis value is out of valid range.
    #[error("pricing axis `{field}` has value {value} (bound: {bound})")]
    #[diagnostic(help("pricing values must be non-negative and finite"))]
    PricingAxisOutOfRange {
        /// Field name (e.g. `"input"`, `"output"`).
        field: String,
        /// The invalid value.
        value: f64,
        /// Description of the bound (e.g. `">= 0.0"`).
        bound: &'static str,
    },

    /// NIKA-234: `max_output_tokens` exceeds `context_window_tokens`.
    #[error("model `{model}`: max_output_tokens ({max_out}) > context_window_tokens ({ctx})")]
    #[diagnostic(help(
        "a model's max output cannot exceed its context window — fix the capability rule"
    ))]
    ContextWindowInvariant {
        /// Model name.
        model: String,
        /// `max_output_tokens` value.
        max_out: u32,
        /// `context_window_tokens` value.
        ctx: u32,
    },

    /// NIKA-235: Unrecognised `json_mode` value in capability rule.
    #[error("model `{model}`: unknown json_mode `{raw_value}`")]
    #[diagnostic(help("valid json_mode values: none, object, schema"))]
    JsonModeUnknown {
        /// Model name.
        model: String,
        /// The raw string that could not be parsed.
        raw_value: String,
    },
}

impl NikaErrorCode for CatalogError {
    fn nika_code(&self) -> NikaCode {
        match self {
            // Legacy variants — generic core/validation code.
            Self::MissingPricing { .. } | Self::DuplicateEntry { .. } | Self::NotSorted { .. } => {
                codes::NIKA_001
            }
            // Session 4a — dedicated catalog codes.
            Self::TomlParse { .. } => codes::NIKA_230,
            Self::SchemaMismatch { .. } => codes::NIKA_231,
            Self::CapabilityRuleConflict { .. } => codes::NIKA_232,
            Self::PricingAxisOutOfRange { .. } => codes::NIKA_233,
            Self::ContextWindowInvariant { .. } => codes::NIKA_234,
            Self::JsonModeUnknown { .. } => codes::NIKA_235,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_unique() {
        let codes_used: Vec<NikaCode> = [
            CatalogError::TomlParse {
                path: String::new(),
                line: 0,
                col: 0,
                msg: String::new(),
            },
            CatalogError::SchemaMismatch {
                expected: "",
                actual: String::new(),
                path: String::new(),
            },
            CatalogError::CapabilityRuleConflict {
                rule_a: String::new(),
                rule_b: String::new(),
                scope: String::new(),
            },
            CatalogError::PricingAxisOutOfRange {
                field: String::new(),
                value: 0.0,
                bound: "",
            },
            CatalogError::ContextWindowInvariant {
                model: String::new(),
                max_out: 0,
                ctx: 0,
            },
            CatalogError::JsonModeUnknown {
                model: String::new(),
                raw_value: String::new(),
            },
        ]
        .iter()
        .map(NikaErrorCode::nika_code)
        .collect();

        // All 6 new codes must be distinct.
        for (i, a) in codes_used.iter().enumerate() {
            for b in &codes_used[i + 1..] {
                assert_ne!(a.num, b.num, "duplicate NIKA code: NIKA-{:03}", a.num);
            }
        }
    }

    #[test]
    fn error_codes_match_expected_nums() {
        let e = CatalogError::TomlParse {
            path: "x.toml".into(),
            line: 1,
            col: 1,
            msg: "bad".into(),
        };
        assert_eq!(e.nika_code().num, 230);

        let e = CatalogError::SchemaMismatch {
            expected: "v1",
            actual: "v2".into(),
            path: "x.toml".into(),
        };
        assert_eq!(e.nika_code().num, 231);

        let e = CatalogError::CapabilityRuleConflict {
            rule_a: "a".into(),
            rule_b: "b".into(),
            scope: "s".into(),
        };
        assert_eq!(e.nika_code().num, 232);

        let e = CatalogError::PricingAxisOutOfRange {
            field: "input".into(),
            value: -1.0,
            bound: ">= 0.0",
        };
        assert_eq!(e.nika_code().num, 233);

        let e = CatalogError::ContextWindowInvariant {
            model: "m".into(),
            max_out: 200,
            ctx: 100,
        };
        assert_eq!(e.nika_code().num, 234);

        let e = CatalogError::JsonModeUnknown {
            model: "m".into(),
            raw_value: "bad".into(),
        };
        assert_eq!(e.nika_code().num, 235);
    }

    #[test]
    fn display_format_includes_details() {
        let e = CatalogError::TomlParse {
            path: "data/model.toml".into(),
            line: 42,
            col: 5,
            msg: "unexpected key".into(),
        };
        let s = e.to_string();
        assert!(s.contains("42"), "should contain line number");
        assert!(s.contains("model.toml"), "should contain file path");

        let e = CatalogError::ContextWindowInvariant {
            model: "gpt-5".into(),
            max_out: 200_000,
            ctx: 128_000,
        };
        let s = e.to_string();
        assert!(s.contains("gpt-5"), "should contain model name");
        assert!(s.contains("200000"), "should contain max_out");
        assert!(s.contains("128000"), "should contain ctx");
    }

    #[test]
    fn legacy_variants_still_use_nika_001() {
        let e = CatalogError::MissingPricing {
            provider: "test",
            model: "test",
        };
        assert_eq!(e.nika_code().num, 1);
    }
}
