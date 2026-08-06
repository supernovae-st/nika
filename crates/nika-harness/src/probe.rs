// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The adapter VERSION probe (spec §4 · the promise B3.2 owed) — the
//! binary's identity is judged BEFORE a session, never discovered
//! mid-run.
//!
//! Why before: an adapter outside its pin range speaks a dialect this
//! client did not read. Learning that at `initialize` wastes a spawn
//! and reports a protocol confusion where the truth is a VERSION
//! mismatch. The probe runs `<command> --version`, parses the first
//! semver-ish token out of whatever the CLI prints (every harness
//! prints its own prose around it), and judges it against the
//! adapter's declared range.
//!
//! What the probe is NOT: a credential read, a network call, or a
//! session. It spawns the same confined child shape the session does
//! (composed env · no shell · bounded output) and reads one line.

use nika_kernel::ai::harness::HarnessError;

/// The inclusive version range an adapter is pinned to — the
/// schema-diff gate's binary-side twin (spec §2).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VersionPin {
    /// Lowest accepted `(major, minor)`.
    pub min: (u32, u32),
    /// Highest accepted MAJOR — a new major means a new dialect until
    /// this client is taught otherwise.
    pub max_major: u32,
}

impl VersionPin {
    /// Construct (INV-019).
    #[must_use]
    pub const fn new(min: (u32, u32), max_major: u32) -> Self {
        Self { min, max_major }
    }

    /// Whether `(major, minor)` sits inside the pin.
    #[must_use]
    pub const fn accepts(&self, major: u32, minor: u32) -> bool {
        if major > self.max_major {
            return false;
        }
        if major < self.min.0 {
            return false;
        }
        !(major == self.min.0 && minor < self.min.1)
    }
}

/// Parse the first `MAJOR.MINOR[.PATCH]` token out of a `--version`
/// line. Harness CLIs print prose around it (`codex-acp 1.4.0
/// (build …)` · `gemini-cli version 0.9.1`), so the parse is
/// scan-for-the-first-triple, never a whole-line format assumption.
#[must_use]
pub fn parse_version(line: &str) -> Option<(u32, u32)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        // A digit run must not begin mid-token (`v1.2` is fine, `x1.2`
        // is a name, `1.2` after a dot is a continuation).
        if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'v') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let major: u32 = line.get(start..i)?.parse().ok()?;
        if bytes.get(i) != Some(&b'.') {
            continue;
        }
        i += 1;
        let mstart = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == mstart {
            continue;
        }
        let minor: u32 = line.get(mstart..i)?.parse().ok()?;
        return Some((major, minor));
    }
    None
}

/// Judge a probe's raw output against a pin — the pure half (the
/// spawn half lives in [`crate::spawn`], so THIS is what tests drive).
///
/// # Errors
///
/// [`HarnessError::Unavailable`] when nothing parses (the binary is
/// not the adapter we think) or when the version sits outside the pin.
pub fn judge_version(
    adapter_id: &str,
    output: &str,
    pin: &VersionPin,
) -> Result<(u32, u32), HarnessError> {
    let Some((major, minor)) = output.lines().find_map(parse_version) else {
        return Err(HarnessError::Unavailable {
            reason: format!(
                "adapter `{adapter_id}`: `--version` printed no version \
                 ({:?}) — this binary is not the adapter it claims to be",
                output.lines().next().unwrap_or("").trim()
            ),
        });
    };
    if pin.accepts(major, minor) {
        return Ok((major, minor));
    }
    Err(HarnessError::Unavailable {
        reason: format!(
            "adapter `{adapter_id}`: version {major}.{minor} is outside the \
             pin (>= {}.{} · major <= {}) — a version this client has not \
             read speaks a dialect it cannot judge",
            pin.min.0, pin.min.1, pin.max_major
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parse_finds_the_triple_inside_real_cli_prose() {
        assert_eq!(
            parse_version("codex-acp 1.4.0 (build 2026-07)"),
            Some((1, 4))
        );
        assert_eq!(parse_version("gemini-cli version 0.9.1"), Some((0, 9)));
        assert_eq!(parse_version("v2.11.3"), Some((2, 11)));
        assert_eq!(parse_version("qwen-code 12.0"), Some((12, 0)));
        // Prose without a version parses to nothing — never a guess.
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version(""), None);
        // A bare integer is not a version (no minor).
        assert_eq!(parse_version("build 12345"), None);
    }

    #[test]
    fn the_pin_is_inclusive_at_the_floor_and_caps_the_major() {
        let pin = VersionPin::new((1, 2), 2);
        assert!(pin.accepts(1, 2), "the floor itself is accepted");
        assert!(pin.accepts(1, 9));
        assert!(pin.accepts(2, 0), "a same-major-cap version rides");
        assert!(!pin.accepts(1, 1), "below the floor minor");
        assert!(!pin.accepts(0, 9), "below the floor major");
        assert!(!pin.accepts(3, 0), "past the major cap — a new dialect");
    }

    #[test]
    fn an_unparseable_probe_names_the_adapter_and_what_it_saw() {
        let err = judge_version(
            "codex-acp",
            "bash: codex-acp: not found\n",
            &VersionPin::new((1, 0), 1),
        )
        .expect_err("no version parses");
        let msg = err.to_string();
        assert!(msg.contains("codex-acp"), "{msg}");
        assert!(msg.contains("not the adapter it claims"), "{msg}");
    }

    #[test]
    fn an_out_of_pin_version_refuses_with_both_sides_named() {
        let err = judge_version("codex-acp", "codex-acp 3.0.0", &VersionPin::new((1, 0), 2))
            .expect_err("major past the cap");
        let msg = err.to_string();
        assert!(msg.contains("3.0"), "{msg}");
        assert!(msg.contains("major <= 2"), "{msg}");
    }

    #[test]
    fn a_probe_inside_the_pin_returns_what_it_read() {
        let seen = judge_version(
            "gemini-cli",
            "gemini-cli version 0.9.1\nextra prose\n",
            &VersionPin::new((0, 8), 0),
        )
        .expect("inside the pin");
        assert_eq!(seen, (0, 9));
    }

    #[test]
    fn the_version_rides_the_first_line_that_carries_one() {
        // A CLI that greets before versioning: the probe scans lines,
        // it does not assume line 1 (a real-world shape).
        let seen = judge_version(
            "noisy",
            "warning: config file missing\nnoisy 4.2.0\n",
            &VersionPin::new((4, 0), 4),
        )
        .expect("second line carries it");
        assert_eq!(seen, (4, 2));
    }
}
