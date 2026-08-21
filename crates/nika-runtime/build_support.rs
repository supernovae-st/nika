// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

/// Parse one exact lowercase Git SHA from a comment-bearing identity file.
pub(crate) fn parse_spec_sha<'a>(label: &str, raw: &'a str) -> Result<&'a str, String> {
    let values: Vec<_> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let [sha] = values.as_slice() else {
        return Err(format!("{label} must contain exactly one identity"));
    };
    if sha.len() != 40
        || !sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{label} identity must be a 40-character lowercase Git SHA"
        ));
    }
    Ok(sha)
}

/// Bind the conformance pin and embedded pack to one source commit.
pub(crate) fn matching_spec_sha<'a>(pin: &'a str, pack: &str) -> Result<&'a str, String> {
    let pin = parse_spec_sha("SPEC_PIN", pin)?;
    let pack = parse_spec_sha("pack/SPEC_SHA", pack)?;
    if pin != pack {
        return Err(format!(
            "SPEC_PIN {pin} differs from embedded pack identity {pack}; run scripts/sync-pack.sh <spec-checkout-at-SPEC_PIN>"
        ));
    }
    Ok(pin)
}

#[cfg(test)]
mod tests {
    use super::{matching_spec_sha, parse_spec_sha};

    const SHA: &str = "9fb39f0978562c1cf06ad7cb0acc680c6b455833";

    #[test]
    fn one_lowercase_sha_is_the_only_identity_shape() {
        assert_eq!(parse_spec_sha("pin", &format!("# pin\n{SHA}\n")), Ok(SHA));
        for bad in [
            "",
            "# comments only\n",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "9FB39F0978562C1CF06AD7CB0ACC680C6B455833",
            "gggggggggggggggggggggggggggggggggggggggg",
        ] {
            assert!(parse_spec_sha("pin", bad).is_err(), "accepted {bad:?}");
        }
        assert!(parse_spec_sha("pin", &format!("{SHA}\n{SHA}\n")).is_err());
    }

    #[test]
    fn pin_and_pack_must_name_the_same_commit() {
        assert_eq!(matching_spec_sha(SHA, SHA), Ok(SHA));
        assert!(matching_spec_sha(SHA, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_err());
    }
}
