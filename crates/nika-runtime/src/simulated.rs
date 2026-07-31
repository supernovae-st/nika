// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effects-simulated seams — the `nika test` plane (UX audit
//! 2026-07-30 · P0-16).
//!
//! `capture_mock_outputs` substitutes the MODEL (`mock/echo` — offline ·
//! zero key · deterministic), but a model swap alone leaves the tool/exec
//! seams REAL: before these seams, `permits: { exec: true }` + an exec
//! task spawned a live child, and `invoke nika:write`/`nika:fetch` wrote
//! bytes and opened sockets under a verb documented « offline ». The
//! simulated plane closes the gap at the composition root:
//!
//! - [`SimulatedShell`] — the exec verb's shell refuses EVERY command
//!   (fail-closed · `ShellError::Blocked`).
//! - [`SimulatedDispatcher`] — the tool plane delegates pure compute,
//!   stderr observability, and fs READS inside the declared boundary to
//!   the real [`BuiltinDispatcher`](nika_builtin::BuiltinDispatcher), and
//!   REFUSES the effect set (writes · network · media · anything outside
//!   the `nika:` namespace — fail-closed for the day an MCP plane joins
//!   the dispatcher).
//!
//! Both speak ONE refusal sentence ([`EFFECTS_DISABLED`]) so the task
//! failure the operator reads names the plane and the reason — an honest
//! failure, never a silent skip: a workflow that NEEDS a real effect
//! cannot pin a green golden under `nika test`.

use std::sync::Arc;

use nika_kernel::ai::provider::ToolDef;
use nika_kernel::ai::tool_defs::{ToolDefinitionProviderDyn, ToolDefsError};
use nika_kernel::process::{ShellCommand, ShellError, ShellResult, ShellRunDyn};
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};

/// The ONE refusal sentence every simulated seam speaks (the audit's
/// « effects disabled under test » — verbatim enough to grep, precise
/// enough to teach).
const EFFECTS_DISABLED: &str =
    "effects disabled under `nika test` — the mock plane simulates the model, not effects";

/// The builtin tools that stay REAL effects even when the model is mock:
/// fs WRITES (`write` · `edit`), NETWORK (`fetch` · `notify`), and the
/// MEDIA four (provider calls + disk-landing outputs — `chart` and the
/// « deterministic » `image_fx` included: a pure transform that lands a
/// file is still a write). Every other `nika:` builtin delegates (pure
/// compute · fs reads inside the declared `permits.fs` boundary · stderr
/// observability).
const EFFECT_TOOLS: &[&str] = &[
    "nika:write",
    "nika:edit",
    "nika:fetch",
    "nika:notify",
    "nika:image_generate",
    "nika:tts_generate",
    "nika:chart",
    "nika:image_fx",
];

/// The refusal predicate: the enumerated effect set, PLUS anything outside
/// the `nika:` namespace (`mcp:*` — the production dispatcher resolves no
/// MCP plane today, but the simulated plane must stay effect-free the day
/// one lands · fail-closed over a growing tool surface).
fn refuses(name: &str) -> bool {
    EFFECT_TOOLS.contains(&name) || !name.starts_with("nika:")
}

/// The exec verb's shell under the simulated plane: EVERY command is
/// refused before a child could spawn (the `Blocked` class — a refusal,
/// not a failure of the command itself).
///
/// `pub` only because it appears in the public
/// [`SimRuntime`](crate::compose::SimRuntime) type spelling — not a
/// re-export surface.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimulatedShell;

impl ShellRunDyn for SimulatedShell {
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        Err(ShellError::Blocked {
            reason: format!("{EFFECTS_DISABLED} · exec `{}` refused", command.program),
        })
    }
}

/// The tool plane under the simulated plane: a thin gate over the real
/// dispatcher. Effect tools refuse (see [`EFFECT_TOOLS`]); everything else
/// delegates VERBATIM — the declared `permits.fs` boundary, the check⇄run
/// parity, and the tool-defs enumeration all keep their production
/// behavior, so a golden exercises the same pure surface `nika run` would.
///
/// `pub` only because it appears in the public
/// [`SimRuntime`](crate::compose::SimRuntime) type spelling — not a
/// re-export surface.
#[derive(Debug)]
pub struct SimulatedDispatcher<D> {
    inner: Arc<D>,
}

impl<D> SimulatedDispatcher<D> {
    /// Wrap the real dispatcher (the gate reads the call name first).
    #[must_use]
    pub fn new(inner: Arc<D>) -> Self {
        Self { inner }
    }
}

impl<D> ToolExecuteDyn for SimulatedDispatcher<D>
where
    D: ToolExecuteDyn,
{
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        if refuses(&call.name) {
            return Err(ToolExecError::NotAvailable {
                reason: format!("{EFFECTS_DISABLED} · tool `{}` refused", call.name),
            });
        }
        self.inner.execute(call).await
    }
}

impl<D> ToolDefinitionProviderDyn for SimulatedDispatcher<D>
where
    D: ToolDefinitionProviderDyn,
{
    async fn tool_defs(&self) -> Result<Vec<ToolDef>, ToolDefsError> {
        // Read-only enumeration (the trait's own contract) — not an
        // effect: the agent's tool WHITELIST stays the production one,
        // the gate refuses at call time.
        self.inner.tool_defs().await
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use nika_kernel_mock::{MockToolDefinitionProvider, MockToolExecutor};

    /// A stub shell command (the refusal never reads past `program`).
    fn cmd(program: &str) -> ShellCommand {
        ShellCommand::new(program)
    }

    #[tokio::test]
    async fn the_shell_refuses_every_command_before_spawn() {
        let shell = SimulatedShell;
        let err = shell
            .run(cmd("touch"))
            .await
            .expect_err("the simulated shell never runs");
        let text = err.to_string();
        assert!(
            text.contains("effects disabled under `nika test`"),
            "{text}"
        );
        assert!(text.contains("exec `touch` refused"), "{text}");
    }

    #[tokio::test]
    async fn the_gate_refuses_the_effect_set_and_foreign_namespaces() {
        let inner = MockToolExecutor::new();
        let d = SimulatedDispatcher::new(Arc::new(inner.clone()));
        for name in [
            "nika:write",
            "nika:edit",
            "nika:fetch",
            "nika:notify",
            "nika:image_generate",
            "nika:tts_generate",
            "nika:chart",
            "nika:image_fx",
            "mcp:server/tool",
        ] {
            let err = d
                .execute(ToolCall::new("c", name, serde_json::json!({})))
                .await
                .expect_err("an effect tool is refused");
            let text = err.to_string();
            assert!(
                text.contains("effects disabled under `nika test`"),
                "{name}: {text}"
            );
        }
        assert!(
            inner.captured_calls().is_empty(),
            "no refused call ever reached the inner plane"
        );
    }

    #[tokio::test]
    async fn the_gate_delegates_pure_tools_verbatim() {
        let mut inner = MockToolExecutor::new();
        for _ in 0..4 {
            inner = inner.enqueue_ok(ToolResult::success("c", "ok"));
        }
        let inner = Arc::new(inner);
        let d = SimulatedDispatcher::new(Arc::clone(&inner));
        for name in ["nika:jq", "nika:read", "nika:log", "nika:date"] {
            d.execute(ToolCall::new("c", name, serde_json::json!({})))
                .await
                .expect("the mock executor answers");
        }
        let seen = inner.captured_calls();
        assert_eq!(seen.len(), 4, "every pure call reached the inner plane");
        assert_eq!(seen[0].name, "nika:jq");
        assert_eq!(seen[3].name, "nika:date");
    }

    #[tokio::test]
    async fn tool_defs_enumerate_through_the_gate() {
        let d = SimulatedDispatcher::new(Arc::new(MockToolDefinitionProvider::new()));
        d.tool_defs()
            .await
            .expect("enumeration is read-only — it delegates");
    }
}
