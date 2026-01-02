//! Output policy for format and validation (v0.1)

use serde::Deserialize;

/// Output policy configuration
#[derive(Debug, Clone, Deserialize, Default)]
pub struct OutputPolicy {
    /// Output format (text or json)
    #[serde(default)]
    pub format: OutputFormat,

    /// Optional JSON Schema path for validation
    #[serde(default)]
    pub schema: Option<String>,
}

/// Output format enum
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Raw text output (default)
    #[default]
    Text,

    /// JSON parsed output
    Json,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_text_format() {
        let yaml = "format: text";
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.format, OutputFormat::Text);
        assert!(policy.schema.is_none());
    }

    #[test]
    fn parse_json_with_schema() {
        let yaml = r#"
            format: json
            schema: .nika/schemas/result.json
        "#;
        let policy: OutputPolicy = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(policy.format, OutputFormat::Json);
        assert_eq!(policy.schema.as_deref(), Some(".nika/schemas/result.json"));
    }

    #[test]
    fn default_is_text() {
        let policy = OutputPolicy::default();
        assert_eq!(policy.format, OutputFormat::Text);
    }
}
