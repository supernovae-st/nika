//! Minimal JSONPath parser (v0.1)
//!
//! Supports:
//! - $.a.b.c (dot notation)
//! - $.a[0].b (array index)
//! - a.b.c (without $ prefix)
//!
//! Does NOT support:
//! - Filters: $.a[?(@.x==1)]
//! - Wildcards: $.a[*]
//! - Slices: $.a[0:5]

use serde_json::Value;

use crate::error::NikaError;

/// A parsed JSONPath segment
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Object field access: .field
    Field(String),
    /// Array index access: [0]
    Index(usize),
}

/// Parse a JSONPath string into segments
///
/// Examples:
/// - "$.price.currency" → [Field("price"), Field("currency")]
/// - "items[0].name" → [Field("items"), Index(0), Field("name")]
pub fn parse(path: &str) -> Result<Vec<Segment>, NikaError> {
    // Remove $. prefix if present
    let path = if path.starts_with("$.") {
        &path[2..]
    } else if path == "$" {
        return Ok(vec![]); // Root reference
    } else {
        path
    };

    if path.is_empty() {
        return Ok(vec![]);
    }

    let mut segments = Vec::new();

    for part in path.split('.') {
        if part.is_empty() {
            return Err(NikaError::JsonPathUnsupported {
                path: path.to_string(),
            });
        }

        // Check for array index: field[0] or just [0]
        if let Some(bracket_pos) = part.find('[') {
            // Field before bracket
            let field = &part[..bracket_pos];
            if !field.is_empty() {
                segments.push(Segment::Field(field.to_string()));
            }

            // Parse index
            if !part.ends_with(']') {
                return Err(NikaError::JsonPathUnsupported {
                    path: path.to_string(),
                });
            }

            let index_str = &part[bracket_pos + 1..part.len() - 1];
            let index: usize = index_str.parse().map_err(|_| NikaError::JsonPathUnsupported {
                path: path.to_string(),
            })?;

            segments.push(Segment::Index(index));
        } else if let Ok(index) = part.parse::<usize>() {
            // Numeric segment treated as array index (e.g., "items.0")
            segments.push(Segment::Index(index));
        } else {
            segments.push(Segment::Field(part.to_string()));
        }
    }

    Ok(segments)
}

/// Apply JSONPath segments to a JSON value
pub fn apply(value: &Value, segments: &[Segment]) -> Option<Value> {
    let mut current = value.clone();

    for segment in segments {
        current = match segment {
            Segment::Field(name) => current.get(name)?.clone(),
            Segment::Index(idx) => current.get(*idx)?.clone(),
        };
    }

    Some(current)
}

/// Parse and apply JSONPath in one step
pub fn resolve(value: &Value, path: &str) -> Result<Option<Value>, NikaError> {
    let segments = parse(path)?;
    Ok(apply(value, &segments))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_simple_path() {
        let segments = parse("$.a.b.c").unwrap();
        assert_eq!(
            segments,
            vec![
                Segment::Field("a".to_string()),
                Segment::Field("b".to_string()),
                Segment::Field("c".to_string()),
            ]
        );
    }

    #[test]
    fn parse_without_dollar() {
        let segments = parse("a.b").unwrap();
        assert_eq!(
            segments,
            vec![
                Segment::Field("a".to_string()),
                Segment::Field("b".to_string()),
            ]
        );
    }

    #[test]
    fn parse_with_array_index() {
        let segments = parse("$.items[0].name").unwrap();
        assert_eq!(
            segments,
            vec![
                Segment::Field("items".to_string()),
                Segment::Index(0),
                Segment::Field("name".to_string()),
            ]
        );
    }

    #[test]
    fn parse_just_root() {
        let segments = parse("$").unwrap();
        assert!(segments.is_empty());
    }

    #[test]
    fn apply_simple() {
        let value = json!({"a": {"b": "value"}});
        let segments = parse("$.a.b").unwrap();
        let result = apply(&value, &segments);
        assert_eq!(result, Some(json!("value")));
    }

    #[test]
    fn apply_array_index() {
        let value = json!({"items": ["first", "second", "third"]});
        let segments = parse("$.items[1]").unwrap();
        let result = apply(&value, &segments);
        assert_eq!(result, Some(json!("second")));
    }

    #[test]
    fn apply_nested_array() {
        let value = json!({
            "users": [
                {"name": "Alice"},
                {"name": "Bob"}
            ]
        });
        let segments = parse("$.users[0].name").unwrap();
        let result = apply(&value, &segments);
        assert_eq!(result, Some(json!("Alice")));
    }

    #[test]
    fn apply_missing_field() {
        let value = json!({"a": 1});
        let segments = parse("$.b").unwrap();
        let result = apply(&value, &segments);
        assert_eq!(result, None);
    }

    #[test]
    fn resolve_shorthand() {
        let value = json!({"price": {"currency": "EUR", "amount": 100}});
        let result = resolve(&value, "$.price.currency").unwrap();
        assert_eq!(result, Some(json!("EUR")));
    }

    #[test]
    fn parse_numeric_index_as_dot() {
        // Support "items.0" syntax (equivalent to "items[0]")
        let segments = parse("items.0").unwrap();
        assert_eq!(
            segments,
            vec![Segment::Field("items".to_string()), Segment::Index(0)]
        );
    }

    #[test]
    fn apply_numeric_index_as_dot() {
        let value = json!({"items": ["first", "second"]});
        let result = resolve(&value, "items.1").unwrap();
        assert_eq!(result, Some(json!("second")));
    }
}
