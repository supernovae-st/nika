// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use super::parser::split_pipe_respecting_parens;
use rstest::rstest;
use serde_json::json;


    // ─────────────────────────────────────────────────────────────
    // Parse tests — simple transforms (rstest parametrized)
    // ─────────────────────────────────────────────────────────────

    #[rstest]
    #[case("upper", &[TransformOp::Upper])]
    #[case("lower", &[TransformOp::Lower])]
    #[case("trim", &[TransformOp::Trim])]
    #[case("length", &[TransformOp::Length])]
    #[case("first", &[TransformOp::First])]
    #[case("shell", &[TransformOp::Shell])]
    #[case("to_json", &[TransformOp::ToJson])]
    #[case("parse_json", &[TransformOp::ParseJson])]
    #[case("round", &[TransformOp::Round(None)])]
    fn parse_simple_transform(#[case] input: &str, #[case] expected: &[TransformOp]) {
        let expr = TransformExpr::parse(input).unwrap();
        assert_eq!(expr.ops.as_slice(), expected);
    }

    // Parse tests — parametric transforms (need owned values, separate table)
    #[rstest]
    #[case("first(3)", TransformOp::FirstN(3))]
    #[case("last(5)", TransformOp::LastN(5))]
    #[case("round(2)", TransformOp::Round(Some(2)))]
    fn parse_parametric_transform(#[case] input: &str, #[case] expected: TransformOp) {
        let expr = TransformExpr::parse(input).unwrap();
        assert_eq!(expr.ops.as_slice(), &[expected]);
    }

    #[rstest]
    #[case("join(', ')", TransformOp::Join(", ".to_string()))]
    #[case("split('/')", TransformOp::Split("/".to_string()))]
    #[case("default('N/A')", TransformOp::Default(Value::String("N/A".to_string())))]
    fn parse_string_parametric(#[case] input: &str, #[case] expected: TransformOp) {
        let expr = TransformExpr::parse(input).unwrap();
        assert_eq!(expr.ops.as_slice(), &[expected]);
    }

    #[test]
    fn parse_default_number() {
        let expr = TransformExpr::parse("default(42)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Default(json!(42))]);
    }

    #[test]
    fn parse_unknown() {
        let err = TransformExpr::parse("bogus").unwrap_err();
        assert!(err.reason.contains("unknown transform"));
    }

    #[test]
    fn parse_unknown_with_suggestion() {
        let err = TransformExpr::parse("uper").unwrap_err();
        assert!(
            err.reason.contains("Did you mean 'upper'"),
            "should suggest 'upper' for 'uper', got: {}",
            err.reason
        );
    }

    #[test]
    fn parse_unknown_parametric_with_suggestion() {
        let err = TransformExpr::parse("jion(',')").unwrap_err();
        assert!(
            err.reason.contains("Did you mean 'join'"),
            "should suggest 'join' for 'jion', got: {}",
            err.reason
        );
    }

    #[test]
    fn parse_pipeline() {
        let expr = TransformExpr::parse("sort | unique | first(3)").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[
                TransformOp::Sort,
                TransformOp::Unique,
                TransformOp::FirstN(3),
            ]
        );
    }

    #[test]
    fn parse_empty() {
        let expr = TransformExpr::parse("").unwrap();
        assert!(expr.is_empty());
    }

    #[test]
    fn parse_single() {
        let expr = TransformExpr::parse("upper").unwrap();
        assert_eq!(expr.ops.len(), 1);
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — String
    // ─────────────────────────────────────────────────────────────

    #[rstest]
    #[case(TransformOp::Upper, json!("hello"), json!("HELLO"))]
    #[case(TransformOp::Lower, json!("HELLO"), json!("hello"))]
    #[case(TransformOp::Trim, json!(" hello "), json!("hello"))]
    #[case(TransformOp::TrimStart, json!("  hello  "), json!("hello  "))]
    #[case(TransformOp::TrimEnd, json!("  hello  "), json!("  hello"))]
    fn apply_string_transform(
        #[case] op: TransformOp,
        #[case] input: Value,
        #[case] expected: Value,
    ) {
        let result = op.apply(&input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn apply_upper_non_string() {
        let err = TransformOp::Upper.apply(&json!(42)).unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    #[test]
    fn apply_upper_null() {
        let err = TransformOp::Upper.apply(&Value::Null).unwrap_err();
        assert!(matches!(err, TransformError::NullInput { .. }));
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — Collection
    // ─────────────────────────────────────────────────────────────

    #[rstest]
    #[case(TransformOp::Length, json!([1, 2, 3]), json!(3))]
    #[case(TransformOp::Length, json!("abc"), json!(3))]
    #[case(TransformOp::Length, json!({"a": 1, "b": 2}), json!(2))]
    fn apply_length_variants(
        #[case] op: TransformOp,
        #[case] input: Value,
        #[case] expected: Value,
    ) {
        let result = op.apply(&input).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn apply_length_null() {
        let result = TransformOp::Length.apply(&Value::Null).unwrap();
        assert_eq!(result, Value::Null); // propagating
    }

    #[test]
    fn apply_first_array() {
        let result = TransformOp::First.apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!(1));
    }

    #[test]
    fn apply_first_empty() {
        let result = TransformOp::First.apply(&json!([])).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn apply_last_array() {
        let result = TransformOp::Last.apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!(3));
    }

    #[test]
    fn apply_first_n() {
        let result = TransformOp::FirstN(3)
            .apply(&json!([1, 2, 3, 4, 5]))
            .unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn apply_last_n() {
        let result = TransformOp::LastN(2)
            .apply(&json!([1, 2, 3, 4, 5]))
            .unwrap();
        assert_eq!(result, json!([4, 5]));
    }

    #[test]
    fn apply_keys() {
        let result = TransformOp::Keys.apply(&json!({"a": 1, "b": 2})).unwrap();
        // serde_json::Map preserves insertion order
        assert_eq!(result, json!(["a", "b"]));
    }

    #[test]
    fn apply_keys_null() {
        let result = TransformOp::Keys.apply(&Value::Null).unwrap();
        assert_eq!(result, Value::Null); // propagating
    }

    #[test]
    fn apply_values() {
        let result = TransformOp::Values.apply(&json!({"a": 1, "b": 2})).unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn apply_sort() {
        let result = TransformOp::Sort.apply(&json!([3, 1, 2])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn apply_unique() {
        let result = TransformOp::Unique.apply(&json!([1, 2, 2, 3])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn apply_compact() {
        let result = TransformOp::Compact
            .apply(&json!([1, null, 2, null]))
            .unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn apply_compact_filters_empty_strings() {
        let result = TransformOp::Compact
            .apply(&json!(["hello", "", null, "world", ""]))
            .unwrap();
        assert_eq!(result, json!(["hello", "world"]));
    }

    #[test]
    fn apply_flatten() {
        let result = TransformOp::Flatten.apply(&json!([[1, 2], [3]])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn apply_reverse() {
        let result = TransformOp::Reverse.apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!([3, 2, 1]));
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — Type conversion
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn apply_to_string() {
        let result = TransformOp::ToString.apply(&json!(42)).unwrap();
        assert_eq!(result, json!("42"));
    }

    #[test]
    fn apply_to_string_null() {
        let result = TransformOp::ToString.apply(&Value::Null).unwrap();
        assert_eq!(result, Value::Null); // propagating
    }

    #[test]
    fn apply_to_number() {
        let result = TransformOp::ToNumber.apply(&json!("42")).unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn apply_to_number_float() {
        let result = TransformOp::ToNumber.apply(&json!("3.12")).unwrap();
        assert_eq!(result, json!(3.12));
    }

    #[test]
    fn apply_to_bool_number() {
        assert_eq!(TransformOp::ToBool.apply(&json!(1)).unwrap(), json!(true));
        assert_eq!(TransformOp::ToBool.apply(&json!(0)).unwrap(), json!(false));
    }

    #[test]
    fn apply_to_bool_string() {
        assert_eq!(
            TransformOp::ToBool.apply(&json!("true")).unwrap(),
            json!(true)
        );
        assert_eq!(
            TransformOp::ToBool.apply(&json!("false")).unwrap(),
            json!(false)
        );
    }

    #[test]
    fn apply_to_json() {
        let result = TransformOp::ToJson.apply(&json!([1, 2])).unwrap();
        assert_eq!(result, json!("[1,2]"));
    }

    #[test]
    fn apply_parse_json() {
        let result = TransformOp::ParseJson.apply(&json!(r#"{"a":1}"#)).unwrap();
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn apply_parse_json_unicode() {
        // E17: CJK, accents, emoji, RTL — must survive parse_json roundtrip
        let input = r#"{"fr":"Café crème à Paris","ja":"東京タワー","ar":"مرحبا بالعالم","emoji":"🦋🚀✨"}"#;
        let result = TransformOp::ParseJson
            .apply(&Value::String(input.to_string()))
            .unwrap();
        assert_eq!(result["fr"], "Café crème à Paris");
        assert_eq!(result["ja"], "東京タワー");
        assert_eq!(result["ar"], "مرحبا بالعالم");
        assert_eq!(result["emoji"], "🦋🚀✨");
    }

    #[test]
    fn apply_parse_json_with_bom() {
        // E17: UTF-8 BOM at start of exec output
        let input = "\u{FEFF}{\"a\":1}";
        let result = TransformOp::ParseJson
            .apply(&Value::String(input.to_string()))
            .unwrap();
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn apply_parse_json_with_nul() {
        // E17: stray NUL byte in exec output
        let input = "{\"a\":1}\0";
        let result = TransformOp::ParseJson
            .apply(&Value::String(input.to_string()))
            .unwrap();
        assert_eq!(result, json!({"a": 1}));
    }

    #[test]
    fn apply_parse_json_error_includes_detail() {
        // E17: error message should include serde's actual error
        let err = TransformOp::ParseJson
            .apply(&json!("not json"))
            .unwrap_err();
        match err {
            TransformError::TypeMismatch { got, .. } => {
                // Should contain serde error detail, not just truncated input
                assert!(
                    got.contains("expected"),
                    "error should include serde detail: {}",
                    got
                );
            }
            _ => panic!("expected TypeMismatch"),
        }
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — Numeric
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn apply_round() {
        let result = TransformOp::Round(Some(2)).apply(&json!(4.56789)).unwrap();
        assert_eq!(result, json!(4.57));
    }

    #[test]
    fn apply_round_no_decimals() {
        // round with 0 decimals returns integer (consistent with ceil/floor)
        let result = TransformOp::Round(None).apply(&json!(3.7)).unwrap();
        assert_eq!(result, json!(4));
    }

    #[test]
    fn apply_abs() {
        let result = TransformOp::Abs.apply(&json!(-5)).unwrap();
        assert_eq!(result, json!(5));
    }

    #[test]
    fn apply_abs_float() {
        let result = TransformOp::Abs.apply(&json!(-3.12)).unwrap();
        assert_eq!(result, json!(3.12));
    }

    #[test]
    fn apply_ceil() {
        let result = TransformOp::Ceil.apply(&json!(3.2)).unwrap();
        assert_eq!(result, json!(4));
    }

    #[test]
    fn apply_floor() {
        let result = TransformOp::Floor.apply(&json!(3.8)).unwrap();
        assert_eq!(result, json!(3));
    }

    // ─────────────────────────────────────────────────────────────
    // Apply tests — Utility
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn apply_join() {
        let result = TransformOp::Join(", ".to_string())
            .apply(&json!(["a", "b"]))
            .unwrap();
        assert_eq!(result, json!("a, b"));
    }

    #[test]
    fn apply_split() {
        let result = TransformOp::Split("/".to_string())
            .apply(&json!("a/b/c"))
            .unwrap();
        assert_eq!(result, json!(["a", "b", "c"]));
    }

    #[test]
    fn apply_default_with_null() {
        let result = TransformOp::Default(json!("N/A"))
            .apply(&Value::Null)
            .unwrap();
        assert_eq!(result, json!("N/A"));
    }

    #[test]
    fn apply_default_with_value() {
        let result = TransformOp::Default(json!("N/A"))
            .apply(&json!("hello"))
            .unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn apply_default_with_empty_string() {
        let result = TransformOp::Default(json!("FALLBACK"))
            .apply(&json!(""))
            .unwrap();
        assert_eq!(result, json!("FALLBACK"));
    }

    #[test]
    fn apply_default_preserves_whitespace_only_string() {
        let result = TransformOp::Default(json!("FALLBACK"))
            .apply(&json!("  "))
            .unwrap();
        assert_eq!(result, json!("  "), "whitespace-only strings are NOT empty");
    }

    #[test]
    fn apply_typeof() {
        assert_eq!(
            TransformOp::TypeOf.apply(&json!(42)).unwrap(),
            json!("number")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&json!("x")).unwrap(),
            json!("string")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&Value::Null).unwrap(),
            json!("null")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&json!(true)).unwrap(),
            json!("boolean")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&json!([1])).unwrap(),
            json!("array")
        );
        assert_eq!(
            TransformOp::TypeOf.apply(&json!({"a": 1})).unwrap(),
            json!("object")
        );
    }

    #[test]
    fn apply_shell() {
        let result = TransformOp::Shell.apply(&json!("hello world")).unwrap();
        assert_eq!(result, json!("'hello world'"));
    }

    #[test]
    fn apply_shell_null_errors() {
        let err = TransformOp::Shell.apply(&Value::Null).unwrap_err();
        assert!(matches!(err, TransformError::NullInput { op: "shell" }));
    }

    // ─────────────────────────────────────────────────────────────
    // URL transform tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_url_transforms() {
        assert_eq!(
            TransformExpr::parse("url_host").unwrap().ops[0],
            TransformOp::UrlHost
        );
        assert_eq!(
            TransformExpr::parse("url_path").unwrap().ops[0],
            TransformOp::UrlPath
        );
        assert_eq!(
            TransformExpr::parse("url_without_query").unwrap().ops[0],
            TransformOp::UrlWithoutQuery
        );
    }

    #[test]
    fn apply_url_host() {
        let url = json!("https://blog.example.com:8080/posts/123?page=2#top");
        assert_eq!(
            TransformOp::UrlHost.apply(&url).unwrap(),
            json!("blog.example.com")
        );
    }

    #[test]
    fn apply_url_path() {
        let url = json!("https://example.com/posts/123?page=2");
        assert_eq!(
            TransformOp::UrlPath.apply(&url).unwrap(),
            json!("/posts/123")
        );
    }

    #[test]
    fn apply_url_without_query() {
        let url = json!("https://example.com/posts/123?page=2&sort=new#comments");
        assert_eq!(
            TransformOp::UrlWithoutQuery.apply(&url).unwrap(),
            json!("https://example.com/posts/123")
        );
    }

    #[test]
    fn url_host_ipv6() {
        let url = json!("https://[::1]:3000/api");
        // URL-002: strip IPv6 brackets for cleaner output
        assert_eq!(TransformOp::UrlHost.apply(&url).unwrap(), json!("::1"));
    }

    #[test]
    fn url_transforms_invalid_url() {
        let bad = json!("not a url");
        assert!(TransformOp::UrlHost.apply(&bad).is_err());
        assert!(TransformOp::UrlPath.apply(&bad).is_err());
        assert!(TransformOp::UrlWithoutQuery.apply(&bad).is_err());
    }

    #[test]
    fn url_transforms_null_errors() {
        assert!(matches!(
            TransformOp::UrlHost.apply(&Value::Null).unwrap_err(),
            TransformError::NullInput { op: "url_host" }
        ));
    }

    #[test]
    fn url_pipeline_host_then_lower() {
        let url = json!("https://EXAMPLE.COM/Page");
        let expr = TransformExpr::parse("url_host | lower").unwrap();
        assert_eq!(expr.apply(&url).unwrap(), json!("example.com"));
    }

    // ─────────────────────────────────────────────────────────────
    // url_normalize tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn url_normalize_strips_utm() {
        let url = json!("https://example.com/page?utm_source=google&utm_medium=cpc&id=123");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page?id=123"));
    }

    #[test]
    fn url_normalize_removes_default_port() {
        let url = json!("https://example.com:443/page");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_removes_default_port_http() {
        let url = json!("http://example.com:80/page");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("http://example.com/page"));
    }

    #[test]
    fn url_normalize_sorts_params() {
        let url = json!("https://example.com/page?z=1&a=2&m=3");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page?a=2&m=3&z=1"));
    }

    #[test]
    fn url_normalize_strips_fragment() {
        let url = json!("https://example.com/page#section");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_strips_trailing_slash() {
        let url = json!("https://example.com/page/");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_preserves_root_slash() {
        let url = json!("https://example.com/");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/"));
    }

    #[test]
    fn url_normalize_strips_all_tracking() {
        let url = json!("https://example.com/page?fbclid=abc&gclid=def&page=2");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page?page=2"));
    }

    #[test]
    fn url_normalize_no_query_no_change() {
        let url = json!("https://example.com/page");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_all_tracking_removed() {
        let url = json!("https://example.com/page?utm_source=a&fbclid=b");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com/page"));
    }

    #[test]
    fn url_normalize_preserves_non_default_port() {
        let url = json!("https://example.com:8443/page");
        let result = TransformOp::UrlNormalize.apply(&url).unwrap();
        assert_eq!(result, json!("https://example.com:8443/page"));
    }

    #[test]
    fn url_normalize_chaining_with_host() {
        let url = json!("https://WWW.Example.COM:443/page?utm_source=x#top");
        let expr = TransformExpr::parse("url_normalize | url_host").unwrap();
        let result = expr.apply(&url).unwrap();
        assert_eq!(result, json!("www.example.com"));
    }

    #[test]
    fn url_normalize_null_errors() {
        assert!(matches!(
            TransformOp::UrlNormalize.apply(&Value::Null).unwrap_err(),
            TransformError::NullInput {
                op: "url_normalize"
            }
        ));
    }

    #[test]
    fn url_normalize_invalid_url_errors() {
        let bad = json!("not a url");
        assert!(TransformOp::UrlNormalize.apply(&bad).is_err());
    }

    #[test]
    fn parse_url_normalize() {
        assert_eq!(
            TransformExpr::parse("url_normalize").unwrap().ops[0],
            TransformOp::UrlNormalize
        );
    }

    // ─────────────────────────────────────────────────────────────
    // slice tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn slice_array_basic() {
        let arr = json!(["a", "b", "c", "d", "e"]);
        assert_eq!(
            TransformOp::Slice(1, 3).apply(&arr).unwrap(),
            json!(["b", "c"])
        );
    }

    #[test]
    fn slice_array_from_start() {
        let arr = json!([1, 2, 3, 4, 5]);
        assert_eq!(
            TransformOp::Slice(0, 3).apply(&arr).unwrap(),
            json!([1, 2, 3])
        );
    }

    #[test]
    fn slice_array_to_end() {
        let arr = json!([1, 2, 3, 4, 5]);
        assert_eq!(
            TransformOp::Slice(3, 100).apply(&arr).unwrap(),
            json!([4, 5])
        );
    }

    #[test]
    fn slice_array_empty_range() {
        let arr = json!([1, 2, 3]);
        assert_eq!(TransformOp::Slice(5, 10).apply(&arr).unwrap(), json!([]));
    }

    #[test]
    fn slice_string() {
        let s = json!("Hello World");
        assert_eq!(TransformOp::Slice(0, 5).apply(&s).unwrap(), json!("Hello"));
    }

    #[test]
    fn slice_null_errors() {
        assert!(TransformOp::Slice(0, 1).apply(&Value::Null).is_err());
    }

    #[test]
    fn parse_slice_transform() {
        let expr = TransformExpr::parse("slice(0, 100)").unwrap();
        assert_eq!(expr.ops[0], TransformOp::Slice(0, 100));
    }

    #[test]
    fn slice_pipeline() {
        let arr = json!(["x", "y", "z", "w"]);
        let expr = TransformExpr::parse("slice(1, 3) | length").unwrap();
        assert_eq!(expr.apply(&arr).unwrap(), json!(2));
    }

    #[test]
    fn display_url_normalize() {
        assert_eq!(TransformOp::UrlNormalize.to_string(), "url_normalize");
    }

    #[test]
    fn display_slice() {
        assert_eq!(TransformOp::Slice(0, 10).to_string(), "slice(0, 10)");
    }

    // ─────────────────────────────────────────────────────────────
    // Pipeline tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn pipeline_sort_unique() {
        let expr = TransformExpr::parse("sort | unique").unwrap();
        let result = expr.apply(&json!([3, 1, 2, 1])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn pipeline_sort_first_n() {
        let expr = TransformExpr::parse("sort | first(2)").unwrap();
        let result = expr.apply(&json!([3, 1, 2])).unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn pipeline_upper_trim() {
        let expr = TransformExpr::parse("trim | upper").unwrap();
        let result = expr.apply(&json!(" hello ")).unwrap();
        assert_eq!(result, json!("HELLO"));
    }

    #[test]
    fn pipeline_empty() {
        let expr = TransformExpr::parse("").unwrap();
        let result = expr.apply(&json!("unchanged")).unwrap();
        assert_eq!(result, json!("unchanged"));
    }

    #[test]
    fn pipeline_single() {
        let expr = TransformExpr::parse("upper").unwrap();
        assert_eq!(expr.ops.len(), 1);
    }

    #[test]
    fn pipeline_default_then_upper() {
        let expr = TransformExpr::parse("default('unknown') | upper").unwrap();
        let result = expr.apply(&Value::Null).unwrap();
        assert_eq!(result, json!("UNKNOWN"));
    }

    // ─────────────────────────────────────────────────────────────
    // Display
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn display_ops() {
        assert_eq!(TransformOp::Upper.to_string(), "upper");
        assert_eq!(TransformOp::FirstN(3).to_string(), "first(3)");
        assert_eq!(
            TransformOp::Join(", ".to_string()).to_string(),
            "join(', ')"
        );
        assert_eq!(TransformOp::Round(Some(2)).to_string(), "round(2)");
        assert_eq!(TransformOp::Round(None).to_string(), "round");
        assert_eq!(
            TransformOp::Default(json!("N/A")).to_string(),
            "default(\"N/A\")"
        );
    }

    // ─────────────────────────────────────────────────────────────
    // Error display
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn error_display_parse() {
        let err = TransformParseError {
            input: "bogus".to_string(),
            reason: "unknown transform: 'bogus'".to_string(),
        };
        assert!(err.to_string().contains("NIKA-151"));
    }

    #[test]
    fn error_display_type_mismatch() {
        let err = TransformError::TypeMismatch {
            op: "upper",
            expected: "string",
            got: "number".to_string(),
        };
        assert!(err.to_string().contains("NIKA-152"));
    }

    #[test]
    fn error_display_object_hints_extract_article() {
        let err = TransformError::TypeMismatch {
            op: "trim",
            expected: "string",
            got: "object".to_string(),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("text_content"),
            "should hint about extract: article fields, got: {}",
            msg
        );
    }

    #[test]
    fn error_display_null_input() {
        let err = TransformError::NullInput { op: "sort" };
        assert!(err.to_string().contains("NIKA-153"));
    }

    // ─────────────────────────────────────────────────────────────
    // Edge cases
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_default_bool() {
        let expr = TransformExpr::parse("default(true)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Default(json!(true))]);
    }

    #[test]
    fn parse_default_null() {
        let expr = TransformExpr::parse("default(null)").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Default(Value::Null)]);
    }

    #[test]
    fn parse_default_array() {
        let expr = TransformExpr::parse("default([])").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Default(json!([]))]);
    }

    #[test]
    fn first_n_larger_than_array() {
        let result = TransformOp::FirstN(10).apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!([1, 2, 3])); // takes what's available
    }

    #[test]
    fn last_n_larger_than_array() {
        let result = TransformOp::LastN(10).apply(&json!([1, 2, 3])).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn last_n_string() {
        let result = TransformOp::LastN(5).apply(&json!("hello world")).unwrap();
        assert_eq!(result, json!("world"));
    }

    #[test]
    fn last_n_string_unicode() {
        let result = TransformOp::LastN(2).apply(&json!("日本語")).unwrap();
        assert_eq!(result, json!("本語"));
    }

    #[test]
    fn last_n_string_exceeds_length() {
        let result = TransformOp::LastN(100).apply(&json!("short")).unwrap();
        assert_eq!(result, json!("short"));
    }

    #[test]
    fn last_n_empty_string() {
        let result = TransformOp::LastN(5).apply(&json!("")).unwrap();
        assert_eq!(result, json!(""));
    }

    #[test]
    fn last_n_object() {
        let obj = json!({"a": 1});
        let result = TransformOp::LastN(5).apply(&obj).unwrap();
        // Last 5 chars of JSON serialization
        assert!(result.is_string());
        assert!(result.as_str().unwrap().len() <= 5);
    }

    #[test]
    fn flatten_mixed() {
        let result = TransformOp::Flatten
            .apply(&json!([[1, 2], 3, [4]]))
            .unwrap();
        assert_eq!(result, json!([1, 2, 3, 4]));
    }

    #[test]
    fn unclosed_paren() {
        let err = TransformExpr::parse("first(3").unwrap_err();
        assert!(err.reason.contains("unclosed parenthesis"));
    }

    #[test]
    fn join_mixed_types() {
        let result = TransformOp::Join(", ".to_string())
            .apply(&json!(["a", 1, true]))
            .unwrap();
        assert_eq!(result, json!("a, 1, true"));
    }

    #[test]
    fn parse_chain_with_pipe_in_join_arg() {
        // B4: join(" | ") must NOT split on the | inside parentheses
        let expr = TransformExpr::parse(r#"trim | split(",") | join(" | ")"#).unwrap();
        assert_eq!(expr.ops.len(), 3);
        assert_eq!(
            expr.ops.as_slice(),
            &[
                TransformOp::Trim,
                TransformOp::Split(",".to_string()),
                TransformOp::Join(" | ".to_string()),
            ]
        );
    }

    #[test]
    fn apply_chain_with_pipe_in_join_arg() {
        // B4: end-to-end — split then join with pipe separator
        let expr = TransformExpr::parse(r#"split(",") | join(" | ")"#).unwrap();
        let result = expr.apply(&json!("a,b,c")).unwrap();
        assert_eq!(result, json!("a | b | c"));
    }

    #[test]
    fn parse_json_invalid() {
        let err = TransformOp::ParseJson
            .apply(&json!("not json"))
            .unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    #[test]
    fn to_number_invalid() {
        let err = TransformOp::ToNumber.apply(&json!("abc")).unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    #[test]
    fn to_bool_invalid_string() {
        let err = TransformOp::ToBool.apply(&json!("maybe")).unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    // ─────────────────────────────────────────────────────────────
    // Bug fix tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn first_n_on_object_serializes_and_truncates() {
        // BUG 3: first(N) on an object should serialize to JSON and truncate
        let obj = json!({"links": [1, 2, 3], "count": 3});
        let result = TransformOp::FirstN(10).apply(&obj).unwrap();
        // Should be a truncated JSON string
        assert!(result.is_string());
        let s = result.as_str().unwrap();
        assert_eq!(s.len(), 10);
    }

    #[test]
    fn first_n_on_object_full() {
        // first(N) with N larger than JSON length returns full JSON
        let obj = json!({"a": 1});
        let result = TransformOp::FirstN(1000).apply(&obj).unwrap();
        assert!(result.is_string());
        assert_eq!(result.as_str().unwrap(), r#"{"a":1}"#);
    }

    #[test]
    fn first_n_on_string_truncates() {
        let result = TransformOp::FirstN(5).apply(&json!("hello world")).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn parse_json_idempotent_on_array() {
        // BUG 7: parse_json on an already-parsed array should be a no-op
        let arr = json!([1, 2, 3]);
        let result = TransformOp::ParseJson.apply(&arr).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn parse_json_idempotent_on_object() {
        // BUG 7: parse_json on an already-parsed object should be a no-op
        let obj = json!({"key": "value"});
        let result = TransformOp::ParseJson.apply(&obj).unwrap();
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn parse_json_idempotent_on_number_and_bool() {
        // parse_json on auto-parsed primitives should be a no-op
        assert_eq!(TransformOp::ParseJson.apply(&json!(42)).unwrap(), json!(42));
        assert_eq!(
            TransformOp::ParseJson.apply(&json!(true)).unwrap(),
            json!(true)
        );
    }

    #[test]
    fn parse_json_strips_markdown_code_block() {
        let input = json!("```json\n{\"name\": \"test\"}\n```");
        let result = TransformOp::ParseJson.apply(&input).unwrap();
        assert_eq!(result, json!({"name": "test"}));
    }

    #[test]
    fn parse_json_strips_generic_code_block() {
        let input = json!("```\n[1, 2, 3]\n```");
        let result = TransformOp::ParseJson.apply(&input).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn parse_json_handles_bare_json() {
        let input = json!("{\"key\": \"value\"}");
        let result = TransformOp::ParseJson.apply(&input).unwrap();
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn parse_json_strips_whitespace_around_code_block() {
        let input = json!("  ```json\n  [\"a\", \"b\"]\n  ```  ");
        let result = TransformOp::ParseJson.apply(&input).unwrap();
        assert_eq!(result, json!(["a", "b"]));
    }

    // ── parse_yaml ───────────────────────────────────────────────

    #[test]
    fn parse_parse_yaml() {
        let expr = TransformExpr::parse("parse_yaml").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::ParseYaml]);
    }

    #[test]
    fn apply_parse_yaml_object() {
        let yaml = json!("name: hello\ncount: 42\n");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result["name"], "hello");
        assert_eq!(result["count"], 42);
    }

    #[test]
    fn apply_parse_yaml_array() {
        let yaml = json!("- one\n- two\n- three\n");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result, json!(["one", "two", "three"]));
    }

    #[test]
    fn apply_parse_yaml_nested() {
        let yaml = json!("locale: fr-FR\ncommunication:\n  formality: tu\n  tone: warm\n");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result["locale"], "fr-FR");
        assert_eq!(result["communication"]["formality"], "tu");
        assert_eq!(result["communication"]["tone"], "warm");
    }

    #[test]
    fn apply_parse_yaml_strips_markdown_code_block() {
        let yaml = json!("```yaml\nname: test\nvalue: 42\n```");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result["name"], "test");
        assert_eq!(result["value"], 42);
    }

    #[test]
    fn apply_parse_yaml_strips_yml_code_block() {
        let yaml = json!("```yml\n- a\n- b\n```");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result, json!(["a", "b"]));
    }

    #[test]
    fn apply_parse_yaml_unicode() {
        let yaml = json!("fr: Café crème\nja: 東京タワー\nemoji: 🦋\n");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result["fr"], "Café crème");
        assert_eq!(result["ja"], "東京タワー");
        assert_eq!(result["emoji"], "🦋");
    }

    #[test]
    fn apply_parse_yaml_null_fails() {
        let err = TransformOp::ParseYaml.apply(&Value::Null).unwrap_err();
        assert!(matches!(err, TransformError::NullInput { .. }));
    }

    #[test]
    fn apply_parse_yaml_idempotent_on_object() {
        let obj = json!({"key": "value"});
        let result = TransformOp::ParseYaml.apply(&obj).unwrap();
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn apply_parse_yaml_invalid() {
        let yaml = json!("{{invalid:\nyaml: [");
        let err = TransformOp::ParseYaml.apply(&yaml).unwrap_err();
        assert!(matches!(err, TransformError::TypeMismatch { .. }));
    }

    #[test]
    fn apply_parse_yaml_scalar_string() {
        // Plain YAML string should parse to a string value
        let yaml = json!("hello world");
        let result = TransformOp::ParseYaml.apply(&yaml).unwrap();
        assert_eq!(result, json!("hello world"));
    }

    #[test]
    fn to_json_then_length_returns_char_count() {
        // BUG 8: to_json | length should return string character count
        let obj = json!({"countries": ["FR", "US"]});
        let json_str = TransformOp::ToJson.apply(&obj).unwrap();
        assert!(json_str.is_string());
        let length = TransformOp::Length.apply(&json_str).unwrap();
        // Should be the character count of the JSON string, not 1
        assert!(length.as_u64().unwrap() > 1);
    }

    /// Bug 30: |length must return character count, not byte count for Unicode.
    #[test]
    fn regression_bug30_length_unicode_chars_not_bytes() {
        // "日本語" is 3 characters but 9 bytes in UTF-8
        let result = TransformOp::Length.apply(&json!("日本語")).unwrap();
        assert_eq!(
            result,
            json!(3),
            "|length on Unicode string must count chars, not bytes"
        );
    }

    /// Bug 30: additional Unicode edge cases for |length.
    #[test]
    fn regression_bug30_length_unicode_emoji() {
        // Emoji: "👋🌍" is 2 characters but 8 bytes
        let result = TransformOp::Length.apply(&json!("👋🌍")).unwrap();
        assert_eq!(result, json!(2), "|length on emoji string must count chars");
    }

    /// Bug 30: |length on ASCII should remain unchanged.
    #[test]
    fn regression_bug30_length_ascii_unchanged() {
        let result = TransformOp::Length.apply(&json!("abc")).unwrap();
        assert_eq!(result, json!(3), "|length on ASCII string is still correct");
    }

    /// Bug 46: |sort must use numeric ordering for numbers, not lexicographic.
    #[test]
    fn regression_bug46_sort_numeric_ordering() {
        let result = TransformOp::Sort.apply(&json!([1, 10, 2, 20, 3])).unwrap();
        assert_eq!(
            result,
            json!([1, 2, 3, 10, 20]),
            "|sort on numbers must use numeric ordering, not lexicographic"
        );
    }

    /// Bug 46: |sort with mixed types (numbers and strings).
    #[test]
    fn regression_bug46_sort_mixed_types() {
        let result = TransformOp::Sort.apply(&json!([10, 2, "b", "a"])).unwrap();
        assert_eq!(result, json!([2, 10, "a", "b"]));
    }

    /// Bug 46: |sort preserves string lexicographic ordering.
    #[test]
    fn regression_bug46_sort_strings_unchanged() {
        let result = TransformOp::Sort
            .apply(&json!(["banana", "apple", "cherry"]))
            .unwrap();
        assert_eq!(result, json!(["apple", "banana", "cherry"]));
    }

    /// Bug 46: |sort with floats.
    #[test]
    fn regression_bug46_sort_floats() {
        let result = TransformOp::Sort
            .apply(&json!([1.5, 0.1, 2.3, 0.9]))
            .unwrap();
        assert_eq!(result, json!([0.1, 0.9, 1.5, 2.3]));
    }

    // ─────────────────────────────────────────────────────────────
    // Data transforms — pluck, where, pick, omit, sort_by, group_by, merge
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_pluck() {
        let expr = TransformExpr::parse("pluck('name')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Pluck("name".to_string())]
        );
    }

    #[test]
    fn parse_pluck_double_quotes() {
        let expr = TransformExpr::parse(r#"pluck("status")"#).unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Pluck("status".to_string())]
        );
    }

    #[test]
    fn apply_pluck_basic() {
        let data = json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);
        let result = TransformOp::Pluck("name".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!(["Alice", "Bob"]));
    }

    #[test]
    fn apply_pluck_missing_field() {
        let data = json!([
            {"name": "Alice", "age": 30},
            {"age": 25},
            {"name": "Charlie"}
        ]);
        let result = TransformOp::Pluck("name".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!(["Alice", "Charlie"]));
    }

    #[test]
    fn apply_pluck_empty_array() {
        let result = TransformOp::Pluck("x".to_string())
            .apply(&json!([]))
            .unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn apply_pluck_null_errors() {
        assert!(TransformOp::Pluck("x".to_string())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn apply_pluck_not_array_errors() {
        assert!(TransformOp::Pluck("x".to_string())
            .apply(&json!({"x": 1}))
            .is_err());
    }

    #[test]
    fn parse_where() {
        let expr = TransformExpr::parse("where('status', 'active')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Where(
                "status".to_string(),
                "eq".to_string(),
                json!("active")
            )]
        );
    }

    #[test]
    fn parse_where_numeric() {
        let expr = TransformExpr::parse("where('age', 30)").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Where(
                "age".to_string(),
                "eq".to_string(),
                json!(30)
            )]
        );
    }

    #[test]
    fn apply_where_basic() {
        let data = json!([
            {"name": "Alice", "status": "active"},
            {"name": "Bob", "status": "inactive"},
            {"name": "Charlie", "status": "active"}
        ]);
        let result = TransformOp::Where("status".to_string(), "eq".to_string(), json!("active"))
            .apply(&data)
            .unwrap();
        assert_eq!(
            result,
            json!([
                {"name": "Alice", "status": "active"},
                {"name": "Charlie", "status": "active"}
            ])
        );
    }

    #[test]
    fn apply_where_no_match() {
        let data = json!([{"a": 1}, {"a": 2}]);
        let result = TransformOp::Where("a".to_string(), "eq".to_string(), json!(99))
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!([]));
    }

    #[test]
    fn apply_where_null_errors() {
        assert!(
            TransformOp::Where("x".to_string(), "eq".to_string(), json!("y"))
                .apply(&Value::Null)
                .is_err()
        );
    }

    #[test]
    fn parse_pick() {
        let expr = TransformExpr::parse("pick('name', 'age')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Pick(vec![
                "name".to_string(),
                "age".to_string()
            ])]
        );
    }

    #[test]
    fn apply_pick_basic() {
        let data = json!({"name": "Alice", "age": 30, "secret": "xxx", "role": "admin"});
        let result = TransformOp::Pick(vec!["name".to_string(), "age".to_string()])
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice", "age": 30}));
    }

    #[test]
    fn apply_pick_missing_field() {
        let data = json!({"name": "Alice"});
        let result = TransformOp::Pick(vec!["name".to_string(), "email".to_string()])
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice"}));
    }

    #[test]
    fn apply_pick_null_errors() {
        assert!(TransformOp::Pick(vec!["x".to_string()])
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn apply_pick_not_object_errors() {
        assert!(TransformOp::Pick(vec!["x".to_string()])
            .apply(&json!([1, 2]))
            .is_err());
    }

    #[test]
    fn parse_omit() {
        let expr = TransformExpr::parse("omit('password', 'secret')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Omit(vec![
                "password".to_string(),
                "secret".to_string()
            ])]
        );
    }

    #[test]
    fn apply_omit_basic() {
        let data = json!({"name": "Alice", "password": "xxx", "secret": "yyy"});
        let result = TransformOp::Omit(vec!["password".to_string(), "secret".to_string()])
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice"}));
    }

    #[test]
    fn apply_omit_missing_field() {
        let data = json!({"name": "Alice"});
        let result = TransformOp::Omit(vec!["nonexistent".to_string()])
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice"}));
    }

    #[test]
    fn apply_omit_null_errors() {
        assert!(TransformOp::Omit(vec!["x".to_string()])
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn parse_sort_by() {
        let expr = TransformExpr::parse("sort_by('age')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::SortBy("age".to_string())]
        );
    }

    #[test]
    fn apply_sort_by_numeric() {
        let data = json!([
            {"name": "Bob", "age": 25},
            {"name": "Alice", "age": 30},
            {"name": "Charlie", "age": 20}
        ]);
        let result = TransformOp::SortBy("age".to_string()).apply(&data).unwrap();
        assert_eq!(result[0]["name"], "Charlie");
        assert_eq!(result[1]["name"], "Bob");
        assert_eq!(result[2]["name"], "Alice");
    }

    #[test]
    fn apply_sort_by_string() {
        let data = json!([
            {"name": "Charlie"},
            {"name": "Alice"},
            {"name": "Bob"}
        ]);
        let result = TransformOp::SortBy("name".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result[0]["name"], "Alice");
        assert_eq!(result[1]["name"], "Bob");
        assert_eq!(result[2]["name"], "Charlie");
    }

    #[test]
    fn apply_sort_by_missing_field() {
        let data = json!([
            {"name": "Alice", "score": 90},
            {"name": "Bob"},
            {"name": "Charlie", "score": 80}
        ]);
        let result = TransformOp::SortBy("score".to_string())
            .apply(&data)
            .unwrap();
        // Items with field come first (numeric sort), missing last
        assert_eq!(result[0]["name"], "Charlie");
        assert_eq!(result[1]["name"], "Alice");
    }

    #[test]
    fn apply_sort_by_null_errors() {
        assert!(TransformOp::SortBy("x".to_string())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn parse_group_by() {
        let expr = TransformExpr::parse("group_by('locale')").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::GroupBy("locale".to_string())]
        );
    }

    #[test]
    fn apply_group_by_basic() {
        let data = json!([
            {"locale": "fr", "text": "Bonjour"},
            {"locale": "en", "text": "Hello"},
            {"locale": "fr", "text": "Merci"}
        ]);
        let result = TransformOp::GroupBy("locale".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result["fr"].as_array().unwrap().len(), 2);
        assert_eq!(result["en"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn apply_group_by_empty() {
        let result = TransformOp::GroupBy("x".to_string())
            .apply(&json!([]))
            .unwrap();
        assert_eq!(result, json!({}));
    }

    #[test]
    fn apply_group_by_null_errors() {
        assert!(TransformOp::GroupBy("x".to_string())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn apply_merge_basic() {
        let data = json!([{"a": 1}, {"b": 2}, {"c": 3}]);
        let result = TransformOp::Merge(None).apply(&data).unwrap();
        assert_eq!(result, json!({"a": 1, "b": 2, "c": 3}));
    }

    #[test]
    fn apply_merge_deep() {
        let data = json!([
            {"nested": {"x": 1}},
            {"nested": {"y": 2}}
        ]);
        let result = TransformOp::Merge(None).apply(&data).unwrap();
        assert_eq!(result["nested"]["x"], 1);
        assert_eq!(result["nested"]["y"], 2);
    }

    #[test]
    fn apply_merge_override() {
        let data = json!([{"a": 1, "b": "old"}, {"b": "new", "c": 3}]);
        let result = TransformOp::Merge(None).apply(&data).unwrap();
        assert_eq!(result, json!({"a": 1, "b": "new", "c": 3}));
    }

    #[test]
    fn apply_merge_empty_array() {
        let result = TransformOp::Merge(None).apply(&json!([])).unwrap();
        assert_eq!(result, json!({}));
    }

    #[test]
    fn apply_merge_non_objects_errors() {
        let data = json!([{"a": 1}, "not an object"]);
        assert!(TransformOp::Merge(None).apply(&data).is_err());
    }

    #[test]
    fn apply_merge_null_errors() {
        assert!(TransformOp::Merge(None).apply(&Value::Null).is_err());
    }

    #[test]
    fn parse_merge() {
        let expr = TransformExpr::parse("merge").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Merge(None)]);
    }

    // ─────────────────────────────────────────────────────────────
    // regex transform
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_regex() {
        let expr = TransformExpr::parse(r#"regex('\d+')"#).unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Regex(r"\d+".to_string())]
        );
    }

    #[test]
    fn apply_regex_match() {
        let result = TransformOp::Regex(r"\d+\.\d+".to_string())
            .apply(&json!("Price: $42.50"))
            .unwrap();
        assert_eq!(result, json!("42.50"));
    }

    #[test]
    fn apply_regex_integer() {
        let result = TransformOp::Regex(r"\d+".to_string())
            .apply(&json!("There are 42 items"))
            .unwrap();
        assert_eq!(result, json!("42"));
    }

    #[test]
    fn apply_regex_no_match() {
        let result = TransformOp::Regex(r"\d+".to_string())
            .apply(&json!("no numbers here"))
            .unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn apply_regex_invalid_pattern() {
        let result = TransformOp::Regex(r"[invalid".to_string()).apply(&json!("test"));
        assert!(result.is_err());
    }

    #[test]
    fn apply_regex_null_errors() {
        assert!(TransformOp::Regex(r"\d+".to_string())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn apply_regex_not_string_errors() {
        assert!(TransformOp::Regex(r"\d+".to_string())
            .apply(&json!(42))
            .is_err());
    }

    // ─────────────────────────────────────────────────────────────
    // base64 transforms
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_base64_encode() {
        let expr = TransformExpr::parse("base64_encode").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Base64Encode]);
    }

    #[test]
    fn parse_base64_decode() {
        let expr = TransformExpr::parse("base64_decode").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Base64Decode]);
    }

    #[test]
    fn apply_base64_encode() {
        let result = TransformOp::Base64Encode
            .apply(&json!("Hello, World!"))
            .unwrap();
        assert_eq!(result, json!("SGVsbG8sIFdvcmxkIQ=="));
    }

    #[test]
    fn apply_base64_decode() {
        let result = TransformOp::Base64Decode
            .apply(&json!("SGVsbG8sIFdvcmxkIQ=="))
            .unwrap();
        assert_eq!(result, json!("Hello, World!"));
    }

    #[test]
    fn apply_base64_roundtrip() {
        let original = json!("Nika 🦋 workflow engine");
        let encoded = TransformOp::Base64Encode.apply(&original).unwrap();
        let decoded = TransformOp::Base64Decode.apply(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn apply_base64_decode_invalid() {
        let result = TransformOp::Base64Decode.apply(&json!("not!!valid!!base64"));
        assert!(result.is_err());
    }

    #[test]
    fn apply_base64_encode_null_errors() {
        assert!(TransformOp::Base64Encode.apply(&Value::Null).is_err());
    }

    #[test]
    fn apply_base64_decode_null_errors() {
        assert!(TransformOp::Base64Decode.apply(&Value::Null).is_err());
    }

    // ─────────────────────────────────────────────────────────────
    // Pipeline tests with new transforms
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn pipeline_pluck_then_sort() {
        let data = json!([
            {"name": "Charlie", "age": 20},
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]);
        let expr = TransformExpr::parse("pluck('name') | sort").unwrap();
        let result = expr.apply(&data).unwrap();
        assert_eq!(result, json!(["Alice", "Bob", "Charlie"]));
    }

    #[test]
    fn pipeline_where_then_pluck() {
        let data = json!([
            {"name": "Alice", "status": "active"},
            {"name": "Bob", "status": "inactive"},
            {"name": "Charlie", "status": "active"}
        ]);
        let expr = TransformExpr::parse("where('status', 'active') | pluck('name')").unwrap();
        let result = expr.apply(&data).unwrap();
        assert_eq!(result, json!(["Alice", "Charlie"]));
    }

    #[test]
    fn pipeline_sort_by_then_pluck() {
        let data = json!([
            {"name": "Bob", "score": 85},
            {"name": "Alice", "score": 95},
            {"name": "Charlie", "score": 70}
        ]);
        let expr = TransformExpr::parse("sort_by('score') | pluck('name')").unwrap();
        let result = expr.apply(&data).unwrap();
        assert_eq!(result, json!(["Charlie", "Bob", "Alice"]));
    }

    #[test]
    fn pipeline_pluck_join() {
        let data = json!([{"name": "Alice"}, {"name": "Bob"}]);
        let expr = TransformExpr::parse("pluck('name') | join(', ')").unwrap();
        let result = expr.apply(&data).unwrap();
        assert_eq!(result, json!("Alice, Bob"));
    }

    #[test]
    fn pipeline_pick_then_to_json() {
        let data = json!({"name": "Alice", "age": 30, "secret": "xxx"});
        let expr = TransformExpr::parse("pick('name', 'age') | to_json").unwrap();
        let result = expr.apply(&data).unwrap();
        assert!(result.as_str().unwrap().contains("name"));
        assert!(!result.as_str().unwrap().contains("secret"));
    }

    #[test]
    fn pipeline_base64_roundtrip() {
        let expr = TransformExpr::parse("base64_encode | base64_decode").unwrap();
        let result = expr.apply(&json!("test data 🦋")).unwrap();
        assert_eq!(result, json!("test data 🦋"));
    }

    // ─────────────────────────────────────────────────────────────
    // Display for new transforms
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn display_new_transforms() {
        assert_eq!(
            TransformOp::Pluck("name".to_string()).to_string(),
            "pluck('name')"
        );
        assert_eq!(
            TransformOp::Where("status".to_string(), "eq".to_string(), json!("active")).to_string(),
            "where('status', \"active\")"
        );
        assert_eq!(
            TransformOp::Pick(vec!["a".to_string(), "b".to_string()]).to_string(),
            "pick('a', 'b')"
        );
        assert_eq!(
            TransformOp::Omit(vec!["x".to_string()]).to_string(),
            "omit('x')"
        );
        assert_eq!(
            TransformOp::SortBy("age".to_string()).to_string(),
            "sort_by('age')"
        );
        assert_eq!(
            TransformOp::GroupBy("locale".to_string()).to_string(),
            "group_by('locale')"
        );
        assert_eq!(TransformOp::Merge(None).to_string(), "merge");
        assert_eq!(
            TransformOp::Regex(r"\d+".to_string()).to_string(),
            r"regex('\d+')"
        );
        assert_eq!(TransformOp::Base64Encode.to_string(), "base64_encode");
        assert_eq!(TransformOp::Base64Decode.to_string(), "base64_decode");
    }

    // ─────────────────────────────────────────────────────────────
    // Edge case tests (code review findings)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn where_with_boolean_value() {
        let data = json!([
            {"name": "Alice", "active": true},
            {"name": "Bob", "active": false},
            {"name": "Charlie", "active": true}
        ]);
        let result = TransformOp::Where("active".to_string(), "eq".to_string(), json!(true))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
        assert_eq!(result[0]["name"], "Alice");
        assert_eq!(result[1]["name"], "Charlie");
    }

    #[test]
    fn where_with_numeric_value() {
        let data = json!([
            {"id": 1, "score": 90},
            {"id": 2, "score": 80},
            {"id": 3, "score": 90}
        ]);
        let result = TransformOp::Where("score".to_string(), "eq".to_string(), json!(90))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn pluck_nested_field_with_dot_path() {
        // pluck supports dot-paths for nested field access
        let data = json!([{"a": {"b": 1}}, {"a": {"b": 2}}]);
        let result = TransformOp::Pluck("a.b".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!([1, 2]));
    }

    #[test]
    fn pluck_top_level_field() {
        let data = json!([{"a": {"b": 1}}, {"a": {"b": 2}}]);
        let result = TransformOp::Pluck("b".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!([]), "top-level 'b' doesn't exist");
    }

    #[test]
    fn group_by_numeric_field() {
        // group_by with numeric values converts them to string keys
        let data = json!([{"score": 90}, {"score": 80}, {"score": 90}]);
        let result = TransformOp::GroupBy("score".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result["90"].as_array().unwrap().len(), 2);
        assert_eq!(result["80"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn group_by_missing_field() {
        // items without the field get grouped under "null"
        let data = json!([{"a": 1}, {"b": 2}]);
        let result = TransformOp::GroupBy("a".to_string()).apply(&data).unwrap();
        assert!(result.get("1").is_some());
        assert!(result.get("null").is_some());
    }

    #[test]
    fn pick_preserves_order() {
        let data = json!({"z": 3, "a": 1, "m": 2});
        let result = TransformOp::Pick(vec!["a".to_string(), "z".to_string()])
            .apply(&data)
            .unwrap();
        let keys: Vec<&String> = result.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["a", "z"]);
    }

    #[test]
    fn sort_by_stable_on_equal_values() {
        let data = json!([
            {"name": "Alice", "score": 90},
            {"name": "Bob", "score": 90},
            {"name": "Charlie", "score": 90}
        ]);
        let result = TransformOp::SortBy("score".to_string())
            .apply(&data)
            .unwrap();
        // All scores equal — order should be preserved (stable sort)
        assert_eq!(result[0]["name"], "Alice");
        assert_eq!(result[1]["name"], "Bob");
        assert_eq!(result[2]["name"], "Charlie");
    }

    #[test]
    fn merge_single_object() {
        let data = json!([{"a": 1, "b": 2}]);
        let result = TransformOp::Merge(None).apply(&data).unwrap();
        assert_eq!(result, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn regex_captures_first_match_only() {
        let result = TransformOp::Regex(r"\d+".to_string())
            .apply(&json!("item1 item2 item3"))
            .unwrap();
        assert_eq!(result, json!("1"), "regex returns first match only");
    }

    #[test]
    fn base64_encode_empty_string() {
        let result = TransformOp::Base64Encode.apply(&json!("")).unwrap();
        assert_eq!(result, json!(""));
    }

    #[test]
    fn base64_decode_empty_string() {
        let result = TransformOp::Base64Decode.apply(&json!("")).unwrap();
        assert_eq!(result, json!(""));
    }

    // ─────────────────────────────────────────────────────────────
    // FIX-1: merge parametric form
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn merge_parametric_basic() {
        let base = json!({"a": 1, "b": 2});
        let overlay = json!({"c": 3});
        let result = TransformOp::Merge(Some(overlay)).apply(&base).unwrap();
        assert_eq!(result, json!({"a": 1, "b": 2, "c": 3}));
    }

    #[test]
    fn merge_parametric_override() {
        let base = json!({"a": 1, "b": "old"});
        let overlay = json!({"b": "new", "c": 3});
        let result = TransformOp::Merge(Some(overlay)).apply(&base).unwrap();
        assert_eq!(result, json!({"a": 1, "b": "new", "c": 3}));
    }

    #[test]
    fn merge_parametric_deep() {
        let base = json!({"nested": {"x": 1, "y": 2}});
        let overlay = json!({"nested": {"z": 3}});
        let result = TransformOp::Merge(Some(overlay)).apply(&base).unwrap();
        assert_eq!(result["nested"]["x"], 1);
        assert_eq!(result["nested"]["y"], 2);
        assert_eq!(result["nested"]["z"], 3);
    }

    #[test]
    fn merge_parametric_non_object_input_errors() {
        assert!(TransformOp::Merge(Some(json!({"a": 1})))
            .apply(&json!([1, 2]))
            .is_err());
    }

    #[test]
    fn parse_merge_parametric() {
        let expr = TransformExpr::parse(r#"merge({"key": "val"})"#).unwrap();
        match &expr.ops[0] {
            TransformOp::Merge(Some(v)) => assert_eq!(v, &json!({"key": "val"})),
            _ => panic!("expected Merge(Some(...))"),
        }
    }

    #[test]
    fn display_merge_parametric() {
        let s = TransformOp::Merge(Some(json!({"a": 1}))).to_string();
        assert!(s.starts_with("merge("), "should display as merge(...)");
    }

    // ─────────────────────────────────────────────────────────────
    // FIX-2: where operators
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn where_gt_operator() {
        let data = json!([
            {"name": "A", "score": 90},
            {"name": "B", "score": 40},
            {"name": "C", "score": 75}
        ]);
        let result = TransformOp::Where("score".to_string(), "gt".to_string(), json!(70))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
        assert_eq!(result[0]["name"], "A");
        assert_eq!(result[1]["name"], "C");
    }

    #[test]
    fn where_lt_operator() {
        let data = json!([{"v": 1}, {"v": 5}, {"v": 10}]);
        let result = TransformOp::Where("v".to_string(), "lt".to_string(), json!(6))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn where_ne_operator() {
        let data = json!([{"s": "active"}, {"s": "deleted"}, {"s": "active"}]);
        let result = TransformOp::Where("s".to_string(), "ne".to_string(), json!("deleted"))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn where_contains_operator() {
        let data = json!([
            {"label": "hello world"},
            {"label": "goodbye"},
            {"label": "hello there"}
        ]);
        let result =
            TransformOp::Where("label".to_string(), "contains".to_string(), json!("hello"))
                .apply(&data)
                .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn where_gte_lte_operators() {
        let data = json!([{"v": 1}, {"v": 5}, {"v": 10}]);
        let gte = TransformOp::Where("v".to_string(), "gte".to_string(), json!(5))
            .apply(&data)
            .unwrap();
        assert_eq!(gte.as_array().unwrap().len(), 2); // 5, 10

        let lte = TransformOp::Where("v".to_string(), "lte".to_string(), json!(5))
            .apply(&data)
            .unwrap();
        assert_eq!(lte.as_array().unwrap().len(), 2); // 1, 5
    }

    #[test]
    fn parse_where_3_args() {
        let expr = TransformExpr::parse("where('score', 'gt', 80)").unwrap();
        assert_eq!(
            expr.ops.as_slice(),
            &[TransformOp::Where(
                "score".to_string(),
                "gt".to_string(),
                json!(80)
            )]
        );
    }

    #[test]
    fn parse_where_invalid_operator() {
        let result = TransformExpr::parse("where('score', 'invalid_op', 80)");
        assert!(result.is_err());
    }

    // ─────────────────────────────────────────────────────────────
    // FIX-5: dot-path support
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn where_dot_path() {
        let data = json!([
            {"meta": {"score": 90}},
            {"meta": {"score": 40}},
            {"meta": {"score": 75}}
        ]);
        let result = TransformOp::Where("meta.score".to_string(), "gt".to_string(), json!(70))
            .apply(&data)
            .unwrap();
        assert_eq!(result.as_array().unwrap().len(), 2);
    }

    #[test]
    fn sort_by_dot_path() {
        let data = json!([
            {"info": {"age": 30}},
            {"info": {"age": 20}},
            {"info": {"age": 25}}
        ]);
        let result = TransformOp::SortBy("info.age".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result[0]["info"]["age"], 20);
        assert_eq!(result[1]["info"]["age"], 25);
        assert_eq!(result[2]["info"]["age"], 30);
    }

    #[test]
    fn group_by_dot_path() {
        let data = json!([
            {"user": {"role": "admin"}, "id": 1},
            {"user": {"role": "user"}, "id": 2},
            {"user": {"role": "admin"}, "id": 3}
        ]);
        let result = TransformOp::GroupBy("user.role".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result["admin"].as_array().unwrap().len(), 2);
        assert_eq!(result["user"].as_array().unwrap().len(), 1);
    }

    // ─────────────────────────────────────────────────────────────
    // FIX-6: regex cache (functional — verify same results)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn regex_cache_consistency() {
        // Same pattern applied multiple times should produce consistent results
        let pattern = r"\d+".to_string();
        let r1 = TransformOp::Regex(pattern.clone())
            .apply(&json!("abc123"))
            .unwrap();
        let r2 = TransformOp::Regex(pattern).apply(&json!("xyz456")).unwrap();
        assert_eq!(r1, json!("123"));
        assert_eq!(r2, json!("456"));
    }

    // ─────────────────────────────────────────────────────────────
    // FEAT-4: jq() transform
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn jq_identity() {
        let data = json!({"a": 1, "b": 2});
        let result = TransformOp::Jq(".".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn jq_field_access() {
        let data = json!({"name": "Alice", "age": 30});
        let result = TransformOp::Jq(".name".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!("Alice"));
    }

    #[test]
    fn jq_array_index() {
        let data = json!([10, 20, 30]);
        let result = TransformOp::Jq(".[1]".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!(20));
    }

    #[test]
    fn jq_nested_access() {
        let data = json!({"user": {"address": {"city": "Paris"}}});
        let result = TransformOp::Jq(".user.address.city".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!("Paris"));
    }

    #[test]
    fn jq_object_construction() {
        let data = json!({"first": "Alice", "last": "Smith", "age": 30});
        let result = TransformOp::Jq("{name: .first, years: .age}".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!({"name": "Alice", "years": 30}));
    }

    #[test]
    fn jq_arithmetic() {
        let data = json!({"a": 10, "b": 3});
        let result = TransformOp::Jq(".a + .b".to_string()).apply(&data).unwrap();
        assert_eq!(result, json!(13));
    }

    #[test]
    fn jq_map_expression() {
        let data = json!([1, 2, 3]);
        let result = TransformOp::Jq("[.[] + 10]".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!([11, 12, 13]));
    }

    #[test]
    fn jq_null_input() {
        let result = TransformOp::Jq(".".to_string())
            .apply(&Value::Null)
            .unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn jq_stdlib_group_by() {
        let data = json!([
            {"locale": "en", "section": "blog"},
            {"locale": "en", "section": "docs"},
            {"locale": "fr", "section": "blog"}
        ]);
        let result = TransformOp::Jq(
            "[group_by(.locale)[] | {name: .[0].locale, count: length}]".to_string(),
        )
        .apply(&data)
        .unwrap();
        assert_eq!(
            result,
            json!([{"name": "en", "count": 2}, {"name": "fr", "count": 1}])
        );
    }

    #[test]
    fn jq_stdlib_map_select() {
        let data = json!([1, 2, 3, 4, 5]);
        let result = TransformOp::Jq("[.[] | select(. > 3)]".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!([4, 5]));
    }

    #[test]
    fn jq_stdlib_keys_length() {
        let data = json!({"a": 1, "b": 2, "c": 3});
        let result = TransformOp::Jq("keys | length".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!(3));
    }

    #[test]
    fn jq_stdlib_to_entries() {
        let data = json!({"name": "Alice", "age": 30});
        let result = TransformOp::Jq("to_entries | length".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!(2));
    }

    #[test]
    fn jq_stdlib_sort_by() {
        let data = json!([{"n": 3}, {"n": 1}, {"n": 2}]);
        let result = TransformOp::Jq("[sort_by(.n)[] | .n]".to_string())
            .apply(&data)
            .unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn jq_stdlib_nested_group_by() {
        // The exact pattern needed for site-audit dashboard TREE
        let data = json!([
            {"locale": "en", "section": "blog"},
            {"locale": "en", "section": "blog"},
            {"locale": "en", "section": "docs"},
            {"locale": "fr", "section": "blog"}
        ]);
        let result = TransformOp::Jq(
            "[group_by(.locale)[] | {name: .[0].locale, children: [group_by(.section)[] | {name: .[0].section, value: length}]}]".to_string()
        ).apply(&data).unwrap();
        assert_eq!(
            result,
            json!([
                {"name": "en", "children": [{"name": "blog", "value": 2}, {"name": "docs", "value": 1}]},
                {"name": "fr", "children": [{"name": "blog", "value": 1}]}
            ])
        );
    }

    #[test]
    fn jq_parse_error() {
        let result = TransformOp::Jq("[invalid!!!".to_string()).apply(&json!(1));
        assert!(result.is_err());
    }

    #[test]
    fn parse_jq() {
        let expr = TransformExpr::parse("jq('.name')").unwrap();
        assert_eq!(expr.ops.as_slice(), &[TransformOp::Jq(".name".to_string())]);
    }

    #[test]
    fn display_jq() {
        assert_eq!(
            TransformOp::Jq(".name".to_string()).to_string(),
            "jq('.name')"
        );
    }

    #[test]
    fn jq_regex_on_null_no_panic() {
        // jaq 1.5.x panicked on test("x") with null input.
        // jaq 3.x should return an error, not panic.
        let result = eval_jq("test(\"foo\")", &Value::Null);
        assert!(
            result.is_err(),
            "regex test() on null should error, not panic"
        );
    }

    // ── replace ─────────────────────────────────────────────

    #[test]
    fn replace_basic() {
        let val = json!("hello world");
        let result = TransformOp::Replace("world".into(), "rust".into())
            .apply(&val)
            .unwrap();
        assert_eq!(result, json!("hello rust"));
    }

    #[test]
    fn replace_multiple_occurrences() {
        let val = json!("aaa");
        let result = TransformOp::Replace("a".into(), "bb".into())
            .apply(&val)
            .unwrap();
        assert_eq!(result, json!("bbbbbb"));
    }

    #[test]
    fn replace_no_match() {
        let val = json!("hello");
        let result = TransformOp::Replace("xyz".into(), "abc".into())
            .apply(&val)
            .unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn replace_to_empty() {
        let val = json!("remove-dashes");
        let result = TransformOp::Replace("-".into(), "".into())
            .apply(&val)
            .unwrap();
        assert_eq!(result, json!("removedashes"));
    }

    #[test]
    fn replace_null_fails() {
        assert!(TransformOp::Replace("a".into(), "b".into())
            .apply(&Value::Null)
            .is_err());
    }

    #[test]
    fn replace_non_string_fails() {
        assert!(TransformOp::Replace("a".into(), "b".into())
            .apply(&json!(42))
            .is_err());
    }

    #[test]
    fn replace_parse() {
        let expr = TransformExpr::parse("replace('hello', 'world')").unwrap();
        assert_eq!(expr.ops.len(), 1);
        assert_eq!(
            expr.ops[0],
            TransformOp::Replace("hello".into(), "world".into())
        );
    }

    #[test]
    fn replace_display() {
        assert_eq!(
            TransformOp::Replace("a".into(), "b".into()).to_string(),
            "replace('a', 'b')"
        );
    }

    // ── truncate ────────────────────────────────────────────

    #[test]
    fn truncate_basic() {
        let val = json!("hello world");
        let result = TransformOp::Truncate(5).apply(&val).unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn truncate_longer_than_string() {
        let val = json!("hi");
        let result = TransformOp::Truncate(100).apply(&val).unwrap();
        assert_eq!(result, json!("hi"));
    }

    #[test]
    fn truncate_zero() {
        let val = json!("hello");
        let result = TransformOp::Truncate(0).apply(&val).unwrap();
        assert_eq!(result, json!(""));
    }

    #[test]
    fn truncate_unicode() {
        let val = json!("héllo wörld");
        let result = TransformOp::Truncate(5).apply(&val).unwrap();
        assert_eq!(result, json!("héllo"));
    }

    #[test]
    fn truncate_null_fails() {
        assert!(TransformOp::Truncate(5).apply(&Value::Null).is_err());
    }

    #[test]
    fn truncate_parse() {
        let expr = TransformExpr::parse("truncate(10)").unwrap();
        assert_eq!(expr.ops.len(), 1);
        assert_eq!(expr.ops[0], TransformOp::Truncate(10));
    }

    // ── add ─────────────────────────────────────────────────

    #[test]
    fn add_numbers() {
        let val = json!([1, 2, 3, 4, 5]);
        let result = TransformOp::Add.apply(&val).unwrap();
        assert_eq!(result, json!(15));
    }

    #[test]
    fn add_floats() {
        let val = json!([1.5, 2.3]);
        let result = TransformOp::Add.apply(&val).unwrap();
        // 1.5 + 2.3 = 3.8 — has fractional part, stays float
        assert_eq!(result.as_f64().unwrap(), 3.8);
    }

    #[test]
    fn add_strings() {
        let val = json!(["hello", " ", "world"]);
        let result = TransformOp::Add.apply(&val).unwrap();
        assert_eq!(result, json!("hello world"));
    }

    #[test]
    fn add_arrays() {
        let val = json!([[1, 2], [3, 4]]);
        let result = TransformOp::Add.apply(&val).unwrap();
        assert_eq!(result, json!([1, 2, 3, 4]));
    }

    #[test]
    fn add_empty_array() {
        let val = json!([]);
        let result = TransformOp::Add.apply(&val).unwrap();
        // Empty array → null (consistent with min/max/avg)
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn add_with_nulls_skipped() {
        let val = json!([1, null, 3]);
        let result = TransformOp::Add.apply(&val).unwrap();
        assert_eq!(result, json!(4));
    }

    #[test]
    fn add_null_propagates() {
        assert_eq!(TransformOp::Add.apply(&Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn add_non_array_fails() {
        assert!(TransformOp::Add.apply(&json!("not an array")).is_err());
    }

    #[test]
    fn add_mixed_types_fails() {
        assert!(TransformOp::Add.apply(&json!([1, "two"])).is_err());
    }

    // ── min ─────────────────────────────────────────────────

    #[test]
    fn min_basic() {
        let val = json!([3, 1, 4, 1, 5]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(1));
    }

    #[test]
    fn min_floats() {
        let val = json!([3.16, 2.73, 1.41]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(1.41));
    }

    #[test]
    fn min_single_element() {
        let val = json!([42]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(42));
    }

    #[test]
    fn min_empty_array() {
        let val = json!([]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn min_with_nulls() {
        let val = json!([5, null, 2, null]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(2));
    }

    #[test]
    fn min_all_nulls() {
        let val = json!([null, null]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn min_null_propagates() {
        assert_eq!(TransformOp::Min.apply(&Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn min_negative() {
        let val = json!([-10, -5, -20]);
        let result = TransformOp::Min.apply(&val).unwrap();
        assert_eq!(result, json!(-20));
    }

    // ── max ─────────────────────────────────────────────────

    #[test]
    fn max_basic() {
        let val = json!([3, 1, 4, 1, 5]);
        let result = TransformOp::Max.apply(&val).unwrap();
        assert_eq!(result, json!(5));
    }

    #[test]
    fn max_floats() {
        let val = json!([3.16, 2.73, 1.41]);
        let result = TransformOp::Max.apply(&val).unwrap();
        assert_eq!(result, json!(3.16));
    }

    #[test]
    fn max_empty_array() {
        let val = json!([]);
        let result = TransformOp::Max.apply(&val).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn max_with_nulls() {
        let val = json!([null, 3, null, 7]);
        let result = TransformOp::Max.apply(&val).unwrap();
        assert_eq!(result, json!(7));
    }

    #[test]
    fn max_null_propagates() {
        assert_eq!(TransformOp::Max.apply(&Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn max_non_array_fails() {
        assert!(TransformOp::Max.apply(&json!(42)).is_err());
    }

    // ── not ─────────────────────────────────────────────────

    #[test]
    fn not_true() {
        assert_eq!(TransformOp::Not.apply(&json!(true)).unwrap(), json!(false));
    }

    #[test]
    fn not_false() {
        assert_eq!(TransformOp::Not.apply(&json!(false)).unwrap(), json!(true));
    }

    #[test]
    fn not_null_propagates() {
        assert_eq!(TransformOp::Not.apply(&Value::Null).unwrap(), Value::Null);
    }

    #[test]
    fn not_non_bool_fails() {
        assert!(TransformOp::Not.apply(&json!("true")).is_err());
        assert!(TransformOp::Not.apply(&json!(1)).is_err());
    }

    // ── parsing and chaining ────────────────────────────────

    #[test]
    fn parse_add_min_max_not() {
        assert_eq!(
            TransformExpr::parse("add").unwrap().ops[0],
            TransformOp::Add
        );
        assert_eq!(
            TransformExpr::parse("min").unwrap().ops[0],
            TransformOp::Min
        );
        assert_eq!(
            TransformExpr::parse("max").unwrap().ops[0],
            TransformOp::Max
        );
        assert_eq!(
            TransformExpr::parse("not").unwrap().ops[0],
            TransformOp::Not
        );
    }

    #[test]
    fn chain_add_round() {
        let val = json!([1.1, 2.2, 3.3]);
        let expr = TransformExpr::parse("add | round").unwrap();
        let result = expr.apply(&val).unwrap();
        assert_eq!(result, json!(7));
    }

    #[test]
    fn chain_replace_upper() {
        let val = json!("hello world");
        let expr = TransformExpr::parse("replace('world', 'rust') | upper").unwrap();
        let result = expr.apply(&val).unwrap();
        assert_eq!(result, json!("HELLO RUST"));
    }

    #[test]
    fn chain_pluck_add() {
        let val = json!([{"score": 10}, {"score": 20}, {"score": 30}]);
        let expr = TransformExpr::parse("pluck('score') | add").unwrap();
        let result = expr.apply(&val).unwrap();
        assert_eq!(result, json!(60));
    }

    #[test]
    fn display_v069_transforms() {
        assert_eq!(TransformOp::Add.to_string(), "add");
        assert_eq!(TransformOp::Min.to_string(), "min");
        assert_eq!(TransformOp::Max.to_string(), "max");
        assert_eq!(TransformOp::Not.to_string(), "not");
        assert_eq!(TransformOp::Truncate(10).to_string(), "truncate(10)");
        assert_eq!(
            TransformOp::Replace("a".into(), "b".into()).to_string(),
            "replace('a', 'b')"
        );
        assert_eq!(TransformOp::Sum.to_string(), "sum");
        assert_eq!(TransformOp::Avg.to_string(), "avg");
        assert_eq!(
            TransformOp::MinBy("score".into()).to_string(),
            "min_by('score')"
        );
        assert_eq!(
            TransformOp::MaxBy("score".into()).to_string(),
            "max_by('score')"
        );
        assert_eq!(TransformOp::Has("name".into()).to_string(), "has('name')");
    }

    // ── min_by / max_by ─────────────────────────────────────

    #[test]
    fn min_by_basic() {
        let val = json!([{"name": "a", "score": 30}, {"name": "b", "score": 10}, {"name": "c", "score": 20}]);
        let result = TransformOp::MinBy("score".into()).apply(&val).unwrap();
        assert_eq!(result["name"], json!("b"));
        assert_eq!(result["score"], json!(10));
    }

    #[test]
    fn max_by_basic() {
        let val = json!([{"name": "a", "score": 30}, {"name": "b", "score": 10}]);
        let result = TransformOp::MaxBy("score".into()).apply(&val).unwrap();
        assert_eq!(result["name"], json!("a"));
    }

    #[test]
    fn min_by_empty_array() {
        assert_eq!(
            TransformOp::MinBy("x".into()).apply(&json!([])).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn max_by_null_propagates() {
        assert_eq!(
            TransformOp::MaxBy("x".into()).apply(&Value::Null).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn min_by_dot_path() {
        let val = json!([{"meta": {"score": 5}}, {"meta": {"score": 2}}]);
        let result = TransformOp::MinBy("meta.score".into()).apply(&val).unwrap();
        assert_eq!(result["meta"]["score"], json!(2));
    }

    // ── sum / avg ───────────────────────────────────────────

    #[test]
    fn sum_numeric_only() {
        let val = json!([10, 20, 30]);
        assert_eq!(TransformOp::Sum.apply(&val).unwrap(), json!(60));
    }

    #[test]
    fn sum_rejects_strings() {
        // sum is numeric-only (unlike add which also concats strings)
        let val = json!(["a", "b", "c"]);
        assert!(TransformOp::Sum.apply(&val).is_err());
    }

    #[test]
    fn sum_rejects_arrays() {
        // sum is numeric-only (unlike add which also concats arrays)
        let val = json!([[1], [2], [3]]);
        assert!(TransformOp::Sum.apply(&val).is_err());
    }

    #[test]
    fn sum_null_and_empty() {
        assert_eq!(TransformOp::Sum.apply(&Value::Null).unwrap(), Value::Null);
        assert_eq!(TransformOp::Sum.apply(&json!([])).unwrap(), Value::Null);
    }

    #[test]
    fn sum_with_nulls_skips_them() {
        let val = json!([10, null, 20]);
        assert_eq!(TransformOp::Sum.apply(&val).unwrap(), json!(30));
    }

    #[test]
    fn add_still_concats_strings() {
        // add retains its polymorphic behavior
        let val = json!(["hello", " ", "world"]);
        assert_eq!(TransformOp::Add.apply(&val).unwrap(), json!("hello world"));
    }

    #[test]
    fn avg_basic() {
        let val = json!([10, 20, 30]);
        let result = TransformOp::Avg.apply(&val).unwrap();
        assert_eq!(result.as_f64().unwrap(), 20.0);
    }

    #[test]
    fn avg_with_nulls() {
        let val = json!([10, null, 20]);
        let result = TransformOp::Avg.apply(&val).unwrap();
        assert_eq!(result.as_f64().unwrap(), 15.0); // only 2 numbers
    }

    #[test]
    fn avg_empty_array() {
        assert_eq!(TransformOp::Avg.apply(&json!([])).unwrap(), Value::Null);
    }

    #[test]
    fn avg_null_propagates() {
        assert_eq!(TransformOp::Avg.apply(&Value::Null).unwrap(), Value::Null);
    }

    // ── has ─────────────────────────────────────────────────

    #[test]
    fn has_key_present() {
        let val = json!({"name": "Alice", "age": 30});
        assert_eq!(
            TransformOp::Has("name".into()).apply(&val).unwrap(),
            json!(true)
        );
    }

    #[test]
    fn has_key_absent() {
        let val = json!({"name": "Alice"});
        assert_eq!(
            TransformOp::Has("age".into()).apply(&val).unwrap(),
            json!(false)
        );
    }

    #[test]
    fn has_null_propagates() {
        assert_eq!(
            TransformOp::Has("x".into()).apply(&Value::Null).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn has_non_object_fails() {
        assert!(TransformOp::Has("x".into()).apply(&json!([1, 2])).is_err());
    }

    // ── parsing ─────────────────────────────────────────────

    #[test]
    fn parse_new_s6_transforms() {
        assert_eq!(
            TransformExpr::parse("sum").unwrap().ops[0],
            TransformOp::Sum
        );
        assert_eq!(
            TransformExpr::parse("avg").unwrap().ops[0],
            TransformOp::Avg
        );
        assert_eq!(
            TransformExpr::parse("min_by('score')").unwrap().ops[0],
            TransformOp::MinBy("score".into())
        );
        assert_eq!(
            TransformExpr::parse("max_by('score')").unwrap().ops[0],
            TransformOp::MaxBy("score".into())
        );
        assert_eq!(
            TransformExpr::parse("has('name')").unwrap().ops[0],
            TransformOp::Has("name".into())
        );
    }

    #[test]
    fn chain_pluck_avg() {
        let val = json!([{"score": 10}, {"score": 20}, {"score": 30}]);
        let expr = TransformExpr::parse("pluck('score') | avg").unwrap();
        let result = expr.apply(&val).unwrap();
        assert_eq!(result.as_f64().unwrap(), 20.0);
    }

// ═══════════════════════════════════════════════════════════════
// Property-based tests (proptest)
// ═══════════════════════════════════════════════════════════════

mod proptest_tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::{json, Number, Value};

    /// Strategy for arbitrary JSON values (bounded depth)
    fn arb_json_value() -> impl Strategy<Value = Value> {
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i64>().prop_map(|n| Value::Number(n.into())),
            any::<f64>()
                .prop_filter("finite", |f| f.is_finite())
                .prop_map(|f| {
                    Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                }),
            "\\PC{0,100}".prop_map(Value::String),
        ];
        leaf.prop_recursive(
            3,  // depth
            64, // max nodes
            8,  // items per collection
            |inner| {
                prop_oneof![
                    prop::collection::vec(inner.clone(), 0..8).prop_map(Value::Array),
                    prop::collection::hash_map("\\PC{1,20}", inner, 0..5)
                        .prop_map(|m| Value::Object(m.into_iter().collect())),
                ]
            },
        )
    }

    /// All simple (non-parameterized) transform op names
    fn arb_simple_transform_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("upper".into()),
            Just("lower".into()),
            Just("trim".into()),
            Just("trim_start".into()),
            Just("trim_end".into()),
            Just("length".into()),
            Just("first".into()),
            Just("last".into()),
            Just("keys".into()),
            Just("values".into()),
            Just("flatten".into()),
            Just("reverse".into()),
            Just("sort".into()),
            Just("unique".into()),
            Just("compact".into()),
            Just("to_string".into()),
            Just("to_number".into()),
            Just("to_bool".into()),
            Just("to_json".into()),
            Just("parse_json".into()),
            Just("parse_yaml".into()),
            Just("round".into()),
            Just("abs".into()),
            Just("ceil".into()),
            Just("floor".into()),
            Just("type_of".into()),
            Just("shell".into()),
        ]
    }

    /// Parameterized transform names
    fn arb_param_transform_name() -> impl Strategy<Value = String> {
        prop_oneof![
            (0usize..100).prop_map(|n| format!("first({})", n)),
            (0usize..100).prop_map(|n| format!("last({})", n)),
            (0u32..10).prop_map(|n| format!("round({})", n)),
            "\\PC{0,20}".prop_map(|s| format!("join('{}')", s.replace('\'', ""))),
            "\\PC{0,20}".prop_map(|s| format!("split('{}')", s.replace('\'', ""))),
            "\\PC{0,20}".prop_map(|s| format!("default('{}')", s.replace('\'', ""))),
        ]
    }

    proptest! {
        // ── Property 1: No transform panics on any JSON value ──
        #[test]
        fn transform_never_panics(
            value in arb_json_value(),
            op_name in prop_oneof![arb_simple_transform_name(), arb_param_transform_name()]
        ) {
            if let Ok(expr) = TransformExpr::parse(&op_name) {
                let _ = expr.apply(&value); // MUST NOT panic — Err is fine
            }
        }

        // ── Property 2: Null input on failing transforms returns NullInput error ──
        #[test]
        fn null_on_failing_transform_returns_error(
            op_name in prop_oneof![
                Just("upper".to_string()), Just("lower".to_string()),
                Just("trim".to_string()), Just("trim_start".to_string()),
                Just("trim_end".to_string()), Just("first".to_string()),
                Just("last".to_string()),
                Just("flatten".to_string()), Just("reverse".to_string()),
                Just("sort".to_string()), Just("unique".to_string()),
                Just("compact".to_string()), Just("to_number".to_string()),
                Just("to_bool".to_string()), Just("parse_json".to_string()), Just("parse_yaml".to_string()),
                Just("round".to_string()), Just("abs".to_string()),
                Just("ceil".to_string()), Just("floor".to_string()),
                Just("join(',')".to_string()), Just("split(',')".to_string()),
            ]
        ) {
            let expr = TransformExpr::parse(&op_name).unwrap();
            let result = expr.apply(&Value::Null);
            prop_assert!(result.is_err(), "Expected error for {} on null, got {:?}", op_name, result);
            match result {
                Err(TransformError::NullInput { .. }) => {} // Expected
                other => prop_assert!(false, "Expected NullInput for {}, got {:?}", op_name, other),
            }
        }

        // ── Property 3: Propagating transforms on null return ok ──
        #[test]
        fn null_on_propagating_transform_returns_ok(
            op_name in prop_oneof![
                Just("length".to_string()), Just("keys".to_string()),
                Just("values".to_string()),
                Just("to_string".to_string()), Just("to_json".to_string()),
                Just("type_of".to_string()),
            ]
        ) {
            let expr = TransformExpr::parse(&op_name).unwrap();
            let result = expr.apply(&Value::Null);
            prop_assert!(result.is_ok(), "Expected ok for {} on null, got {:?}", op_name, result);
        }

        // ── Property 4: default() always returns non-null ──
        #[test]
        fn default_on_null_returns_non_null(
            default_val in "[a-zA-Z0-9 ]{0,30}"
        ) {
            let expr_str = format!("default('{}')", default_val);
            if let Ok(expr) = TransformExpr::parse(&expr_str) {
                let result = expr.apply(&Value::Null);
                prop_assert!(result.is_ok(), "default should always succeed");
                prop_assert!(!result.unwrap().is_null(), "default on null must not return null");
            }
        }

        // ── Property 5: shell escape always wraps in single quotes ──
        #[test]
        fn shell_escape_always_single_quoted(input in "\\PC{0,100}") {
            let expr = TransformExpr::parse("shell").unwrap();
            let result = expr.apply(&Value::String(input)).unwrap();
            if let Value::String(s) = result {
                prop_assert!(s.starts_with('\''), "shell escape must start with '");
                prop_assert!(s.ends_with('\''), "shell escape must end with '");
            } else {
                prop_assert!(false, "shell should return a string");
            }
        }

        // ── Property 6: sort is idempotent ──
        #[test]
        fn sort_is_idempotent(items in prop::collection::vec(any::<i64>(), 0..20)) {
            let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
            let expr = TransformExpr::parse("sort").unwrap();
            let once = expr.apply(&arr).unwrap();
            let twice = expr.apply(&once).unwrap();
            prop_assert_eq!(once, twice);
        }

        // ── Property 7: unique is idempotent ──
        #[test]
        fn unique_is_idempotent(items in prop::collection::vec(0i64..10, 0..20)) {
            let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
            let expr = TransformExpr::parse("unique").unwrap();
            let once = expr.apply(&arr).unwrap();
            let twice = expr.apply(&once).unwrap();
            prop_assert_eq!(once, twice);
        }

        // ── Property 8: reverse is involution (f(f(x)) == x) ──
        #[test]
        fn reverse_is_involution(items in prop::collection::vec(any::<i64>(), 0..20)) {
            let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
            let expr = TransformExpr::parse("reverse").unwrap();
            let once = expr.apply(&arr).unwrap();
            let twice = expr.apply(&once).unwrap();
            prop_assert_eq!(arr, twice);
        }

        // ── Property 9: compact removes all nulls and empty strings ──
        #[test]
        fn compact_no_nulls_or_empty(items in prop::collection::vec(
            prop_oneof![
                Just(Value::Null),
                any::<i64>().prop_map(|n| json!(n)),
                Just(json!("hello")),
                Just(json!("")),
            ],
            0..20
        )) {
            let arr = Value::Array(items);
            let result = TransformExpr::parse("compact").unwrap().apply(&arr).unwrap();
            if let Value::Array(ref compacted) = result {
                for v in compacted {
                    prop_assert!(!v.is_null(), "compact must remove nulls");
                    prop_assert!(v != &json!(""), "compact must remove empty strings");
                }
            }
        }

        // ── Property 10: to_json then parse_json roundtrip for integers ──
        #[test]
        fn to_json_parse_json_roundtrip(n in any::<i64>()) {
            let val = json!(n);
            let as_json = TransformExpr::parse("to_json").unwrap().apply(&val).unwrap();
            let back = TransformExpr::parse("parse_json").unwrap().apply(&as_json).unwrap();
            prop_assert_eq!(val, back);
        }

        // ── Property 11: parse never panics on arbitrary strings ──
        #[test]
        fn transform_parse_no_panic(input in "\\PC{0,200}") {
            let _ = TransformExpr::parse(&input); // Must not panic
        }

        // ── Property 12: pipe chain parse never panics ──
        #[test]
        fn pipe_chain_parse_no_panic(
            ops in prop::collection::vec(arb_simple_transform_name(), 1..10)
        ) {
            let chain = ops.join(" | ");
            let _ = TransformExpr::parse(&chain); // Must not panic
        }

        // ── Property 13: flatten total == sum of inner lengths ──
        #[test]
        fn flatten_total_equals_sum_of_inner(
            items in prop::collection::vec(
                prop::collection::vec(any::<i64>(), 0..5)
                    .prop_map(|v| Value::Array(v.into_iter().map(|n| json!(n)).collect())),
                0..10,
            )
        ) {
            let expected_len: usize = items.iter().map(|v| {
                if let Value::Array(a) = v { a.len() } else { 0 }
            }).sum();
            let arr = Value::Array(items);
            let flat = TransformExpr::parse("flatten").unwrap().apply(&arr).unwrap();
            if let Value::Array(ref f) = flat {
                prop_assert_eq!(f.len(), expected_len, "flatten total must equal sum of inner lengths");
            }
        }
    }

    // ─────────────────────────────────────────────────────────────
    // split_pipe_respecting_parens — quote tracking
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn pipe_split_unquoted_apostrophe_in_parens() {
        // "filter(it's)" should NOT break the pipe parser
        let parts = split_pipe_respecting_parens("join(it's) | upper");
        assert_eq!(
            parts.len(),
            2,
            "apostrophe inside parens must not break split"
        );
        assert_eq!(parts[0].trim(), "join(it's)");
        assert_eq!(parts[1].trim(), "upper");
    }

    #[test]
    fn pipe_split_quoted_pipe_in_parens() {
        let parts = split_pipe_respecting_parens(r#"join(" | ") | upper"#);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].trim(), r#"join(" | ")"#);
    }

    #[test]
    fn pipe_split_top_level_apostrophe() {
        // Apostrophe at top level (no parens) should not break splitting
        let parts = split_pipe_respecting_parens("it's a test | upper");
        assert_eq!(parts.len(), 2);
    }

    // ─────────────────────────────────────────────────────────────
    // has_default() tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn has_default_true_for_default_transform() {
        let expr = TransformExpr::parse("default(\"x\")").unwrap();
        assert!(expr.has_default());
    }

    #[test]
    fn has_default_true_in_chain() {
        let expr = TransformExpr::parse("default(\"x\") | upper").unwrap();
        assert!(expr.has_default());
    }

    #[test]
    fn has_default_false_without_default() {
        let expr = TransformExpr::parse("upper | trim").unwrap();
        assert!(!expr.has_default());
    }

    // ─────────────────────────────────────────────────────────────
    // starts_with / ends_with / contains
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn starts_with_true() {
        let expr = TransformExpr::parse("starts_with('/api')").unwrap();
        assert_eq!(expr.apply(&json!("/api/users")).unwrap(), json!(true));
    }

    #[test]
    fn starts_with_false() {
        let expr = TransformExpr::parse("starts_with('/api')").unwrap();
        assert_eq!(expr.apply(&json!("/blog/post")).unwrap(), json!(false));
    }

    #[test]
    fn ends_with_true() {
        let expr = TransformExpr::parse("ends_with('.html')").unwrap();
        assert_eq!(expr.apply(&json!("page.html")).unwrap(), json!(true));
    }

    #[test]
    fn ends_with_false() {
        let expr = TransformExpr::parse("ends_with('.html')").unwrap();
        assert_eq!(expr.apply(&json!("page.json")).unwrap(), json!(false));
    }

    #[test]
    fn contains_true() {
        let expr = TransformExpr::parse("contains('world')").unwrap();
        assert_eq!(expr.apply(&json!("hello world")).unwrap(), json!(true));
    }

    #[test]
    fn contains_false() {
        let expr = TransformExpr::parse("contains('xyz')").unwrap();
        assert_eq!(expr.apply(&json!("hello world")).unwrap(), json!(false));
    }

    // ─────────────────────────────────────────────────────────────
    // content_hash
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn content_hash_deterministic() {
        let expr = TransformExpr::parse("content_hash").unwrap();
        let h1 = expr.apply(&json!("hello")).unwrap();
        let h2 = expr.apply(&json!("hello")).unwrap();
        assert_eq!(h1, h2);
        // Verify it's a 16-char hex string
        assert_eq!(h1.as_str().unwrap().len(), 16);
    }

    #[test]
    fn content_hash_different_input() {
        let expr = TransformExpr::parse("content_hash").unwrap();
        let h1 = expr.apply(&json!("hello")).unwrap();
        let h2 = expr.apply(&json!("world")).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn content_hash_object() {
        let expr = TransformExpr::parse("content_hash").unwrap();
        let result = expr.apply(&json!({"a": 1})).unwrap();
        assert_eq!(result.as_str().unwrap().len(), 16);
    }

    // ─────────────────────────────────────────────────────────────
    // unique_urls
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn unique_urls_dedup_tracking_params() {
        let expr = TransformExpr::parse("unique_urls").unwrap();
        let input = json!([
            "https://example.com/page?utm_source=twitter",
            "https://example.com/page?utm_source=facebook",
            "https://example.com/other"
        ]);
        let result = expr.apply(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2); // first two normalize to same URL
    }

    #[test]
    fn unique_urls_preserves_order() {
        let expr = TransformExpr::parse("unique_urls").unwrap();
        let input = json!(["https://b.com", "https://a.com", "https://b.com"]);
        let result = expr.apply(&input).unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], json!("https://b.com"));
        assert_eq!(arr[1], json!("https://a.com"));
    }
}
