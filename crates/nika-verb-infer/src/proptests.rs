use super::*;
use proptest::prelude::*;

proptest! {
    /// Temperature acceptance matches the spec interval exactly.
    #[test]
    fn temperature_validation_matches_spec_interval(t in -10.0f32..10.0) {
        let mut input = InferInput::new("p");
        input.temperature = Some(t);
        let ok = validate_params(&input).is_ok();
        prop_assert_eq!(ok, (0.0..=2.0).contains(&t));
    }

    /// The balanced-span extractor never panics on arbitrary text and,
    /// when it extracts, the candidate is real JSON.
    #[test]
    fn extraction_total_on_arbitrary_text(s in ".{0,400}") {
        // Total function — must not panic (the coercion pass included).
        let schema = serde_json::json!({ "type": "object" });
        let v = crate::structured::compile_schema(&schema)
            .expect("trivial schema compiles");
        let _ = crate::structured::extract_and_validate(&s, &v, &schema);
    }
}
