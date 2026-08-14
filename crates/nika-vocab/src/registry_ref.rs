// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `registry:` reference grammar — ONE parser, two readers.
//!
//! A `registry:` target is read at two moments by two crates, and until
//! 2026-08-15 each carried its own parser. They disagreed **in both
//! directions**, which is worse than disagreeing in one:
//!
//! ```text
//! registry:acme/flows@nightly   the CHECK passed it · the CLIENT refused
//! registry:acme/flows           the CHECK refused it · the CLIENT passed
//! ```
//!
//! The first is the dangerous half — a check that says « clean » about a
//! reference the resolver will reject is a check that LIES, and it lies
//! at the only moment an author is still reading. The check also had no
//! charset rule and no `SemVer` rule at all, so `ACME/Flows@nightly` sailed
//! through it.
//!
//! They split on `@` differently too (`rsplit_once` vs `split_once`), so
//! `owner/name@1.0@2.0` parsed as two different references.
//!
//! ## Why the grammar lives HERE and not in the client
//!
//! The obvious home is the client — it owns resolution. But
//! `nika-registry-client` is **L2** and `nika-check` is **L0**: the check
//! cannot depend on the client without an upward dep the layering gate
//! refuses. `nika-vocab` is L0 and **both already depend on it**, so the
//! shared grammar costs neither crate a new edge.
//!
//! ## What stays out
//!
//! Version ORDERING (which of two versions is newer) is the client's —
//! it belongs to resolution, not to the grammar. This module answers
//! « is this a version at all »; the client answers « which one wins ».
//! `the_two_readers_agree_on_every_version` (client-side) pins that the
//! two never disagree about validity.

/// The parsed parts of a `registry:owner/name[@version]` reference —
/// borrowed, so neither reader pays an allocation to ask a question.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RegistryRef<'a> {
    /// The publisher — what pins WHOSE artifact you get.
    pub owner: &'a str,
    /// The artifact name.
    pub name: &'a str,
    /// The pinned version, when the reference carries one.
    pub version: Option<&'a str>,
}

impl<'a> RegistryRef<'a> {
    /// The constructor the `#[non_exhaustive]` marker requires
    /// (invariant #19).
    #[must_use]
    pub const fn new(owner: &'a str, name: &'a str, version: Option<&'a str>) -> Self {
        Self {
            owner,
            name,
            version,
        }
    }
}

/// Why a string is not a reference — each arm carries the sentence the
/// author reads, so both readers refuse in the SAME words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RefDefect {
    /// No `owner/name` split.
    MissingOwner,
    /// More than one `/`.
    TooManySegments,
    /// The owner is not a GitHub owner.
    BadOwner,
    /// The name is not a registry name.
    BadName,
    /// The version is not plain `SemVer`.
    BadVersion,
}

impl RefDefect {
    /// The teaching sentence — one voice for both readers.
    #[must_use]
    pub const fn teaching(self) -> &'static str {
        match self {
            Self::MissingOwner => {
                "expected owner/name (the publisher is required — it is what pins WHOSE artifact you get)"
            }
            Self::TooManySegments => "expected exactly owner/name — one `/`",
            Self::BadOwner => {
                "the owner is a GitHub owner: letters, digits and `-`, not starting with `-`"
            }
            Self::BadName => {
                "the name is lowercase letters, digits and `-` (up to 64, starting alphanumeric)"
            }
            Self::BadVersion => {
                "the version is plain SemVer, e.g. 1.2.0 or 1.2.0-rc.1 (no `v` prefix, no build metadata)"
            }
        }
    }
}

/// Parse the part AFTER the `registry:` scheme.
///
/// The `@` split takes the FIRST `@`, so a second one lands inside the
/// version and is refused there rather than silently becoming part of
/// the name.
///
/// # Errors
/// [`RefDefect`] — each arm carries its own teaching sentence.
pub fn parse(rest: &str) -> Result<RegistryRef<'_>, RefDefect> {
    let (locator, version) = match rest.split_once('@') {
        Some((l, v)) => (l, Some(v)),
        None => (rest, None),
    };
    let Some((owner, name)) = locator.split_once('/') else {
        return Err(RefDefect::MissingOwner);
    };
    if name.contains('/') {
        return Err(RefDefect::TooManySegments);
    }
    if !valid_owner(owner) {
        return Err(RefDefect::BadOwner);
    }
    if !valid_name(name) {
        return Err(RefDefect::BadName);
    }
    if version.is_some_and(|v| !is_plain_semver(v)) {
        return Err(RefDefect::BadVersion);
    }
    Ok(RegistryRef::new(owner, name, version))
}

/// A GitHub owner — letters, digits and `-`, never leading `-`.
#[must_use]
pub fn valid_owner(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// A registry artifact name — lowercase, digits and `-`, ≤64, starting
/// alphanumeric.
#[must_use]
pub fn valid_name(s: &str) -> bool {
    s.len() <= 64
        && s.as_bytes()
            .first()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// One core component — a bare `u64`, so `01` and `1e3` both refuse.
fn is_u64(s: &str) -> bool {
    s.parse::<u64>().is_ok()
}

/// Is this plain `SemVer`? `MAJOR.MINOR.PATCH` with an optional
/// `-prerelease`, and **no build metadata** — a pin must name ONE thing,
/// and `1.0.0+a` and `1.0.0+b` are the same version by `SemVer`'s own
/// ordering rules.
#[must_use]
pub fn is_plain_semver(v: &str) -> bool {
    if v.contains('+') {
        return false;
    }
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (v, None),
    };
    let mut nums = core.split('.');
    let three = [nums.next(), nums.next(), nums.next()];
    if nums.next().is_some() || three.iter().any(|n| !n.is_some_and(is_u64)) {
        return false;
    }
    match pre {
        None => true,
        Some("") => false,
        Some(pre) => pre
            .split('.')
            .all(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_form_parses() {
        let r = parse("acme/flows@1.2.0").expect("a pinned ref");
        assert_eq!(r.owner, "acme");
        assert_eq!(r.name, "flows");
        assert_eq!(r.version, Some("1.2.0"));
    }

    /// ⭐ The forms the CHECK used to wave through and the CLIENT then
    /// refused. One parser means one verdict — they are now refused at
    /// BOTH moments, and the author learns at the first.
    ///
    /// (No count in this sentence on purpose: a docstring that says
    /// « the four » above a list of five is the partial-pass defect, and
    /// it survives every review of the CONTENT.)
    #[test]
    fn the_forms_the_check_used_to_wave_through() {
        for (bad, why) in [
            ("acme/flows@nightly", "not SemVer"),
            ("acme/Flows@1.0.0", "name charset"),
            ("acme/flows@1.0.0+build", "build metadata"),
            ("acme/flows@1.0", "not three parts"),
            ("-acme/flows@1.0.0", "owner may not lead with `-`"),
        ] {
            assert!(parse(bad).is_err(), "`{bad}` must refuse — {why}");
        }
        // An UPPERCASE owner is legal — GitHub owners carry case
        // (`SuperNovae-studio`). Only the artifact NAME is lowercase.
        assert!(parse("ACME/flows@1.0.0").is_ok(), "owners carry case");
    }

    /// The `@` split takes the FIRST one, so a second `@` is refused as
    /// a bad version rather than folded into the name (the two readers
    /// used to split on opposite ends and parse two different refs).
    #[test]
    fn a_second_at_lands_in_the_version_and_refuses() {
        assert_eq!(parse("acme/flows@1.0@2.0"), Err(RefDefect::BadVersion));
    }

    /// An UNPINNED ref parses — the grammar has nothing against it. That
    /// it is REFUSED before a run is a rule of the check (spec 14 law 1
    /// · a call graph you cannot bound), not of the grammar, and the two
    /// are kept apart on purpose: the resolver legitimately reads
    /// unpinned refs through its own pin ladder.
    #[test]
    fn unpinned_is_a_grammar_yes_and_a_check_no() {
        let r = parse("acme/flows").expect("the grammar accepts it");
        assert_eq!(r.version, None);
    }

    #[test]
    fn prerelease_rides_but_an_empty_one_does_not() {
        assert!(is_plain_semver("1.2.0-rc.1"));
        assert!(!is_plain_semver("1.2.0-"));
        assert!(!is_plain_semver("v1.2.0"));
    }
}
