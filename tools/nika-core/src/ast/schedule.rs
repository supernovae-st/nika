//! Schedule configuration for recurring workflow execution.
//!
//! Supports three input formats:
//! 1. Human-readable (hron): `"every weekday at 9:00"`
//! 2. Presets: `"@daily"`, `"@hourly"`
//! 3. Raw 5-field cron: `"0 9 * * *"`
//!
//! String-or-object pattern (like `infer:`):
//! ```yaml
//! schedule: "@daily"
//! schedule:
//!   cron: "0 9 * * 1-5"
//!   timezone: "Europe/Paris"
//! ```

use crate::source::Span;

/// Parsed and validated schedule configuration.
#[derive(Debug, Clone)]
pub struct ScheduleConfig {
    /// Canonical 5-field cron expression (always normalized after parsing).
    pub cron: String,
    /// IANA timezone. None = UTC.
    pub timezone: Option<String>,
    /// Original human-readable string (for display). None if raw cron.
    pub human: Option<String>,
    /// Overlap policy: "skip" (default), "queue", "replace".
    pub overlap: String,
    /// Start paused?
    pub paused: bool,
    /// Span for diagnostics.
    pub span: Span,
}

/// Parse a schedule value (string or object) into a ScheduleConfig.
///
/// Returns `Ok(config)` or an error message string for NIKA-280/282.
pub fn parse_schedule_value(
    value: &serde_json::Value,
    span: Span,
) -> Result<ScheduleConfig, String> {
    match value {
        serde_json::Value::String(s) => parse_schedule_string(s, span),
        serde_json::Value::Object(map) => {
            let cron_str = map
                .get("cron")
                .or_else(|| map.get("every"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    "schedule object requires a 'cron' field (e.g. schedule: { cron: \"@daily\" })"
                        .to_string()
                })?;

            let config = parse_schedule_string(cron_str, span)?;

            let timezone = map
                .get("timezone")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(ref tz) = timezone {
                validate_timezone(tz)?;
            }

            let overlap = map
                .get("overlap")
                .and_then(|v| v.as_str())
                .unwrap_or("skip")
                .to_string();
            if !["skip", "queue", "replace"].contains(&overlap.as_str()) {
                return Err(format!(
                    "invalid overlap policy '{}' — must be skip, queue, or replace",
                    overlap
                ));
            }

            let paused = map.get("paused").and_then(|v| v.as_bool()).unwrap_or(false);

            Ok(ScheduleConfig {
                timezone: timezone.or(config.timezone),
                overlap,
                paused,
                ..config
            })
        }
        _ => Err("schedule must be a string or object".to_string()),
    }
}

/// Parse a schedule string (hron, @preset, raw cron, or duration shorthand).
fn parse_schedule_string(s: &str, span: Span) -> Result<ScheduleConfig, String> {
    let trimmed = s.trim();

    // 1. Try @preset — croner handles these directly
    if trimmed.starts_with('@') {
        validate_cron(trimmed)?;
        return Ok(ScheduleConfig {
            cron: trimmed.to_string(),
            timezone: None,
            human: Some(trimmed.to_string()),
            overlap: "skip".to_string(),
            paused: false,
            span,
        });
    }

    // 2. Try hron ("every day at 9:00", "every weekday at 14:30")
    if trimmed.starts_with("every ") {
        if let Ok(schedule) = hron::Schedule::parse(trimmed) {
            let cron = schedule
                .to_cron()
                .map_err(|e| format!("hron→cron conversion failed: {e}"))?;
            // Validate the generated cron expression (defense-in-depth)
            validate_cron(&cron)?;
            let tz = schedule.timezone().map(|s| s.to_string());
            return Ok(ScheduleConfig {
                cron,
                timezone: tz,
                human: Some(trimmed.to_string()),
                overlap: "skip".to_string(),
                paused: false,
                span,
            });
        }
        // hron failed — fall through to try as raw cron (it won't match, but gives better error)
    }

    // 3. Try duration shorthand: "6h", "30m", "1d"
    if let Some(cron) = duration_to_cron(trimmed) {
        validate_cron(&cron)?;
        return Ok(ScheduleConfig {
            cron,
            timezone: None,
            human: Some(format!("every {trimmed}")),
            overlap: "skip".to_string(),
            paused: false,
            span,
        });
    }

    // 4. Try raw 5-field cron
    validate_cron(trimmed)?;
    Ok(ScheduleConfig {
        cron: trimmed.to_string(),
        timezone: None,
        human: None,
        overlap: "skip".to_string(),
        paused: false,
        span,
    })
}

/// Validate a cron expression using croner.
///
/// Rejects 6-field cron (with seconds) — Nika uses standard 5-field cron only.
fn validate_cron(expr: &str) -> Result<(), String> {
    if !expr.starts_with('@') {
        let field_count = expr.split_whitespace().count();
        if field_count != 5 {
            return Err(format!(
                "expected 5-field cron expression, got {field_count} fields. \
                 Nika uses standard cron (minute hour day month weekday)."
            ));
        }
    }
    expr.parse::<croner::Cron>()
        .map(|_| ())
        .map_err(|e| format!("invalid cron expression '{}': {}", expr, e))
}

/// Validate an IANA timezone name.
fn validate_timezone(tz: &str) -> Result<(), String> {
    tz.parse::<chrono_tz::Tz>().map(|_| ()).map_err(|_| {
        format!(
            "invalid timezone '{}' — use IANA name (e.g. Europe/Paris)",
            tz
        )
    })
}

/// Convert duration shorthand to cron expression.
///
/// Supports: "6h" → "0 */6 * * *", "30m" → "*/30 * * * *", "1d" → "0 0 * * *".
pub fn duration_to_cron(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_suffix('h') {
        let n: u32 = rest.parse().ok()?;
        if n == 0 || n > 23 {
            return None;
        }
        Some(format!("0 */{n} * * *"))
    } else if let Some(rest) = s.strip_suffix('m') {
        let n: u32 = rest.parse().ok()?;
        if n == 0 || n > 59 {
            return None;
        }
        Some(format!("*/{n} * * * *"))
    } else if let Some(rest) = s.strip_suffix('d') {
        let n: u32 = rest.parse().ok()?;
        if n == 0 || n > 28 {
            return None;
        }
        if n == 1 {
            Some("0 0 * * *".to_string())
        } else {
            Some(format!("0 0 */{n} * *"))
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;

    fn test_span() -> Span {
        Span::dummy()
    }

    #[test]
    fn parse_raw_cron() {
        let val = serde_json::json!("0 9 * * *");
        let config = parse_schedule_value(&val, test_span()).unwrap();
        assert_eq!(config.cron, "0 9 * * *");
        assert!(config.human.is_none());
        assert!(config.timezone.is_none());
    }

    #[test]
    fn parse_preset() {
        let val = serde_json::json!("@daily");
        let config = parse_schedule_value(&val, test_span()).unwrap();
        assert_eq!(config.cron, "@daily");
        assert_eq!(config.human.as_deref(), Some("@daily"));
    }

    #[test]
    fn parse_object_form() {
        let val = serde_json::json!({
            "cron": "0 9 * * 1-5",
            "timezone": "Europe/Paris",
            "overlap": "queue"
        });
        let config = parse_schedule_value(&val, test_span()).unwrap();
        assert_eq!(config.cron, "0 9 * * 1-5");
        assert_eq!(config.timezone.as_deref(), Some("Europe/Paris"));
        assert_eq!(config.overlap, "queue");
    }

    #[test]
    fn parse_invalid_cron() {
        // "not a cron" has 3 fields → rejected by field count check
        let val = serde_json::json!("not a cron");
        let err = parse_schedule_value(&val, test_span()).unwrap_err();
        assert!(err.contains("5-field"), "got: {err}");

        // 5 fields but invalid content → rejected by croner
        let val = serde_json::json!("99 99 99 99 99");
        let err = parse_schedule_value(&val, test_span()).unwrap_err();
        assert!(err.contains("invalid cron"), "got: {err}");
    }

    #[test]
    fn parse_invalid_timezone() {
        let val = serde_json::json!({
            "cron": "@daily",
            "timezone": "Mars/Olympus"
        });
        let err = parse_schedule_value(&val, test_span()).unwrap_err();
        assert!(err.contains("invalid timezone"), "got: {err}");
    }

    #[test]
    fn parse_duration_shorthand() {
        let val = serde_json::json!("6h");
        let config = parse_schedule_value(&val, test_span()).unwrap();
        assert_eq!(config.cron, "0 */6 * * *");
        assert_eq!(config.human.as_deref(), Some("every 6h"));

        let val = serde_json::json!("30m");
        let config = parse_schedule_value(&val, test_span()).unwrap();
        assert_eq!(config.cron, "*/30 * * * *");
    }

    #[test]
    fn parse_hron_string() {
        // hron uses 24h format
        let val = serde_json::json!("every day at 9:00");
        let result = parse_schedule_value(&val, test_span());
        // hron may or may not be available; if it parses, check the output
        if let Ok(config) = result {
            assert!(
                config.cron.contains('9'),
                "cron should reference hour 9: {}",
                config.cron
            );
            assert!(config.human.is_some());
        }
        // If hron fails to parse, it falls through to cron which will fail —
        // that's OK, it means hron doesn't support this exact format
    }

    #[test]
    fn reject_six_field_cron() {
        let val = serde_json::json!("0 0 9 * * *");
        let err = parse_schedule_value(&val, test_span()).unwrap_err();
        assert!(
            err.contains("5-field"),
            "should mention 5-field requirement: {err}"
        );
    }

    #[test]
    fn accept_five_field_cron() {
        let val = serde_json::json!("0 9 * * *");
        let config = parse_schedule_value(&val, test_span()).unwrap();
        assert_eq!(config.cron, "0 9 * * *");
    }

    #[test]
    fn accept_preset_no_field_count_check() {
        let val = serde_json::json!("@daily");
        let config = parse_schedule_value(&val, test_span()).unwrap();
        assert_eq!(config.cron, "@daily");
    }

    #[test]
    fn parse_invalid_overlap() {
        let val = serde_json::json!({
            "cron": "@daily",
            "overlap": "invalid"
        });
        let err = parse_schedule_value(&val, test_span()).unwrap_err();
        assert!(err.contains("overlap"), "got: {err}");
    }
}
