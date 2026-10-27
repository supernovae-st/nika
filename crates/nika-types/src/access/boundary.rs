// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What Nika can hold a harness seat's `agent:` session to (NIK-266) — the ONE table the
//! client (which selects and reads back the mode) and `nika check` (which tells the author
//! what the boundary covers) both read, so the two can never drift.
//!
//! A harness runs its OWN tools. Nika's authority over it is two levers: the session mode it
//! selects before the prompt, and its answer to every permission the harness asks. A mode is
//! a row here only when it was MEASURED on a named adapter release (its source and its
//! `session/new` answer) to send its approvals to the client; a mode that approves on its own
//! (an auto-reviewer, accept-edits, full access, bypass, yolo) is never a row. What the
//! measured mode still leaves to the harness itself, without asking, is stated in the row —
//! Nika cannot see or judge it.

use alloc::format;
use alloc::string::String;

/// One measured confinement of a harness adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct HarnessBoundary {
    /// The seat token this adapter serves ([`super::HarnessRuntime::id`]).
    pub seat: &'static str,
    /// The `agentInfo.name` values the measurement holds for — the adapter's own report on
    /// `initialize`, never the command that spawned it.
    pub agents: &'static [&'static str],
    /// The adapter release the measurement was made on.
    pub measured_on: &'static str,
    /// The mode selected when the workflow grants no `fs.write`.
    pub read_only_mode: &'static str,
    /// The mode selected when the workflow grants `fs.write`.
    pub workspace_write_mode: &'static str,
    /// What the harness reads without asking, under the selected mode.
    pub reads_unasked: &'static str,
    /// Where it writes without asking, under the selected mode (`None`: every write asks).
    pub writes_unasked: Option<&'static str>,
    /// What it runs without asking, under the selected mode (`None`: every command asks).
    pub runs_unasked: Option<&'static str>,
    /// What it fetches without asking, under the selected mode (`None`: every fetch asks).
    pub fetches_unasked: Option<&'static str>,
    /// What it loads from the user's own configuration on its own, at every session (`None`:
    /// nothing measured).
    pub loads_unasked: Option<&'static str>,
    /// Whether the selected mode runs ordinary commands in the harness's own sandbox without
    /// asking, so a command it asks about asks to LEAVE that sandbox (NIK-274): the client
    /// sends such an ask as an escalation, which no grant answers, `exec: true` included.
    pub asked_command_leaves_sandbox: bool,
    /// Whether the client recognises this adapter's image tool, so an image the harness
    /// generates reaches the task with its bytes through `images:` (spec 02 §Harness images).
    /// Only a recognised tool's bytes are received; on another adapter `images: { save: }`
    /// can only end in `NIKA-AGENT-007`.
    pub delivers_images: bool,
}

impl HarnessBoundary {
    /// codex-acp 1.13.1 (it bundles codex 0.156.1). Its three modes, read from its source and
    /// its `session/new` answer: `read-only` ("Ask for approval": approval policy `on-request`,
    /// reviewed by the user — the client — over a `workspaceWrite` sandbox: the session root,
    /// `/tmp` and `$TMPDIR` writable, network off, reads unrestricted); `agent` (the adapter's
    /// DEFAULT, "Approve for me": the same sandbox, approvals reviewed by Codex's own
    /// auto-reviewer, so most never reach the client); `agent-full-access` (no sandbox, no
    /// approval at all). No mode is a read-only sandbox, so both confinements select
    /// `read-only`, the only mode whose every approval reaches Nika. In it, ordinary commands
    /// run sandboxed without asking, so a command ask is an escalation (`require_escalated`,
    /// the command as a shell STRING, the reason in `_meta.permission.description`): approved,
    /// it runs with no sandbox at all. A network approval is a command ask titled `<protocol>
    /// network access to <host>` (request `_meta.permission.title` `Allow network access?`),
    /// with `rawInput.url` only for http and https; `rawInput.additionalPermissions` asks to
    /// widen the sandbox. Measured with real Codex on 2026-10-26: without asking it also calls
    /// its MCP tools marked `readOnlyHint` (remote ones included), runs its tools through a
    /// JavaScript code mode, opens pages with its hosted web tool, writes its own `~/.codex`
    /// (images, session logs, state), starts every configured MCP server and applies the
    /// user's global `~/.codex/AGENTS.md` and Codex plugins.
    pub const CODEX_ACP: Self = Self {
        seat: "codex",
        agents: &["@agentclientprotocol/codex-acp", "codex-acp"],
        measured_on: "codex-acp 1.13.1",
        read_only_mode: "read-only",
        workspace_write_mode: "read-only",
        reads_unasked: "any file",
        writes_unasked: Some("the project, /tmp, $TMPDIR and its own ~/.codex"),
        runs_unasked: Some(
            "any program in its own sandbox, its tools through a JavaScript code mode and its \
             MCP tools marked readOnlyHint (remote ones included)",
        ),
        fetches_unasked: Some("pages with its hosted web tool, whose content reaches the model"),
        loads_unasked: Some(
            "starts every configured MCP server and applies your global ~/.codex/AGENTS.md \
             and Codex plugins",
        ),
        asked_command_leaves_sandbox: true,
        delivers_images: true,
    };

    /// claude-agent-acp 0.81.1 (Claude Code 2.1.280). Its modes: `default` ("Manual": every
    /// edit and every non-read-only command asks), `acceptEdits`, `plan`, `auto` (a classifier
    /// decides, approvals rarely reach the client) and `bypassPermissions`. A session starts in
    /// the user's `permissions.defaultMode` (`auto` on a machine that set it), so `default` is
    /// selected and read back for both confinements — `plan` still lets an allowed write run.
    /// Reads in the session root, commands it classifies read-only and pre-approved
    /// documentation hosts proceed without asking; a command ask carries a shell STRING; its
    /// own settings (permission rules, hooks) still apply.
    pub const CLAUDE_AGENT_ACP: Self = Self {
        seat: "claude-code",
        agents: &["@agentclientprotocol/claude-agent-acp"],
        measured_on: "claude-agent-acp 0.81.1",
        read_only_mode: "default",
        workspace_write_mode: "default",
        reads_unasked: "files in the project",
        writes_unasked: None,
        runs_unasked: Some("shell commands Claude Code classifies as read-only"),
        fetches_unasked: Some("documentation hosts Claude Code pre-approves"),
        loads_unasked: None,
        asked_command_leaves_sandbox: false,
        delivers_images: false,
    };

    /// gemini-cli 0.62.0. Legacy modes only (`session/set_mode`, no mode config option):
    /// `default` (edits, shell commands and fetches ask), `autoEdit`, `yolo`, `plan`. Its file
    /// tools refuse every path outside the workspace; known-safe read commands inside it and
    /// Google web searches proceed without asking; an ask carries no `rawInput` (the command
    /// and the URL ride only its title).
    pub const GEMINI_CLI: Self = Self {
        seat: "gemini-cli",
        agents: &["gemini-cli"],
        measured_on: "gemini-cli 0.62.0",
        read_only_mode: "default",
        workspace_write_mode: "default",
        reads_unasked: "files in the project",
        writes_unasked: None,
        runs_unasked: Some("shell commands it judges safe reads in the project"),
        fetches_unasked: Some("Google web searches"),
        loads_unasked: None,
        asked_command_leaves_sandbox: false,
        delivers_images: false,
    };

    /// Every measured adapter, in the registry's order (gemini first · Anthropic last).
    pub const MEASURED: [Self; 3] = [Self::GEMINI_CLI, Self::CODEX_ACP, Self::CLAUDE_AGENT_ACP];

    /// The measured boundary of the adapter that reported `agent_name` on `initialize`.
    #[must_use]
    pub fn for_agent(agent_name: &str) -> Option<Self> {
        Self::MEASURED
            .into_iter()
            .find(|row| row.agents.contains(&agent_name))
    }

    /// The measured boundary of the seat `id` (`codex`).
    #[must_use]
    pub fn for_seat(id: &str) -> Option<Self> {
        Self::MEASURED.into_iter().find(|row| row.seat == id)
    }

    /// The mode a confinement selects: no `fs.write` grant → [`Self::read_only_mode`].
    #[must_use]
    pub const fn mode_for_writes(&self, writes: bool) -> &'static str {
        if writes {
            self.workspace_write_mode
        } else {
            self.read_only_mode
        }
    }

    /// What the harness does without asking, in one clause: reads, writes (told against the
    /// workflow's own `fs.write`), runs, fetches, loads.
    fn unasked_clause(&self, writes_granted: bool) -> String {
        let mut clause = format!("reads {}", self.reads_unasked);
        if let Some(place) = self.writes_unasked {
            let scope = if writes_granted {
                "(not only the fs.write paths)"
            } else {
                "(the workflow grants no fs.write)"
            };
            let reach = if writes_granted { "anywhere in " } else { "" };
            clause.push_str("; writes ");
            clause.push_str(reach);
            clause.push_str(place);
            clause.push(' ');
            clause.push_str(scope);
        }
        if let Some(runs) = self.runs_unasked {
            clause.push_str("; runs ");
            clause.push_str(runs);
        }
        if let Some(fetches) = self.fetches_unasked {
            clause.push_str("; fetches ");
            clause.push_str(fetches);
        }
        if let Some(loads) = self.loads_unasked {
            clause.push_str("; ");
            clause.push_str(loads);
        }
        clause
    }

    /// What the harness still does without asking under the selected mode, as the run's
    /// witness states it (`permit_checked`, plane `agent`).
    #[must_use]
    pub fn unasked(&self, writes_granted: bool) -> String {
        format!(
            "Without asking, the harness {}: permits: does not bound that. {HARNESS_SHARED_RESIDUAL}",
            self.unasked_clause(writes_granted)
        )
    }

    /// The `nika check` advice for an `agent:` task this seat serves (one line, task-free so
    /// the renderer folds every task of one seat into one row with its site count), ending
    /// with [`HARNESS_IMAGES_ADVICE`] on an adapter that [`Self::delivers_images`].
    #[must_use]
    pub fn check_advice(&self, writes_granted: bool) -> String {
        let advice = format!(
            "an `agent:` task on {} runs in mode `{}`: Nika judges each approval it asks \
             (journaled permit_checked), but without asking it {} — permits: does not bound \
             that (`nika explain harness`)",
            self.seat,
            self.mode_for_writes(writes_granted),
            self.unasked_clause(writes_granted)
        );
        if self.delivers_images {
            format!("{advice}. {HARNESS_IMAGES_ADVICE}")
        } else {
            advice
        }
    }
}

/// How an image a seat generates reaches the workflow (spec 02 §Harness images): the bytes
/// this session sent, never a copy out of a folder the harness shares with other sessions.
/// Stated only for an adapter whose image tool the client recognises
/// ([`HarnessBoundary::delivers_images`]).
pub const HARNESS_IMAGES_ADVICE: &str = "An image it generates reaches the workflow through \
     `images: { save: <path> }`: Nika writes the bytes this session sent, never a file the \
     harness reports or a copy from its shared images folder.";

/// What holds on EVERY harness seat, measured or not (NIK-266): the residual beside each row.
pub const HARNESS_SHARED_RESIDUAL: &str = "A command asked as a shell string is admitted only \
     by `permits.exec: true`, otherwise it pauses for the operator; a command Codex asks about \
     in `read-only` asks to leave its sandbox and always pauses. An ask for network access (a \
     fetch, or a command the harness says needs the network), for a wider harness sandbox or \
     to approve one of the harness's own MCP tool calls always pauses. An action Nika allows \
     once runs under the harness's own sandbox, not Nika's exec sandbox, so permits.fs and \
     permits.net do not confine it: an allowed command opens whatever connections that sandbox \
     lets it. The harness's own settings (its permission rules, hooks and MCP servers) still \
     apply: what they allow never asks.";

/// The marker an anchored ask location starts with when the client could not resolve the
/// file the harness's path reaches (a symlink loop, an unreadable component, a component that
/// is not a directory). The bridge's judge reads it as unverifiable, never as a path a grant
/// could admit, so the ask pauses for the operator (NIK-266). A real path that happens to
/// start with it only pauses too: the collision is fail-closed.
pub const HARNESS_UNRESOLVED_PATH: &str = "<unresolved>";

/// What an unmeasured harness does under its own mode, in the engine's words: Nika selects
/// nothing and judges only the asks that reach it.
pub const UNMEASURED_HARNESS_UNASKED: &str = "No approval mode is measured for this harness: \
     Nika judges only the permissions it asks for, and whatever it reads, writes, runs or \
     fetches without asking stays outside the declared boundary.";

/// The `nika check` advice for an `agent:` task on a seat with no measured boundary.
#[must_use]
pub fn unmeasured_check_advice(seat: &str) -> String {
    format!(
        "an `agent:` task on {seat} runs in a mode Nika has not measured: Nika judges only the \
         permissions it asks for (journaled permit_checked); whatever it reads, writes, runs or \
         fetches unasked is outside permits: (`nika explain harness`)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_measured_adapter_is_found_by_its_own_report_only() {
        assert_eq!(
            HarnessBoundary::for_agent("@agentclientprotocol/codex-acp"),
            Some(HarnessBoundary::CODEX_ACP)
        );
        assert_eq!(
            HarnessBoundary::for_agent("codex-acp"),
            Some(HarnessBoundary::CODEX_ACP),
            "the deprecated package answers its bare name"
        );
        assert_eq!(
            HarnessBoundary::for_agent("@agentclientprotocol/claude-agent-acp"),
            Some(HarnessBoundary::CLAUDE_AGENT_ACP)
        );
        assert_eq!(
            HarnessBoundary::for_agent("gemini-cli"),
            Some(HarnessBoundary::GEMINI_CLI)
        );
        for unmeasured in [
            "Codex",
            "Claude Agent",
            "@zed-industries/claude-agent-acp",
            "asking-harness",
        ] {
            assert_eq!(HarnessBoundary::for_agent(unmeasured), None, "{unmeasured}");
        }
        assert_eq!(
            HarnessBoundary::for_seat("codex"),
            Some(HarnessBoundary::CODEX_ACP)
        );
        assert_eq!(HarnessBoundary::for_seat("kimi-code"), None);
        for row in HarnessBoundary::MEASURED {
            assert!(
                super::super::HarnessRuntime::lookup(row.seat).is_some(),
                "{} is a shipped seat",
                row.seat
            );
        }
    }

    /// Every measured row selects the mode whose approvals reach the client, for both
    /// confinements: never an auto-reviewer, accept-edits, full access, bypass or yolo.
    #[test]
    fn every_measured_row_selects_an_asking_mode() {
        let selected: alloc::vec::Vec<(&str, &str, &str)> = HarnessBoundary::MEASURED
            .iter()
            .map(|row| (row.seat, row.read_only_mode, row.workspace_write_mode))
            .collect();
        assert_eq!(
            selected,
            [
                ("gemini-cli", "default", "default"),
                ("codex", "read-only", "read-only"),
                ("claude-code", "default", "default"),
            ]
        );
        for (_, read_only, write) in selected {
            for mode in [read_only, write] {
                let lower = mode.to_ascii_lowercase();
                for word in ["full", "bypass", "yolo", "auto", "accept", "agent", "plan"] {
                    assert!(!lower.contains(word), "{mode} is not an asking mode");
                }
            }
        }
    }

    /// The residual names what stays unbounded, and the no-write case says the project is
    /// still writable — the boundary a reader would otherwise believe.
    #[test]
    fn the_residual_states_what_permits_do_not_bound() {
        let codex = HarnessBoundary::CODEX_ACP;
        assert_eq!(
            codex.check_advice(false),
            "an `agent:` task on codex runs in mode `read-only`: Nika judges each approval it \
             asks (journaled permit_checked), but without asking it reads any file; writes the \
             project, /tmp, $TMPDIR and its own ~/.codex (the workflow grants no fs.write); runs \
             any program in its own sandbox, its tools through a JavaScript code mode and its \
             MCP tools marked readOnlyHint (remote ones included); fetches pages with its hosted \
             web tool, whose content reaches the model; starts every configured MCP server and \
             applies your global ~/.codex/AGENTS.md and Codex plugins — permits: does not bound \
             that (`nika explain harness`). An image it generates reaches the workflow through \
             `images: { save: <path> }`: Nika writes the bytes this session sent, never a file \
             the harness reports or a copy from its shared images folder."
        );
        assert!(codex.check_advice(true).contains(
            "writes anywhere in the project, /tmp, $TMPDIR and its own ~/.codex (not only the \
             fs.write paths)"
        ));
        let witness = codex.unasked(false);
        assert!(
            witness.starts_with("Without asking, the harness reads any file; "),
            "{witness}"
        );
        assert!(witness.ends_with(HARNESS_SHARED_RESIDUAL), "{witness}");
        let claude = HarnessBoundary::CLAUDE_AGENT_ACP.check_advice(true);
        assert!(claude.contains("in mode `default`"), "{claude}");
        assert!(
            !claude.contains("writes"),
            "every Claude Code edit asks: {claude}"
        );
        assert!(
            claude.contains("fetches documentation hosts Claude Code pre-approves"),
            "{claude}"
        );
        // NIK-274 · only the mode that runs ordinary commands sandboxed unasked turns every
        // command ask into an escalation.
        let escalating: alloc::vec::Vec<&str> = HarnessBoundary::MEASURED
            .iter()
            .filter(|row| row.asked_command_leaves_sandbox)
            .map(|row| row.seat)
            .collect();
        assert_eq!(escalating, ["codex"]);
        let unmeasured = unmeasured_check_advice("kimi-code");
        assert!(
            unmeasured.contains("on kimi-code runs in a mode Nika has not measured"),
            "{unmeasured}"
        );
        // B-D18 · the one way an image reaches the workflow, taught only where the client
        // recognises the adapter's image tool (Codex): elsewhere `images:` receives nothing.
        assert!(codex.check_advice(true).ends_with(HARNESS_IMAGES_ADVICE));
        let gemini = HarnessBoundary::GEMINI_CLI.check_advice(true);
        for advice in [claude, gemini, unmeasured] {
            assert!(!advice.contains("images:"), "{advice}");
        }
    }
}
