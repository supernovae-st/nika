// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sanitized provider HTTP failure metadata. Never stores a response body:
//! at most the provider's own message, reduced to one bounded line without secrets.

/// Safe HTTP failure evidence, independent of usage or billing evidence.
///
/// Unknown provider identifiers are omitted: an identifier-shaped string can
/// still be a credential. Only the closed vocabulary below crosses this seam
/// for decisions. The provider's own message may ride beside it for the person
/// to read ([`ProviderHttpError::with_message`]); nothing classifies it.
/// This is an in-process diagnostic, not a serialized response contract.
#[derive(Debug)]
#[non_exhaustive]
pub struct ProviderHttpError {
    status: u16,
    code: Option<&'static str>,
    error_type: Option<&'static str>,
    // Boxed text, and the delay read on demand: the error stays no larger than it was before the
    // provider's message rode it, since every `Result` carrying it grows with it.
    retry_after: Option<Box<str>>,
    message: Option<Box<str>>,
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
        Self {
            status,
            code: code.and_then(safe_identifier),
            error_type: error_type.and_then(safe_identifier),
            retry_after: retry_after
                .and_then(safe_retry_after)
                .map(String::into_boxed_str),
            message: None,
        }
    }

    /// Attach the provider's own words about the failure, for the person to read.
    ///
    /// They are relayed, never classified: transience and quota still come from
    /// the status and the closed vocabulary. Every `withheld` value (the
    /// credential the call sent) and every credential-shaped word reads
    /// `[withheld]`, control and invisible formatting characters are removed,
    /// and one line of at most 400 characters remains. Empty text attaches nothing.
    #[must_use]
    pub fn with_message(mut self, message: &str, withheld: &[&str]) -> Self {
        self.message = readable(message, withheld).map(String::into_boxed_str);
        self
    }

    /// The provider's own words, as [`Self::with_message`] kept them.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
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
        self.retry_after.as_deref().and_then(delay_ms)
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
        let label = if self.code == Some("credit_balance_exhausted") {
            "provider quota exhausted (credit balance): top up the account or seat another model; automatic retry disabled"
        } else if self.is_quota_exhausted() {
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
        if let Some(message) = &self.message {
            write!(f, "; the provider said: \"{message}\"")?;
        }
        write!(f, "; usage and billing unknown")?;
        if matches!(self.status, 401 | 403) {
            write!(f, " — {}", super::auth_failure_help())?;
        }
        Ok(())
    }
}

/// Longest provider message kept, in characters: a quota notice with its links fits.
const MESSAGE_CHARS: usize = 400;

/// What a withheld value or a credential-shaped word reads as.
const WITHHELD: &str = "[withheld]";

/// Prefixes that providers and platforms give their keys and tokens.
const KEY_PREFIXES: &str = "sk- sk_ rk_ xai- gsk_ hf_ nvapi- AIza ya29. ghp_ gho_ ghu_ ghs_ ghr_ \
                            github_pat_ glpat- xox AKIA ASIA eyJ npm_ SCW";

/// Words after which a value is a credential (`api_key=…`, `token: …`).
const CREDENTIAL_LABELS: &str = "key token secret password passwd pwd auth authorization signature \
                                 sig credential credentials";

/// The provider's words as one bounded line without secrets (see `with_message`).
fn readable(raw: &str, withheld: &[&str]) -> Option<String> {
    let text = one_line(&withhold(raw.to_owned(), withheld.iter().copied()));
    // An echo split by invisible characters meets its value only once they are gone.
    let lines: Vec<String> = withheld.iter().copied().map(one_line).collect();
    let text = without_credential_shapes(&withhold(text, lines.iter().map(String::as_str)));
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(match text.char_indices().nth(MESSAGE_CHARS) {
        Some((cut, _)) => format!("{}…", text[..cut].trim_end()),
        None => text.to_owned(),
    })
}

/// Every occurrence of every non-empty value reads `[withheld]`.
fn withhold<'v>(mut text: String, values: impl IntoIterator<Item = &'v str>) -> String {
    for value in values {
        if !value.is_empty() && text.contains(value) {
            text = text.replace(value, WITHHELD);
        }
    }
    text
}

/// One line: a run of controls and whitespace becomes one space, invisible characters vanish.
fn one_line(text: &str) -> String {
    let mut line = String::with_capacity(text.len());
    for c in text.chars().filter(|&c| !invisible(c)) {
        if c.is_control() || c.is_whitespace() {
            if !line.is_empty() && !line.ends_with(' ') {
                line.push(' ');
            }
        } else {
            line.push(c);
        }
    }
    line.truncate(line.trim_end().len());
    line
}

/// Characters that change what a terminal shows without being seen: direction marks,
/// overrides and isolates, zero-width characters, tags and variation selectors.
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{AD}'
            | '\u{61C}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
            | '\u{E0000}'..='\u{E007F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// Every credential-shaped or credential-labelled word reads `[withheld]`; the rest is kept.
fn without_credential_shapes(line: &str) -> String {
    let mut shown = String::with_capacity(line.len());
    let mut previous = "";
    let mut rest = line;
    while let Some(start) = rest.find(word_char) {
        let (gap, tail) = rest.split_at(start);
        let end = tail.find(|c: char| !word_char(c)).unwrap_or(tail.len());
        let (word, after) = tail.split_at(end);
        let labelled = previous.eq_ignore_ascii_case("bearer")
            || (gap.contains([':', '=']) && names_a_credential(previous));
        shown.push_str(gap);
        shown.push_str(if labelled || credential_shaped(word) {
            WITHHELD
        } else {
            word
        });
        previous = word;
        rest = after;
    }
    shown.push_str(rest);
    shown
}

/// The characters keys, tokens and masked keys are written with.
fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~' | '+' | '/' | '*' | '%')
}

/// `api_key`, `Authorization`, `x-amz-signature`…: a word naming a credential.
fn names_a_credential(word: &str) -> bool {
    let word = word.to_ascii_lowercase();
    (CREDENTIAL_LABELS.split_whitespace()).any(|label| word.ends_with(label))
}

/// A key or token by its shape: a known prefix, a run of 32 or more letters and
/// digits, a run of 16 or more mixing both, or a UUID. A shape filter, not a
/// general secret detector: the credential the call sent is withheld by value.
fn credential_shaped(word: &str) -> bool {
    let prefixed = (KEY_PREFIXES.split_whitespace()).any(|prefix| {
        word.strip_prefix(prefix)
            .is_some_and(|rest| rest.len() >= 6)
    });
    let runs: Vec<&str> = (word.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|run| !run.is_empty())
        .collect();
    let random = runs.iter().any(|run| {
        let digits = run.bytes().any(|b| b.is_ascii_digit());
        let letters = run.bytes().any(|b| b.is_ascii_alphabetic());
        run.len() >= 32 || (run.len() >= 16 && digits && letters)
    });
    let uuid = runs.windows(5).any(|five| {
        (five.iter().zip([8, 4, 4, 4, 12]))
            .all(|(run, len)| run.len() == len && run.bytes().all(|b| b.is_ascii_hexdigit()))
    });
    prefixed || random || uuid
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

#[cfg(test)]
mod tests;
