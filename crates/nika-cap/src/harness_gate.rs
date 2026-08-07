// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The harness permission-bridge JUDGE (D-2026-08-04-N1 · P3 B5) — the
//! pure half of the authority bridge, living beside the boundary it
//! consults (the `allows_*` predicates' sibling, so the bridge and the
//! static boundary can never drift).
//!
//! A harness asks permission in ITS vocabulary (an ACP `toolCall`); the
//! engine answers in ITS OWN (the declared `permits:` boundary). This
//! module is the translation's judge: the wire client extracts the
//! facts (kind · locations · command · url), and the verdict is only
//! ever **Inside** (the grants cover the ask — the bridge answers
//! `allow_once`, NEVER `allow_always`, A-5) or **Outside** (the run
//! pauses for the operator — the durable exit-4 gate).
//!
//! Fail-closed by construction: anything the judge cannot verify (a
//! prose `execute` under a program allowlist · a `read` with no
//! locations · a `fetch` whose host no shared parser extracted) is
//! OUTSIDE, never a guess. A kind the judge does not know is outside
//! too — a future ACP dialect earns its mapping deliberately.

use crate::permits::{ExecPermit, Permits};

/// The wire facts of ONE harness permission ask — what the agent SAID
/// it will do, extracted by the wire client (`nika-harness`) from the
/// ACP `toolCall` verbatim. Pure data, zero I/O (L0).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HarnessAskFacts {
    /// The ACP toolCall `kind` (`read` · `edit` · `delete` · `move` ·
    /// `search` · `execute` · `think` · `fetch` · `switch_mode` ·
    /// `other`) — absent when the agent declared none.
    pub kind: Option<String>,
    /// `toolCall.locations[].path` — the paths the action touches.
    pub locations: Vec<String>,
    /// `rawInput.command` in ARRAY form, for an `execute` ask — empty
    /// when the agent spoke prose only (or carried no command).
    pub command: Vec<String>,
    /// `rawInput.url`, for a `fetch` ask.
    pub url: Option<String>,
}

impl HarnessAskFacts {
    /// Construct (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach the toolCall kind.
    #[must_use]
    pub fn with_kind(mut self, kind: Option<String>) -> Self {
        self.kind = kind;
        self
    }

    /// Attach the touched paths.
    #[must_use]
    pub fn with_locations(mut self, locations: Vec<String>) -> Self {
        self.locations = locations;
        self
    }

    /// Attach the execute argv.
    #[must_use]
    pub fn with_command(mut self, command: Vec<String>) -> Self {
        self.command = command;
        self
    }

    /// Attach the fetch URL.
    #[must_use]
    pub fn with_url(mut self, url: Option<String>) -> Self {
        self.url = url;
        self
    }
}

/// The bridge's verdict on one ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessGate {
    /// The declared grants cover the ask — the bridge answers
    /// `allow_once` (never `allow_always` · A-5) and witnesses it.
    Inside {
        /// The authority plane exercised (`exec` · `fs` · `net` ·
        /// `agent` for the no-effect kinds).
        plane: &'static str,
        /// The grant law applied (the teaching one-liner).
        why: String,
    },
    /// Outside every grant — or unverifiable, which is the same thing
    /// under fail-closed. The run pauses for the operator; the harness
    /// hears `cancelled` until a human answers.
    Outside {
        /// The authority plane the ask WOULD have exercised.
        plane: &'static str,
        /// Why the grants do not cover it (the teaching one-liner).
        why: String,
    },
}

impl HarnessGate {
    /// The verdict's plane (witness stamping).
    #[must_use]
    pub fn plane(&self) -> &'static str {
        match self {
            Self::Inside { plane, .. } | Self::Outside { plane, .. } => plane,
        }
    }

    /// The verdict's teaching line (witness stamping · the pause's
    /// refusal detail).
    #[must_use]
    pub fn why(&self) -> &str {
        match self {
            Self::Inside { why, .. } | Self::Outside { why, .. } => why,
        }
    }
}

/// Judge ONE ask against the declared boundary. `None` permits is the
/// F-O8 zero-authority posture: every EFFECTFUL ask is outside (a
/// no-effect `think` still needs no grant).
#[must_use]
pub fn judge_harness_ask(facts: &HarnessAskFacts, permits: Option<&Permits>) -> HarnessGate {
    match facts.kind.as_deref() {
        // A think ask declares no effect reach — the one kind that
        // needs no grant (the pure-internal exemption's harness twin).
        Some("think") => HarnessGate::Inside {
            plane: "agent",
            why: "a think ask declares no effect reach".to_owned(),
        },
        Some("execute") => judge_execute(facts, permits),
        Some("read" | "search") => judge_fs(facts, permits, false),
        Some("edit" | "delete" | "move") => judge_fs(facts, permits, true),
        // The net plane narrows its claim honestly: no SHARED url→host
        // parser exists at L0 (hand-rolling one is the SSRF class of
        // bug), so a fetch ask is never auto-answered in P3 — the
        // operator judges it with the URL verbatim in the question.
        Some("fetch") => HarnessGate::Outside {
            plane: "net",
            why: "a fetch ask is never auto-judged in P3 (no shared host parser at \
                  L0) — the operator answers with the URL in view"
                .to_owned(),
        },
        // `switch_mode` · `other` · an undeclared kind · a kind from a
        // newer dialect: unverifiable = outside (fail-closed).
        other => {
            let named = other.unwrap_or("<none declared>");
            HarnessGate::Outside {
                plane: "agent",
                why: format!(
                    "harness ask kind `{named}` has no grant mapping — unverifiable asks \
                     pause for the operator (fail-closed)"
                ),
            }
        }
    }
}

fn judge_execute(facts: &HarnessAskFacts, permits: Option<&Permits>) -> HarnessGate {
    let program = facts.command.first();
    match (program, permits) {
        (Some(program), Some(permits)) if permits.allows_program(program) => HarnessGate::Inside {
            plane: "exec",
            why: format!("permits.exec covers `{program}`"),
        },
        (Some(program), Some(_)) => HarnessGate::Outside {
            plane: "exec",
            why: format!("program `{program}` is not in the permits.exec allowlist"),
        },
        (Some(program), None) => HarnessGate::Outside {
            plane: "exec",
            why: format!(
                "program `{program}` under an absent permits: block — zero authority (F-O8)"
            ),
        },
        // A prose-only execute: no argv[0] to match. Only the `Any`
        // grant covers an unenumerated command; a program allowlist
        // cannot verify prose (the shell-form law's bridge twin).
        (None, Some(permits)) if matches!(permits.exec, Some(ExecPermit::Any)) => {
            HarnessGate::Inside {
                plane: "exec",
                why: "exec: true covers an unenumerated command".to_owned(),
            }
        }
        (None, _) => HarnessGate::Outside {
            plane: "exec",
            why: "a prose execute carries no program to judge against the allowlist — \
                  unverifiable asks pause for the operator"
                .to_owned(),
        },
    }
}

fn judge_fs(facts: &HarnessAskFacts, permits: Option<&Permits>, write: bool) -> HarnessGate {
    let direction = if write { "write" } else { "read" };
    if facts.locations.is_empty() {
        return HarnessGate::Outside {
            plane: "fs",
            why: format!(
                "a {direction} ask with no locations is unverifiable — unverifiable asks \
                 pause for the operator"
            ),
        };
    }
    let Some(permits) = permits else {
        return HarnessGate::Outside {
            plane: "fs",
            why: format!(
                "{} under an absent permits: block — zero authority (F-O8)",
                facts.locations.join(", ")
            ),
        };
    };
    for path in &facts.locations {
        if !permits.allows_path(path, write) {
            return HarnessGate::Outside {
                plane: "fs",
                why: format!("`{path}` is not in the permits.fs.{direction} allowlist"),
            };
        }
    }
    HarnessGate::Inside {
        plane: "fs",
        why: format!("permits.fs.{direction} covers every location"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::permits::{FsPermits, NetPermits};

    fn facts(kind: &str) -> HarnessAskFacts {
        HarnessAskFacts::new().with_kind(Some(kind.to_owned()))
    }

    fn permits_exec_any() -> Permits {
        let mut p = Permits::new();
        p.exec = Some(ExecPermit::Any);
        p
    }

    fn permits_exec_git() -> Permits {
        let mut p = Permits::new();
        p.exec = Some(ExecPermit::Programs(vec!["git".to_owned()]));
        p
    }

    fn permits_fs() -> Permits {
        let mut p = Permits::new();
        p.fs = Some(FsPermits::new(
            vec!["src/**".to_owned()],
            vec!["out/**".to_owned()],
        ));
        p.net = Some(NetPermits::new(vec!["github.com".to_owned()]));
        p
    }

    // ── execute ──────────────────────────────────────────────────

    #[test]
    fn an_execute_with_a_listed_program_is_inside() {
        let mut f = facts("execute");
        f.command = vec!["git".to_owned(), "status".to_owned()];
        let v = judge_harness_ask(&f, Some(&permits_exec_git()));
        let HarnessGate::Inside { plane, why } = v else {
            panic!("listed program must be inside, got {v:?}");
        };
        assert_eq!(plane, "exec");
        assert!(why.contains("git"), "{why}");
    }

    #[test]
    fn an_execute_with_an_unlisted_program_is_outside() {
        let mut f = facts("execute");
        f.command = vec!["rm".to_owned(), "-rf".to_owned(), "/tmp/x".to_owned()];
        let v = judge_harness_ask(&f, Some(&permits_exec_git()));
        assert!(
            matches!(v, HarnessGate::Outside { plane: "exec", .. }),
            "{v:?}"
        );
    }

    #[test]
    fn a_prose_execute_needs_exec_true_never_a_program_list() {
        let f = facts("execute"); // no command extracted
        assert!(
            matches!(
                judge_harness_ask(&f, Some(&permits_exec_any())),
                HarnessGate::Inside { .. }
            ),
            "exec: true covers prose"
        );
        assert!(
            matches!(
                judge_harness_ask(&f, Some(&permits_exec_git())),
                HarnessGate::Outside { .. }
            ),
            "a program list cannot verify prose — fail-closed"
        );
        assert!(
            matches!(judge_harness_ask(&f, None), HarnessGate::Outside { .. }),
            "absent permits = zero authority"
        );
    }

    // ── fs ───────────────────────────────────────────────────────

    #[test]
    fn a_read_inside_the_globs_is_inside_a_write_direction_is_not_shared() {
        let mut f = facts("read");
        f.locations = vec!["src/main.rs".to_owned()];
        assert!(matches!(
            judge_harness_ask(&f, Some(&permits_fs())),
            HarnessGate::Inside { plane: "fs", .. }
        ));
        // The SAME path as an edit asks the WRITE direction — denied.
        let mut w = facts("edit");
        w.locations = vec!["src/main.rs".to_owned()];
        assert!(matches!(
            judge_harness_ask(&w, Some(&permits_fs())),
            HarnessGate::Outside { .. }
        ));
    }

    #[test]
    fn every_location_must_fit_one_miss_is_outside() {
        let mut f = facts("edit");
        f.locations = vec!["out/report.md".to_owned(), "src/lib.rs".to_owned()];
        let v = judge_harness_ask(&f, Some(&permits_fs()));
        let HarnessGate::Outside { why, .. } = v else {
            panic!("one path outside the globs fails the whole ask, got {v:?}");
        };
        assert!(
            why.contains("src/lib.rs"),
            "the failing path is named: {why}"
        );
    }

    #[test]
    fn an_fs_ask_without_locations_is_unverifiable() {
        let f = facts("search"); // no locations
        assert!(matches!(
            judge_harness_ask(&f, Some(&permits_fs())),
            HarnessGate::Outside { plane: "fs", .. }
        ));
    }

    // ── net / think / unknown ────────────────────────────────────

    #[test]
    fn a_fetch_is_never_auto_judged_in_p3() {
        let mut f = facts("fetch");
        f.url = Some("https://github.com/x".to_owned());
        let v = judge_harness_ask(&f, Some(&permits_fs()));
        let HarnessGate::Outside { plane, why } = v else {
            panic!("fetch defers to the operator in P3, got {v:?}");
        };
        assert_eq!(plane, "net");
        assert!(why.contains("operator"), "{why}");
    }

    #[test]
    fn a_think_needs_no_grant_even_under_zero_authority() {
        let f = facts("think");
        assert!(matches!(
            judge_harness_ask(&f, None),
            HarnessGate::Inside { plane: "agent", .. }
        ));
    }

    #[test]
    fn an_unknown_or_absent_kind_is_outside_fail_closed() {
        for f in [
            facts("switch_mode"),
            facts("other"),
            facts("teleport"),
            HarnessAskFacts::new(),
        ] {
            assert!(
                matches!(
                    judge_harness_ask(&f, Some(&permits_exec_any())),
                    HarnessGate::Outside { .. }
                ),
                "{f:?} must pause for the operator"
            );
        }
    }

    #[test]
    fn zero_authority_refuses_every_effectful_kind() {
        for kind in [
            "execute", "read", "search", "edit", "delete", "move", "fetch",
        ] {
            let mut f = facts(kind);
            f.command = vec!["git".to_owned()];
            f.locations = vec!["src/x".to_owned()];
            f.url = Some("https://github.com".to_owned());
            assert!(
                matches!(judge_harness_ask(&f, None), HarnessGate::Outside { .. }),
                "{kind} under absent permits must be outside"
            );
        }
    }

    #[test]
    fn the_accessors_read_both_halves() {
        let inside = HarnessGate::Inside {
            plane: "exec",
            why: "w".to_owned(),
        };
        let outside = HarnessGate::Outside {
            plane: "fs",
            why: "z".to_owned(),
        };
        assert_eq!(inside.plane(), "exec");
        assert_eq!(outside.plane(), "fs");
        assert_eq!(inside.why(), "w");
        assert_eq!(outside.why(), "z");
    }

    /// Every builder sets exactly its field (the mutation-killers for
    /// the `with_*` row — a builder replaced by a default is a facts
    /// hole the bridge would judge blind).
    #[test]
    fn the_fact_builders_set_exactly_their_field() {
        let f = HarnessAskFacts::new()
            .with_kind(Some("execute".to_owned()))
            .with_locations(vec!["a.rs".to_owned()])
            .with_command(vec!["git".to_owned()])
            .with_url(Some("https://x.sh".to_owned()));
        assert_eq!(f.kind.as_deref(), Some("execute"));
        assert_eq!(f.locations, vec!["a.rs"]);
        assert_eq!(f.command, vec!["git"]);
        assert_eq!(f.url.as_deref(), Some("https://x.sh"));
        let bare = HarnessAskFacts::new();
        assert!(
            bare.kind.is_none()
                && bare.locations.is_empty()
                && bare.command.is_empty()
                && bare.url.is_none()
        );
    }
}
