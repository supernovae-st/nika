// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The `nika:convert` column-order batteries (#1666), a child module of [`super`] in its
//! own file for the reason `tests.rs` states: the file-LOC gate measures `wc -l`.
use super::tests::args;
use super::*;

// ─── nika:convert · columns (an explicit header order · #1666) ──────────

#[test]
fn convert_columns_keeps_the_requested_header_order() {
    // Without `columns:` the header is sorted (the determinism default,
    // unchanged); with it, the listed columns lead in exactly that order — a
    // CSV a requester read as `order_id,customer,amount` is written back the
    // same way, whatever order the parsed objects happen to hold.
    let rows = serde_json::json!([
        {"amount": "150", "customer": "ada", "order_id": "A-1"},
        {"amount": "300", "customer": "cy", "order_id": "A-3"}
    ]);
    let sorted = convert(&args(serde_json::json!({
        "input": rows.clone(), "from": "json", "to": "csv"
    })))
    .expect("ok");
    assert_eq!(
        sorted.as_str().expect("string"),
        "amount,customer,order_id\n150,ada,A-1\n300,cy,A-3\n"
    );
    let ordered = convert(&args(serde_json::json!({
        "input": rows, "from": "json", "to": "csv",
        "columns": ["order_id", "customer", "amount"]
    })))
    .expect("ok");
    assert_eq!(
        ordered.as_str().expect("string"),
        "order_id,customer,amount\nA-1,ada,150\nA-3,cy,300\n"
    );
}

#[test]
fn convert_columns_emits_a_listed_column_absent_from_every_row_empty() {
    // A requested column no row carries still heads the file, empty: the
    // requester's layout is the contract, the data merely fills it.
    let csv = convert(&args(serde_json::json!({
        "input": [{"a": "1"}, {"a": "2"}],
        "from": "json", "to": "csv", "columns": ["missing", "a"]
    })))
    .expect("ok");
    assert_eq!(csv.as_str().expect("string"), "missing,a\n,1\n,2\n");
}

#[test]
fn convert_columns_appends_unlisted_keys_in_sorted_order() {
    // Keys the list does not name follow it in the sorted order the default
    // emits, so a partial list is still deterministic across engines.
    let csv = convert(&args(serde_json::json!({
        "input": [{"z": "1", "b": "2", "m": "3", "a": "4"}],
        "from": "json", "to": "csv", "columns": ["m", "z"]
    })))
    .expect("ok");
    assert_eq!(csv.as_str().expect("string"), "m,z,a,b\n3,1,4,2\n");
}

#[test]
fn convert_columns_folds_a_repeated_name_to_its_first_mention() {
    let csv = convert(&args(serde_json::json!({
        "input": [{"a": "1", "b": "2"}],
        "from": "json", "to": "csv", "columns": ["b", "a", "b"]
    })))
    .expect("ok");
    assert_eq!(csv.as_str().expect("string"), "b,a\n2,1\n");
}

#[test]
fn convert_columns_keeps_the_formula_guard_at_write_time() {
    // The guard reads the emitted header and cells after the order is
    // settled: a `=`-led column name is neutralized wherever the list places
    // it, and the guard never perturbs the order itself.
    let csv = convert(&args(serde_json::json!({
        "input": [{"=evil": "1", "safe": "=cmd"}],
        "from": "json", "to": "csv", "columns": ["safe", "=evil"], "formula_guard": true
    })))
    .expect("ok");
    assert_eq!(csv.as_str().expect("string"), "safe,'=evil\n'=cmd,1\n");
}

#[test]
fn convert_columns_leaves_the_identity_rejection_unchanged() {
    let identity = convert(&args(serde_json::json!({
        "input": "a\n1", "from": "csv", "to": "csv", "columns": ["a"]
    })));
    assert!(matches!(identity, Err(f) if f.code == "NIKA-BUILTIN-CONVERT-001"));
}

#[test]
fn convert_columns_is_a_strict_list_of_strings() {
    // A list in the wrong shape is a loud CONVERT-001, never silently the
    // sorted default: the requester asked for an order and would not get it.
    let malformed = [
        serde_json::json!("order_id,customer"),
        serde_json::json!([1, "a"]),
        serde_json::json!({"a": 1}),
    ];
    for bad in malformed {
        let out = convert(&args(serde_json::json!({
            "input": [{"a": "1"}], "from": "json", "to": "csv", "columns": bad
        })));
        assert!(
            matches!(&out, Err(f) if f.code == "NIKA-BUILTIN-CONVERT-001"
                && f.message.contains("columns")),
            "malformed columns is a loud CONVERT-001: {out:?}"
        );
    }
}

#[test]
fn hash_blake3_default_and_rejects_broken() {
    let h = hash(&args(serde_json::json!({ "content": "hello" }))).expect("ok");
    // blake3("hello") is a fixed 64-hex-char value.
    assert_eq!(h.as_str().expect("s").len(), 64);
    let b64 = hash(&args(
        serde_json::json!({ "content": "hello", "encoding": "base64" }),
    ))
    .expect("ok");
    assert!(b64.as_str().expect("s").ends_with('='));
    let sha = hash(&args(
        serde_json::json!({ "content": "x", "algo": "sha256" }),
    ))
    .expect("ok");
    assert_eq!(sha.as_str().expect("s").len(), 64);
    assert!(hash(&args(serde_json::json!({ "content": "x", "algo": "md5" }))).is_err());
}

#[test]
fn hash_literal_acceptance_matches_schema_and_check() {
    let definition = crate::defs::tool_defs()
        .into_iter()
        .find(|tool| tool.name == "nika:hash")
        .expect("hash definition");
    let schema = jsonschema::validator_for(&definition.parameters).expect("schema");
    for (input, accepted) in [
        (serde_json::json!({"content": ""}), true),
        (serde_json::json!({"content": {}}), true),
        (serde_json::json!({"content": []}), true),
        (serde_json::json!({"content": false}), true),
        (serde_json::json!({"content": 0}), true),
        (serde_json::json!({"content": null}), false),
        (serde_json::json!({"content": "", "algo": "md5"}), false),
        (serde_json::json!({"content": "", "algo": null}), false),
        (serde_json::json!({"content": "", "algo": 256}), false),
        (
            serde_json::json!({"content": "", "encoding": "rot13"}),
            false,
        ),
        (serde_json::json!({"content": "", "encoding": null}), false),
        (serde_json::json!({"content": "", "encoding": false}), false),
    ] {
        assert_eq!(
            hash(&args(input.clone())).is_ok(),
            accepted,
            "runtime: {input}"
        );
        assert_eq!(schema.is_valid(&input), accepted, "schema: {input}");
        assert_eq!(
            nika_cap::builtin_shape_findings("nika:hash", Some(&input)).is_empty(),
            accepted,
            "check: {input}"
        );
    }
    assert!(!schema.is_valid(&serde_json::json!({})));
    assert!(hash(&args(serde_json::json!({}))).is_err());
    // Dispatch/model schemas historically accept extra args; authoring check
    // owns rejecting them. This change must not silently close that boundary.
    let extra = serde_json::json!({"content": "", "unrelated": true});
    assert!(schema.is_valid(&extra));
    assert!(hash(&args(extra)).is_ok());
}

#[test]
fn hash_preserves_null_errors_and_optional_type_precedence() {
    for (input, message) in [
        (
            serde_json::json!({"content": null, "algo": null}),
            "`content:` is required",
        ),
        (
            serde_json::json!({"content": "", "algo": null}),
            "`algo:` must be a string when present",
        ),
        (
            serde_json::json!({"content": "", "algo": "md5", "encoding": null}),
            "`encoding:` must be a string when present",
        ),
        (
            serde_json::json!({"content": "", "algo": "md5", "encoding": "rot13"}),
            "unsupported algo `md5` (blake3|sha256|sha512 · md5/sha1 are broken)",
        ),
    ] {
        let err = hash(&args(input)).expect_err("invalid args");
        assert_eq!(err.code, "NIKA-BUILTIN-HASH-001");
        assert_eq!(err.message, message);
    }
}

#[test]
fn hash_preserves_verbatim_text_and_compact_json_bytes() {
    // Fixed SHA-256 values from Python hashlib, with ensure_ascii=False and
    // compact separators for JSON; no use of the implementation's serializer.
    for (content, expected) in [
        (
            serde_json::json!("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            serde_json::json!("a\r\nbé"),
            "33c464fa9324fd7f06d95d8451abd71bc323735401b9d4b50d8ca82e4dea7aef",
        ),
        (
            serde_json::json!(3),
            "4e07408562bedb8b60ce05c1decfe3ad16b72230967de01f640b7e4729b49fce",
        ),
        (
            serde_json::json!(false),
            "fcbcf165908dd18a9e49f7ff27810176db8e9f63b4352213741664245224f8aa",
        ),
        (
            serde_json::json!([1, null, "é"]),
            "e39a71fdaf04f5e69ebf9284cbdb692d3d279d190545c796d8f9cd7549b4c2d3",
        ),
    ] {
        let output = hash(&args(
            serde_json::json!({"content": content, "algo": "sha256"}),
        ))
        .expect("digest");
        assert_eq!(output, serde_json::json!(expected));
    }
}

#[test]
fn hash_empty_digest_vectors_cover_every_algorithm_and_encoding() {
    // BLAKE3 1.8.7 test_vectors.json, input_len=0, first 32 bytes:
    // https://github.com/BLAKE3-team/BLAKE3/blob/1.8.7/test_vectors/test_vectors.json
    // SHA vectors independently produced with Python hashlib/OpenSSL.
    let vectors = [
        (
            "blake3",
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
            "rxNJufX5oaagQE3qNtzJSZvLJcmtwRK3zJqTyuQfMmI=",
        ),
        (
            "sha256",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
        ),
        (
            "sha512",
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            "z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg==",
        ),
    ];
    let definition = crate::defs::tool_defs()
        .into_iter()
        .find(|tool| tool.name == "nika:hash")
        .expect("hash definition");
    let schema = jsonschema::validator_for(&definition.parameters).expect("schema");
    assert_eq!(vectors.len(), HashAlgorithm::ALL.len());
    for (algo, expected, expected_base64) in vectors {
        for encoding in HashEncoding::ALL {
            let input =
                serde_json::json!({"content": "", "algo": algo, "encoding": encoding.as_str()});
            assert!(schema.is_valid(&input));
            let output = hash(&args(input)).expect("digest");
            let text = output.as_str().expect("digest string");
            if encoding == HashEncoding::Hex {
                assert_eq!(text, expected);
            } else {
                assert_eq!(text, expected_base64);
            }
        }
    }
}

#[test]
fn hash_accepts_structured_content_without_a_tojson_prepass() {
    // Empirical 2026-08-19: interpolating a roster object into
    // `content:` used to refuse HASH-001 "`content:` (string) is required".
    let from_object = hash(&args(serde_json::json!({
        "content": [{"stem": "ada", "level": "gold"}]
    })))
    .expect("object content hashes");
    let via_json = hash(&args(serde_json::json!({
        "content": "[{\"level\":\"gold\",\"stem\":\"ada\"}]"
    })));
    // Compact serde_json key order is insertion order — the digest is
    // defined, not compared to a hand-typed string here. A number is
    // hashed as its decimal digits (same as a string of those digits).
    assert_eq!(from_object.as_str().expect("hex").len(), 64);
    let as_number = hash(&args(serde_json::json!({ "content": 3 }))).expect("n");
    let as_text = hash(&args(serde_json::json!({ "content": "3" }))).expect("s");
    assert_eq!(as_number, as_text);
    assert!(via_json.is_ok(), "string content still works: {via_json:?}");
}

#[test]
fn validate_parses_a_json_string_schema_from_nika_read() {
    let schema = "{\n  \"type\": \"object\",\n  \"required\": [\"name\"]\n}\n";
    let out = validate(&args(serde_json::json!({
        "data": {"name": "ada"},
        "schema": schema
    })))
    .expect("string schema is a schema");
    assert_eq!(out["valid"], true);
    let garbage = validate(&args(serde_json::json!({
        "data": {},
        "schema": "not a schema at all"
    })));
    assert!(
        matches!(&garbage, Err(f) if f.code == "NIKA-BUILTIN-VALIDATE-001"),
        "garbage string schema is VALIDATE-001: {garbage:?}"
    );
}

#[test]
fn base64_encoder_matches_known_vectors() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"f"), "Zg==");
    assert_eq!(base64_encode(b"fo"), "Zm8=");
    assert_eq!(base64_encode(b"foo"), "Zm9v");
    assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    // High bytes with overlapping bits: | and ^ diverge here.
    assert_eq!(base64_encode(&[0xff, 0xff, 0xff]), "////");
    assert_eq!(base64_encode(&[0xfb, 0xf0]), "+/A=");
}

#[test]
fn base64_decoder_mirrors_the_encoder_and_rejects_malformed() {
    // Round-trip over the encoder's own vectors (the consumer
    // contract: read binary → write).
    for bytes in [
        &b""[..],
        b"f",
        b"fo",
        b"foo",
        b"foobar",
        &[0xff, 0xff, 0xff],
        &[0xfb, 0xf0],
        &[0x00, 0x01, 0x02, 0x03, 0xfe],
    ] {
        assert_eq!(
            base64_decode(&base64_encode(bytes)).expect("round-trips"),
            bytes,
            "{bytes:?}"
        );
    }
    // Strictness: bad length · bad byte · interior/misplaced padding.
    assert!(base64_decode("Zg=").is_err(), "length not multiple of 4");
    assert!(base64_decode("Zg!=").is_err(), "non-alphabet byte");
    assert!(base64_decode("=g==").is_err(), "leading pad");
    assert!(base64_decode("Zg==Zm8=").is_err(), "interior padding quad");
    assert!(base64_decode("Z===").is_err(), "triple pad");
    // "A=B=": two pads but quad[2] is NOT '=' — this MUST be the
    // PADDING error, not the later invalid-byte error a mutated
    // `&&`→`||` arm would fall through to (the message IS the pin).
    let split_pads = base64_decode("A=B=").expect_err("split pads rejected");
    assert!(
        split_pads.contains("padding"),
        "rejected AT the padding gate: {split_pads}"
    );
}

proptest::proptest! {
    /// decode ∘ encode = identity over arbitrary bytes (the binary
    /// write clause rides this exact round-trip).
    #[test]
    fn base64_round_trips_arbitrary_bytes(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..256)) {
        proptest::prop_assert_eq!(
            base64_decode(&base64_encode(&bytes)).expect("round-trips"),
            bytes
        );
    }
}

// ── Gate 6 · property tests (crate spec §5) ─────────────────────────

/// Arbitrary JSON values whose textual form round-trips exactly:
/// full-range integers (jaq carries them losslessly), finite floats
/// (Display is shortest-round-trip), and `-0.0` excluded (its sign
/// is not observable through `Value` equality).
fn arb_json() -> impl proptest::strategy::Strategy<Value = serde_json::Value> {
    use proptest::prelude::*;
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(|n| serde_json::json!(n)),
        // Floats restricted to serde_json's OWN round-trip set: on
        // rare 17-digit edges serde's printer and parser disagree by
        // 1 ULP (verified empirically on 118132816.07034513 ·
        // serde_json 1.0.149) — that ecosystem caveat is not this
        // bridge's contract. Text fidelity through jaq is verbatim
        // either way (Num::Dec carries the literal).
        (-1.0e9f64..1.0e9)
            .prop_filter("skip negative zero", |f| !(*f == 0.0
                && f.is_sign_negative()))
            .prop_filter("serde_json self-round-trips", |f| {
                serde_json::to_string(f)
                    .ok()
                    .and_then(|s| serde_json::from_str::<f64>(&s).ok())
                    == Some(*f)
            })
            .prop_map(|f| serde_json::json!(f)),
        "[a-zA-Z0-9 _.-]{0,12}".prop_map(serde_json::Value::String),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
            proptest::collection::btree_map("[a-z]{1,6}", inner, 0..4)
                .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
        ]
    })
}

proptest::proptest! {
    /// The exactly-one-output law over ARBITRARY input: the identity
    /// program always emits exactly one value, and the jaq round-trip
    /// (serde → Val → Display → serde) is lossless.
    #[test]
    fn jq_identity_round_trips_arbitrary_json(value in arb_json()) {
        let out = jq(&args(serde_json::json!({
            "expression": ".", "input": value.clone()
        })))
        .expect("identity emits exactly one value");
        proptest::prop_assert_eq!(out, value);
    }
}

/// The EXPRESSION BOUNDARY at the `nika:jq` seam — the ratchet that survives
/// the next `jaq` release.
///
/// D-2026-08-11-N26 says an expression sees only its input. A blocklist alone
/// would let a future `jaq` ship a new ambient native and reopen the hole IN
/// SILENCE, so the guard here is a PINNED INVENTORY: the full native set the
/// workspace-pinned stack exposes, asserted as a set. Grow it, rename one,
/// drop one — this goes red and a human triages the newcomer into
/// `nika_cap::WITHHELD_JQ_NATIVES` or into the pin.
#[allow(clippy::expect_used, clippy::panic)]
mod expression_boundary {
    use jaq_core::data::JustLut;
    use jaq_json::Val;

    /// Every native the pinned stack exposes — jaq-core 3.1 (10) · jaq-std 3.0
    /// (96) · jaq-json 2.0 (8). Derived by running `funs()` on 2026-08-15, not
    /// transcribed from upstream docs.
    ///
    /// `debug_empty` · `stderr_empty` · `halt` are present ON PURPOSE: they
    /// EMIT to the host or act on the process rather than SEE beyond the input,
    /// which is a different class from N26's subtraction, and jaq-std's own
    /// `defs.jq` builds `debug`/`stderr`/`halt_error` on them. Named here so
    /// the next reader knows they were considered, not missed.
    const PINNED_NATIVES: &[&str] = &[
        // jaq-core
        "error_empty",
        "first",
        "key_values",
        "keys_unsorted",
        "last",
        "limit",
        "path",
        "path_value",
        "range",
        "skip",
        // jaq-std
        "acos",
        "acosh",
        "ascii_downcase",
        "ascii_upcase",
        "asin",
        "asinh",
        "atan",
        "atan2",
        "atanh",
        "cbrt",
        "ceil",
        "copysign",
        "cos",
        "cosh",
        "debug_empty",
        "decode_base64",
        "decode_uri",
        "encode_base64",
        "encode_uri",
        "endswith",
        "env",
        "erf",
        "erfc",
        "escape_html",
        "escape_sh",
        "exp",
        "exp10",
        "exp2",
        "explode",
        "expm1",
        "fabs",
        "fdim",
        "floor",
        "fma",
        "fmax",
        "fmin",
        "fmod",
        "frexp",
        "fromdateiso8601",
        "gmtime",
        "group_by",
        "halt",
        "hypot",
        "ilogb",
        "implode",
        "j0",
        "j1",
        "jn",
        "ldexp",
        "lgamma",
        "localtime",
        "log",
        "log10",
        "log1p",
        "log2",
        "ltrim",
        "ltrimstr",
        "matches",
        "max_by_or_empty",
        "min_by_or_empty",
        "mktime",
        "modf",
        "nearbyint",
        "nextafter",
        "now",
        "pow",
        "remainder",
        "reverse",
        "rint",
        "round",
        "rtrim",
        "rtrimstr",
        "scalbln",
        "sin",
        "sinh",
        "sort",
        "sort_by",
        "split_",
        "split_matches",
        "sqrt",
        "startswith",
        "stderr_empty",
        "strflocaltime",
        "strftime",
        "strptime",
        "tan",
        "tanh",
        "tgamma",
        "todateiso8601",
        "trim",
        "trunc",
        "unescape_html",
        "utf8bytelength",
        "y0",
        "y1",
        "yn",
        // jaq-json
        "bsearch",
        "contains",
        "fromjson",
        "has",
        "indices",
        "length",
        "tobytes",
        "tojson",
    ];

    fn exposed() -> std::collections::BTreeSet<&'static str> {
        jaq_core::funs::<JustLut<Val>>()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs())
            .map(|f| f.0)
            .collect()
    }

    #[test]
    fn the_native_inventory_is_pinned() {
        let pinned: std::collections::BTreeSet<&str> = PINNED_NATIVES.iter().copied().collect();
        assert_eq!(
            pinned.len(),
            PINNED_NATIVES.len(),
            "the pin lists a name twice"
        );
        let live = exposed();
        let added: Vec<_> = live.difference(&pinned).copied().collect();
        let gone: Vec<_> = pinned.difference(&live).copied().collect();
        assert!(
            added.is_empty() && gone.is_empty(),
            "the jaq native set MOVED · new: {added:?} · gone: {gone:?}\n\
             Triage every newcomer: does it read the process, the clock, the disk \
             or the environment? If so it belongs in nika_cap::JQ_CAPABILITY_POLICY \
             (D-2026-08-11-N26/N27). Otherwise add \
             it to PINNED_NATIVES with that judgment recorded in the commit."
        );
    }

    #[test]
    fn every_withheld_name_really_exists_upstream() {
        // A withheld name that jaq does not define would be a dead entry
        // pretending to guard something — the list must bite.
        let live = exposed();
        for w in nika_cap::JQ_CAPABILITY_POLICY
            .iter()
            .filter(|rule| rule.kind == nika_cap::JqSymbolKind::Native)
        {
            assert!(
                live.contains(w.name),
                "`{}` is withheld but jaq no longer defines it — the row guards nothing",
                w.name
            );
        }
    }

    #[test]
    fn the_compiled_function_set_is_the_inventory_minus_the_withheld() {
        let live = exposed();
        let withheld: std::collections::BTreeSet<&str> = nika_cap::JQ_CAPABILITY_POLICY
            .iter()
            .filter(|rule| rule.kind == nika_cap::JqSymbolKind::Native)
            .map(|w| w.name)
            .collect();
        let compiled: std::collections::BTreeSet<&str> = jaq_core::funs::<JustLut<Val>>()
            .chain(jaq_std::funs())
            .chain(jaq_json::funs())
            .filter(|f| nika_cap::install_jq_native(f.0))
            .map(|f| f.0)
            .collect();
        let expected: std::collections::BTreeSet<&str> =
            live.difference(&withheld).copied().collect();
        assert_eq!(compiled, expected, "the filter is not the subtraction");
        assert!(!compiled.contains("env"), "env reached the compiler");
    }
}
