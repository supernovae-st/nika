// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a secret looks like, for an object the caller marked protected: a
//! key that names a credential, or a value that wears a credential's shape
//! (a known token prefix, a bearer header, a signed web token, a password
//! in a URL, a long mixed-case token). The judgement is a heuristic that
//! masks generously; a template reference (`${{ secrets.x }}`) stays
//! readable because a reference is not a secret. The masking walks the
//! object's structure with these judgements (`mask`).

/// The mask a secret-looking value shows as (the same length whatever it
/// hides).
pub(crate) const fn mask(ascii: bool) -> &'static str {
    if ascii {
        "******"
    } else {
        "••••••"
    }
}

/// Words that name a credential on their own.
const SECRET_WORDS: [&str; 16] = [
    "password",
    "passwd",
    "pwd",
    "passphrase",
    "secret",
    "secrets",
    "token",
    "tokens",
    "apikey",
    "credential",
    "credentials",
    "auth",
    "authorization",
    "cookie",
    "bearer",
    "signature",
];

/// Words that turn a neighbouring `key`, `id` or `token` into a credential.
const KEY_QUALIFIERS: [&str; 9] = [
    "api",
    "access",
    "private",
    "secret",
    "signing",
    "encryption",
    "client",
    "session",
    "master",
];

/// Known credential prefixes.
const PREFIXES: [&str; 26] = [
    "sk-",
    "sk_live_",
    "sk_test_",
    "rk_live_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "xoxr-",
    "xapp-",
    "AKIA",
    "ASIA",
    "AIza",
    "ya29.",
    "hf_",
    "npm_",
    "pypi-",
    "shpat_",
    "dop_v1_",
    "-----BEGIN",
];

/// The lower-case words of a key: split on punctuation and at camel-case
/// boundaries (`apiKey`, `APIKey` and `api_key` all read `api`, `key`).
fn words(key: &str) -> Vec<String> {
    let chars: Vec<char> = key.chars().take(128).collect();
    let mut out = Vec::new();
    let mut word = String::new();
    for (at, &c) in chars.iter().enumerate() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            continue;
        }
        let before = at.checked_sub(1).and_then(|p| chars.get(p)).copied();
        let after = chars.get(at + 1).copied();
        let boundary = c.is_uppercase()
            && before.is_some_and(|p| {
                p.is_lowercase()
                    || p.is_ascii_digit()
                    || (p.is_uppercase() && after.is_some_and(char::is_lowercase))
            });
        if boundary && !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        word.extend(c.to_lowercase());
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// Whether a key names a credential.
pub(crate) fn secret_key(key: &str) -> bool {
    let words = words(key);
    let has = |w: &str| words.iter().any(|x| x == w);
    words.iter().any(|w| SECRET_WORDS.contains(&w.as_str()))
        || (["key", "keys", "id", "ids"].iter().any(|w| has(w))
            && KEY_QUALIFIERS.iter().any(|q| has(q)))
}

/// Whether a value wears a credential's shape.
pub(crate) fn secret_value(value: &str) -> bool {
    let v = value.trim().trim_matches(['"', '\'']);
    if v.contains("${{") || v.len() < 8 {
        return false;
    }
    if PREFIXES.iter().any(|p| v.starts_with(p)) {
        return true;
    }
    let lower = v.to_ascii_lowercase();
    if lower.starts_with("bearer ") || lower.starts_with("basic ") {
        return true;
    }
    let segments: Vec<&str> = v.split('.').collect();
    if v.starts_with("eyJ") && segments.len() == 3 && segments.iter().all(|s| !s.is_empty()) {
        return true;
    }
    if let Some((_, rest)) = v.split_once("://") {
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if let Some((user, _)) = authority.split_once('@')
            && user.contains(':')
        {
            return true;
        }
    }
    let token_chars = v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-'));
    token_chars
        && v.len() >= 24
        && v.chars().any(|c| c.is_ascii_digit())
        && v.chars().any(|c| c.is_ascii_lowercase())
        && v.chars().any(|c| c.is_ascii_uppercase())
}

/// Whether `text` is one template reference and nothing else
/// (`${{ secrets.x }}`): a reference names a secret, it is not one.
pub(crate) fn reference(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("${{") && text.ends_with("}}") && text.matches("${{").count() == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_keys_are_recognised_by_their_words() {
        for key in [
            "password",
            "API_KEY",
            "apiKey",
            "APIKey",
            "api_keys",
            "accessKeyIds",
            "client_secret",
            "sessionId",
            "Authorization",
            "x-api-key",
            "github_token",
        ] {
            assert!(secret_key(key), "{key}");
        }
        for key in [
            "author",
            "keyboard",
            "name",
            "monkey",
            "primary_key_column",
            "id",
        ] {
            assert!(!secret_key(key), "{key}");
        }
    }

    #[test]
    fn credential_shapes_are_recognised_and_digests_are_not() {
        for value in [
            "sk-proj-abcdefghijklmnop",
            "ghp_0123456789abcdefghijABCDEFGHIJ",
            "Bearer abc.def",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig",
            "https://user:hunter2@example.com/x",
            "Xq7vR2mZp9LkW3sT8bNc4dYf",
        ] {
            assert!(secret_value(value), "{value}");
        }
        for value in [
            "9f3c1a0b9f3c1a0b9f3c1a0b9f3c1a0b9f3c1a0b9f3c1a0b9f3c1a0b9f3c1a0b",
            "550e8400-e29b-41d4-a716-446655440000",
            "${{ secrets.api_key }}",
            "https://example.com/path",
            "hello world",
        ] {
            assert!(!secret_value(value), "{value}");
        }
    }

    #[test]
    fn only_a_whole_reference_is_a_reference() {
        assert!(reference("${{ secrets.api_key }}"));
        assert!(reference("  ${{ inputs.x }} "));
        assert!(!reference("${{ secrets.a }} sk-live-0123456789"));
        assert!(!reference("${{ a }}${{ b }}"));
        assert!(!reference("plain"));
    }
}
