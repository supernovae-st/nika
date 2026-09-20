// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sanitized provider HTTP failure metadata. Never stores a response body.

/// Safe HTTP failure evidence, independent of usage or billing evidence.
///
/// Unknown provider identifiers are omitted: an identifier-shaped string can
/// still be a credential. Only the closed vocabulary below crosses this seam.
/// This is an in-process diagnostic, not a serialized response contract.
#[derive(Debug)]
#[non_exhaustive]
pub struct ProviderHttpError {
    status: u16,
    code: Option<&'static str>,
    error_type: Option<&'static str>,
    retry_after: Option<String>,
    retry_after_ms: Option<u64>,
}

impl ProviderHttpError {
    /// Retain recognized identifiers and Retry-After with bounded safe syntax.
    #[must_use]
    pub fn new(
        status: u16,
        code: Option<&str>,
        error_type: Option<&str>,
        retry_after: Option<&str>,
    ) -> Self {
        let retry_after = retry_after.and_then(safe_retry_after);
        let retry_after_ms = retry_after.as_deref().and_then(delay_ms);
        Self {
            status,
            code: code.and_then(safe_identifier),
            error_type: error_type.and_then(safe_identifier),
            retry_after,
            retry_after_ms,
        }
    }

    /// HTTP response status (or the equivalent in-band provider status).
    #[must_use]
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Recognized provider error code, absent for unknown or malformed values.
    #[must_use]
    pub fn code(&self) -> Option<&'static str> {
        self.code
    }

    /// Recognized provider error type, absent for unknown or malformed values.
    #[must_use]
    pub fn error_type(&self) -> Option<&'static str> {
        self.error_type
    }

    /// Sanitized delay-seconds or IMF-fixdate header; no clock is consulted.
    #[must_use]
    pub fn retry_after(&self) -> Option<&str> {
        self.retry_after.as_deref()
    }

    /// Delay in milliseconds when Retry-After uses delay-seconds.
    /// Date-form headers remain available through `retry_after`.
    #[must_use]
    pub fn retry_after_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }

    /// An explicit quota/credit exhaustion signal, never inferred from prose.
    #[must_use]
    pub fn is_quota_exhausted(&self) -> bool {
        [self.code, self.error_type]
            .into_iter()
            .flatten()
            .any(|s| matches!(s, "insufficient_quota" | "credit_balance_exhausted"))
    }

    /// Whether waiting may help. Exhausted credit is always terminal.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        !self.is_quota_exhausted() && matches!(self.status, 429 | 500..=599)
    }
}

impl std::fmt::Display for ProviderHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = if self.is_quota_exhausted() {
            "provider quota exhausted; automatic retry disabled"
        } else {
            match self.status {
                401 | 403 => "authentication failed",
                404 => "provider endpoint or model not found",
                429 => "rate limited",
                _ => "provider API error",
            }
        };
        write!(f, "{label} (HTTP {})", self.status)?;
        if let Some(code) = self.code {
            write!(f, "; code={code}")?;
        }
        if let Some(kind) = self.error_type {
            write!(f, "; type={kind}")?;
        }
        if let Some(delay) = &self.retry_after {
            write!(f, "; Retry-After={delay}")?;
        }
        write!(f, "; usage and billing unknown")?;
        if matches!(self.status, 401 | 403) {
            write!(f, " — {}", super::auth_failure_help())?;
        }
        Ok(())
    }
}

/// Return static vocabulary, never a substring of provider-controlled text.
fn safe_identifier(value: &str) -> Option<&'static str> {
    match value {
        "insufficient_quota" => Some("insufficient_quota"),
        "credit_balance_exhausted" => Some("credit_balance_exhausted"),
        "rate_limit_exceeded" => Some("rate_limit_exceeded"),
        "rate_limit_error" => Some("rate_limit_error"),
        "requests" => Some("requests"),
        "tokens" => Some("tokens"),
        "invalid_api_key" => Some("invalid_api_key"),
        "authentication_error" => Some("authentication_error"),
        "permission_error" => Some("permission_error"),
        "invalid_request_error" => Some("invalid_request_error"),
        "not_found_error" => Some("not_found_error"),
        "model_not_found" => Some("model_not_found"),
        "api_error" => Some("api_error"),
        "server_error" => Some("server_error"),
        "overloaded_error" => Some("overloaded_error"),
        "RESOURCE_EXHAUSTED" => Some("RESOURCE_EXHAUSTED"),
        _ => None,
    }
}

/// Numeric seconds (including bounded fractional gateway values).
fn delay_ms(value: &str) -> Option<u64> {
    if value.len() > 32 || !value.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    let duration = std::time::Duration::try_from_secs_f64(value.parse().ok()?).ok()?;
    u64::try_from(duration.as_millis()).ok()
}

/// Permit only numeric seconds or the fixed RFC 9110 preferred date grammar.
fn safe_retry_after(value: &str) -> Option<String> {
    if value.len() > 32 {
        return None;
    }
    let value = value.trim();
    if delay_ms(value).is_some() || is_http_date(value) {
        Some(value.to_owned())
    } else {
        None
    }
}

/// Strict ASCII IMF-fixdate grammar, with bounded numeric calendar fields.
fn is_http_date(value: &str) -> bool {
    if value.len() != 29 || !value.is_ascii() {
        return false;
    }
    let number = |start, end, min, max| {
        let field = &value[start..end];
        field.bytes().all(|b| b.is_ascii_digit())
            && field.parse::<u32>().is_ok_and(|n| (min..=max).contains(&n))
    };
    matches!(
        &value[..3],
        "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun"
    ) && &value[3..5] == ", "
        && number(5, 7, 1, 31)
        && &value[7..8] == " "
        && matches!(
            &value[8..11],
            "Jan"
                | "Feb"
                | "Mar"
                | "Apr"
                | "May"
                | "Jun"
                | "Jul"
                | "Aug"
                | "Sep"
                | "Oct"
                | "Nov"
                | "Dec"
        )
        && &value[11..12] == " "
        && number(12, 16, 1900, 9999)
        && &value[16..17] == " "
        && number(17, 19, 0, 23)
        && &value[19..20] == ":"
        && number(20, 22, 0, 59)
        && &value[22..23] == ":"
        && number(23, 25, 0, 59)
        && &value[25..] == " GMT"
}
