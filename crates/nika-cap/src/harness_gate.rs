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
    /// The network target an ask opens (NIK-266), read by the wire client from every
    /// structured signal it carries: `rawInput.url`, a Codex network approval's
    /// `<protocol> network access to <host>` (as `<protocol>://<host>`), its
    /// `Allow network access?` request title, a network additional permission. `None`: the
    /// ask names no network access.
    pub network: Option<String>,
    /// The widening of the harness's OWN sandbox the ask asks for its command (Codex
    /// `additionalPermissions`, Claude Code `dangerouslyDisableSandbox`), summarized. No grant
    /// covers it, `exec: true` included. `None`: the ask asks no widening.
    pub widens: Option<String>,
    /// The harness's own MCP tool call the ask approves (codex-acp's
    /// `_meta.is_mcp_tool_approval`): its server, else the tool call it refers to. `None`: the
    /// ask approves no MCP tool call.
    pub mcp: Option<String>,
    /// The command the ask asks to run OUTSIDE the harness's own sandbox, with the reason the
    /// harness gave (NIK-274: codex-acp `read-only` asks only for such commands). No grant
    /// covers it, `exec: true` included. `None`: the ask names no such command.
    pub unsandboxed: Option<String>,
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

    /// Attach the network target the ask opens.
    #[must_use]
    pub fn with_network(mut self, network: Option<String>) -> Self {
        self.network = network;
        self
    }

    /// Attach the sandbox widening the ask asks.
    #[must_use]
    pub fn with_widens(mut self, widens: Option<String>) -> Self {
        self.widens = widens;
        self
    }

    /// Attach the harness's own MCP tool call the ask approves.
    #[must_use]
    pub fn with_mcp(mut self, mcp: Option<String>) -> Self {
        self.mcp = mcp;
        self
    }

    /// Attach the command the ask asks to run outside the harness's own sandbox.
    #[must_use]
    pub fn with_unsandboxed(mut self, command: Option<String>) -> Self {
        self.unsandboxed = command;
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
    // NIK-266 · what an ask reaches BEYOND its kind is judged first, whatever the kind: no
    // grant answers network access the harness asks for, nor a wider harness sandbox.
    if let Some(gate) = judge_escalation(facts) {
        return gate;
    }
    match facts.kind.as_deref() {
        // A think ask declares no effect reach — the one kind that
        // needs no grant (the pure-internal exemption's harness twin).
        Some("think") => HarnessGate::Inside {
            plane: "agent",
            why: "a think ask declares no effect reach".to_owned(),
        },
        // An execute ask that names a URL asks for NETWORK access for a
        // command (codex-acp's network approval: `rawInput.url` beside the
        // command, NIK-266): the net plane judges it, never the program
        // allowlist — a listed program must not carry a socket out.
        Some("execute") if facts.url.is_some() => HarnessGate::Outside {
            plane: "net",
            why: "an execute ask that opens network access is judged on the net plane, \
                  never by its program — never auto-judged (no shared host parser at L0); \
                  the operator answers with the URL in view"
                .to_owned(),
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

/// One of the harness's own MCP tool calls, network access, a sandbox widening and a command
/// run outside the sandbox, what an ask may reach beyond its own kind: never auto-judged, the
/// operator answers with it in view.
fn judge_escalation(facts: &HarnessAskFacts) -> Option<HarnessGate> {
    // The server or tool call a harness names is ITS configuration, not a `permits.tools` id
    // (`mcp:<server>/<tool>` names the workflow's own servers): no grant can be matched.
    if let Some(call) = facts.mcp.as_deref() {
        return Some(HarnessGate::Outside {
            plane: "tools",
            why: format!(
                "the ask approves one of the harness's own MCP tool calls ({call}): \
                 permits.tools names the workflow's MCP tools, not the harness's, so no grant \
                 covers it — the operator answers"
            ),
        });
    }
    let widening = facts
        .widens
        .as_deref()
        .map(|widens| format!("it asks the harness to widen its own sandbox ({widens})"))
        .into_iter()
        .chain(facts.unsandboxed.as_deref().map(|command| {
            format!("it asks to run a command outside the harness's own sandbox ({command})")
        }))
        .reduce(|first, second| format!("{first}; {second}"))
        .map(|reach| format!("{reach}, which no grant covers, `exec: true` included"));
    if let Some(target) = facts.network.as_deref() {
        let mut why = format!(
            "the ask opens network access ({target}): never auto-judged (no shared host parser \
             at L0), the operator answers with the target in view"
        );
        if let Some(widening) = widening {
            why.push_str("; ");
            why.push_str(&widening);
        }
        return Some(HarnessGate::Outside { plane: "net", why });
    }
    widening.map(|widening| HarnessGate::Outside {
        plane: "exec",
        why: format!("{widening}: the operator answers"),
    })
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
        // An identity the client could not resolve is never a path a grant admits (NIK-266):
        // a `**` glob would match the marker's text, so it is refused before any glob runs.
        if path.starts_with(nika_types::access::HARNESS_UNRESOLVED_PATH) {
            return HarnessGate::Outside {
                plane: "fs",
                why: format!(
                    "{path}: the file this path reaches could not be resolved — unverifiable \
                     asks pause for the operator"
                ),
            };
        }
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

    /// NIK-266 · codex-acp asks network access for a command as an
    /// `execute` carrying `rawInput.url`: a granted program must not admit
    /// the socket — the net plane judges it, outside like every fetch.
    #[test]
    fn an_execute_that_opens_the_network_is_a_net_ask_never_an_exec_grant() {
        let mut f = facts("execute");
        f.command = vec!["git".to_owned(), "push".to_owned()];
        f.url = Some("https://github.com".to_owned());
        for permits in [permits_exec_git(), permits_exec_any(), permits_fs()] {
            let v = judge_harness_ask(&f, Some(&permits));
            let HarnessGate::Outside { plane, why } = v else {
                panic!("a network execute is never inside an exec grant, got {v:?}");
            };
            assert_eq!(plane, "net");
            assert!(why.contains("net plane"), "{why}");
        }
        // The same argv without a URL stays the exec plane's to judge.
        f.url = None;
        assert!(matches!(
            judge_harness_ask(&f, Some(&permits_exec_git())),
            HarnessGate::Inside { plane: "exec", .. }
        ));
    }

    /// NIK-266 · a Codex SOCKS network approval carries no `rawInput.url`: the wire client
    /// reads its target from the approval's title, and the net plane judges it — never the
    /// `exec: true` grant that used to answer `allow_once`.
    #[test]
    fn a_socks_network_ask_without_a_url_is_never_inside_an_exec_grant() {
        let mut f = facts("execute");
        f.network = Some("socks5Tcp://exfil.example".to_owned());
        for permits in [permits_exec_any(), permits_exec_git(), permits_fs()] {
            let v = judge_harness_ask(&f, Some(&permits));
            let HarnessGate::Outside { plane, why } = v else {
                panic!("a network ask is never inside, got {v:?}");
            };
            assert_eq!(plane, "net");
            assert!(why.contains("socks5Tcp://exfil.example"), "{why}");
        }
    }

    /// NIK-266 · a command that asks the harness to widen its own sandbox pauses under every
    /// grant, `exec: true` included; with network among the widening it is a net ask.
    #[test]
    fn a_sandbox_widening_is_never_covered_by_exec_true() {
        let mut fs_only = facts("execute");
        fs_only.widens = Some("write /".to_owned());
        let v = judge_harness_ask(&fs_only, Some(&permits_exec_any()));
        let HarnessGate::Outside { plane, why } = v else {
            panic!("a widening is never inside exec: true, got {v:?}");
        };
        assert_eq!(plane, "exec");
        assert!(
            why.contains("write /") && why.contains("`exec: true` included"),
            "{why}"
        );

        let mut with_network = facts("execute");
        with_network.widens = Some("network, write /".to_owned());
        with_network.network = Some("network access (an additional permission)".to_owned());
        let v = judge_harness_ask(&with_network, Some(&permits_exec_any()));
        let HarnessGate::Outside { plane, why } = v else {
            panic!("a network widening is never inside, got {v:?}");
        };
        assert_eq!(plane, "net");
        assert!(why.contains("an additional permission") && why.contains("network, write /"));
    }

    /// NIK-266 · an identity the client could not resolve is outside every grant, even one
    /// whose glob would match the marker's text (`**` · `./**` · `/**`).
    #[test]
    fn an_unresolved_location_is_outside_every_grant() {
        let mut everything = Permits::new();
        everything.fs = Some(FsPermits::new(
            vec!["**".to_owned(), "/**".to_owned()],
            vec!["**".to_owned(), "./**".to_owned(), "/**".to_owned()],
        ));
        for kind in ["edit", "read"] {
            let mut f = facts(kind);
            f.locations = vec![
                "out/l1/f.md".to_owned(),
                format!(
                    "{} out/l1/f.md · a symlink loop",
                    nika_types::access::HARNESS_UNRESOLVED_PATH
                ),
            ];
            let v = judge_harness_ask(&f, Some(&everything));
            let HarnessGate::Outside { plane, why } = v else {
                panic!("an unresolved identity is never inside, got {v:?}");
            };
            assert_eq!(plane, "fs");
            assert!(why.contains("could not be resolved"), "{why}");
        }
        // The same grants still admit the resolvable path alone (the control).
        let mut f = facts("edit");
        f.locations = vec!["out/l1/f.md".to_owned()];
        assert!(matches!(
            judge_harness_ask(&f, Some(&everything)),
            HarnessGate::Inside { .. }
        ));
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

    /// The NIK-266 builders set exactly their field, and a bare fact set names none.
    #[test]
    fn the_escalation_builders_set_exactly_their_field() {
        let f = HarnessAskFacts::new()
            .with_network(Some("socks5Tcp://h".to_owned()))
            .with_widens(Some("write /".to_owned()));
        assert_eq!(f.network.as_deref(), Some("socks5Tcp://h"));
        assert_eq!(f.widens.as_deref(), Some("write /"));
        assert!(f.kind.is_none() && f.url.is_none() && f.command.is_empty() && f.mcp.is_none());
        let mcp = HarnessAskFacts::new().with_mcp(Some("web".to_owned()));
        assert_eq!(mcp.mcp.as_deref(), Some("web"));
        assert!(mcp.network.is_none() && mcp.widens.is_none() && mcp.unsandboxed.is_none());
        let run = HarnessAskFacts::new().with_unsandboxed(Some("make".to_owned()));
        assert_eq!(run.unsandboxed.as_deref(), Some("make"));
        assert!(run.mcp.is_none() && run.widens.is_none());
        let bare = HarnessAskFacts::new();
        assert!(bare.network.is_none() && bare.widens.is_none() && bare.mcp.is_none());
    }

    /// NIK-274 · in codex-acp `read-only` an asked command asks to leave Codex's sandbox (an
    /// approved one runs with none): `exec: true` answered it `allow_once` and Codex fetched a
    /// site outside its sandbox. It pauses on the exec plane under every grant, the command
    /// and its reason in the why.
    #[test]
    fn a_command_run_outside_the_sandbox_is_never_covered_by_exec() {
        let command = "curl -sS https://example.com — reason: Fetch example.com for its title";
        for program in [vec![], vec!["curl".to_owned()]] {
            let mut f = facts("execute");
            f.command = program;
            f.unsandboxed = Some(command.to_owned());
            for permits in [Some(permits_exec_any()), Some(permits_exec_git()), None] {
                let v = judge_harness_ask(&f, permits.as_ref());
                let HarnessGate::Outside { plane, why } = v else {
                    panic!("a command outside the sandbox is never inside, got {v:?}");
                };
                assert_eq!(plane, "exec");
                assert!(
                    why.contains(command) && why.contains("`exec: true` included"),
                    "{why}"
                );
            }
        }
        // Beside a widening, both are named.
        let mut both = facts("execute");
        both.widens = Some("write /".to_owned());
        both.unsandboxed = Some("make install".to_owned());
        let v = judge_harness_ask(&both, Some(&permits_exec_any()));
        let HarnessGate::Outside { plane, why } = v else {
            panic!("never inside, got {v:?}");
        };
        assert_eq!(plane, "exec");
        assert!(
            why.contains("write /") && why.contains("make install"),
            "{why}"
        );
    }

    /// NIK-266 · codex-acp asks to approve its own MCP tool calls as `execute` asks with no
    /// command: they were answered `allow_once` by `exec: true`. They are judged on the tools
    /// plane and never inside a grant, `permits.tools` included (its ids name the workflow's
    /// MCP servers, not the harness's).
    #[test]
    fn an_mcp_tool_approval_is_a_tools_ask_never_an_exec_grant() {
        let mut everything = permits_exec_any();
        everything.tools = Some(vec!["*".to_owned(), "mcp:web/*".to_owned()]);
        for call in ["web", "tool call mcp-call-1"] {
            let mut f = facts("execute");
            f.mcp = Some(call.to_owned());
            for permits in [Some(&everything), Some(&permits_exec_any()), None] {
                let v = judge_harness_ask(&f, permits);
                let HarnessGate::Outside { plane, why } = v else {
                    panic!("an MCP tool approval is never inside, got {v:?}");
                };
                assert_eq!(plane, "tools");
                assert!(why.contains(call) && why.contains("permits.tools"), "{why}");
            }
        }
    }
}
