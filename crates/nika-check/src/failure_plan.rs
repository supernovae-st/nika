// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `failure_plan[]` — the four P0 shapes a run would refuse, named on
//! the check JSON so an agent does not have to rediscover them at the
//! wire (I05). Additive: `report_version` stays 1.

use nika_schema::raw::{RawAction, RawWorkflow};
use serde::Serialize;

use crate::CheckReport;

/// One predicted (or already-found) run refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct FailurePlanEntry {
    /// Closed slug: `host_passwd_read` · `exec_cat_host` ·
    /// `priced_image_over_cap` · `unpriced_cloud_cap`.
    pub shape: &'static str,
    /// The wire code the run would stamp.
    pub code: String,
    /// The task that carries the shape.
    pub task: String,
    /// Human row — the same voice as the finding / admission gate.
    pub message: String,
}

impl FailurePlanEntry {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        shape: &'static str,
        code: impl Into<String>,
        task: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            shape,
            code: code.into(),
            task: task.into(),
            message: message.into(),
        }
    }
}

/// Project the four P0 shapes off the judged workflow + report.
#[must_use]
pub(crate) fn collect(wf: &RawWorkflow, report: &CheckReport) -> Vec<FailurePlanEntry> {
    let mut out = Vec::new();
    out.extend(passwd_reads(wf, report));
    out.extend(exec_cat_host(wf, report));
    out.extend(priced_image(wf));
    out.extend(unpriced_cloud(report));
    out
}

fn passwd_reads(wf: &RawWorkflow, report: &CheckReport) -> Vec<FailurePlanEntry> {
    report
        .capability_escapes
        .iter()
        .filter(|e| e.detail.contains("/etc/passwd") || e.detail.contains("escapes the workspace"))
        .filter(|e| {
            wf.tasks.iter().any(|t| {
                t.value.id.value == e.task
                    && matches!(&t.value.action, RawAction::Invoke(inv) if inv.tool().is_some_and(|tool| tool.value.contains("nika:read") || tool.value.contains("nika:write")))
            })
        })
        .map(|e| {
            FailurePlanEntry::new(
                "host_passwd_read",
                "NIKA-SEC-004",
                e.task.clone(),
                e.detail.clone(),
            )
        })
        .collect()
}

fn exec_cat_host(wf: &RawWorkflow, report: &CheckReport) -> Vec<FailurePlanEntry> {
    report
        .capability_escapes
        .iter()
        .filter(|e| {
            wf.tasks.iter().any(|t| {
                t.value.id.value == e.task && matches!(&t.value.action, RawAction::Exec(_))
            })
        })
        .map(|e| {
            FailurePlanEntry::new(
                "exec_cat_host",
                "NIKA-SEC-004",
                e.task.clone(),
                e.detail.clone(),
            )
        })
        .collect()
}

fn priced_image(wf: &RawWorkflow) -> Vec<FailurePlanEntry> {
    wf.tasks
        .iter()
        .filter_map(|t| {
            let RawAction::Invoke(inv) = &t.value.action else {
                return None;
            };
            let tool = inv.tool()?;
            if tool.value != "nika:image_generate" {
                return None;
            }
            let args = inv.args.as_ref()?;
            let provider = args.value.get("provider").and_then(|v| v.as_str())?;
            if provider.contains("${{") || provider == "mock" {
                return None;
            }
            let floor = nika_catalog::builtin_provider_floor_usd("image_generate", provider)?;
            Some(FailurePlanEntry::new(
                "priced_image_over_cap",
                "NIKA-1709",
                t.value.id.value.clone(),
                format!(
                    "nika:image_generate on `{provider}` has catalog floor ${floor:.6} — \
                     `--max-cost-usd` below that floor refuses NIKA-1709 before HTTP"
                ),
            ))
        })
        .collect()
}

fn unpriced_cloud(report: &CheckReport) -> Vec<FailurePlanEntry> {
    report
        .data_journey
        .model_endpoints
        .iter()
        .filter(|ep| ep.locus == crate::EndpointLocus::Cloud && !ep.priced)
        .map(|ep| {
            FailurePlanEntry::new(
                "unpriced_cloud_cap",
                "NIKA-1709",
                ep.task.clone(),
                format!(
                    "cloud model `{}` is unpriced — `--max-cost-usd` cannot bound unknown spend \
                     (NIKA-1709 before infer HTTP)",
                    ep.model
                ),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn plan(yaml: &str) -> Vec<FailurePlanEntry> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let report = crate::check(&wf);
        collect(&wf, &report)
    }

    #[test]
    fn host_passwd_read_is_on_the_plan() {
        let p = plan(
            "nika: passwd-read\npermits:\n  tools: [\"nika:read\"]\n  fs: { read: [\"./**\"] }\ntasks:\n  p:\n    invoke: { tool: nika:read, args: { path: /etc/passwd } }\n",
        );
        assert!(
            p.iter()
                .any(|e| e.shape == "host_passwd_read" && e.code == "NIKA-SEC-004"),
            "{p:?}"
        );
    }

    #[test]
    fn exec_cat_host_is_on_the_plan() {
        let p = plan(
            "nika: dump\npermits:\n  exec: [\"cat\"]\ntasks:\n  p:\n    exec: { command: [\"cat\", \"/etc/passwd\"] }\n",
        );
        assert!(
            p.iter()
                .any(|e| e.shape == "exec_cat_host" && e.code == "NIKA-SEC-004"),
            "{p:?}"
        );
    }

    #[test]
    fn exec_true_shell_cat_host_is_on_the_plan() {
        let p = plan(
            "nika: dump\npermits:\n  exec: true\ntasks:\n  p:\n    exec: { shell: \"cat /etc/passwd\" }\n",
        );
        assert!(
            p.iter()
                .any(|e| e.shape == "exec_cat_host" && e.code == "NIKA-SEC-004"),
            "{p:?}"
        );
    }

    #[test]
    fn exec_true_templated_cat_is_on_the_plan() {
        let p = plan(
            "nika: dump-tmpl\ninputs:\n  pth: { type: string, default: \"/etc/passwd\" }\npermits:\n  exec: true\ntasks:\n  p:\n    exec: { shell: \"cat ${{ inputs.pth }}\" }\n",
        );
        assert!(
            p.iter()
                .any(|e| e.shape == "exec_cat_host" && e.code == "NIKA-SEC-004"),
            "{p:?}"
        );
    }

    #[test]
    fn a_host_grant_leaves_templated_cat_to_the_run() {
        let p = plan(
            "nika: dump-granted\ninputs:\n  pth: { type: string, default: \"/etc/passwd\" }\npermits:\n  exec: true\n  fs:\n    read: [\"/etc/passwd\"]\ntasks:\n  p:\n    exec: { shell: \"cat ${{ inputs.pth }}\" }\n",
        );
        assert!(
            p.iter().all(|e| e.shape != "exec_cat_host"),
            "an explicit host grant is the operator's act: {p:?}"
        );
    }

    #[test]
    fn priced_image_is_on_the_plan() {
        let p = plan(
            "nika: b24\npermits: { tools: [\"nika:image_generate\"], fs: { write: [\"./out/**\"] } }\ntasks:\n  og:\n    invoke: { tool: \"nika:image_generate\", args: { provider: xai, prompt: \"a monarch butterfly\", output_dir: \"./out\" } }\n",
        );
        assert!(
            p.iter()
                .any(|e| e.shape == "priced_image_over_cap" && e.code == "NIKA-1709"),
            "{p:?}"
        );
    }

    #[test]
    fn unpriced_cloud_canary_is_on_the_plan() {
        let p = plan(
            "nika: b20\nmodel: gemini/nika-b20-unpriced-canary\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: PONG, max_tokens: 16 }\n",
        );
        assert!(
            p.iter()
                .any(|e| e.shape == "unpriced_cloud_cap" && e.code == "NIKA-1709"),
            "{p:?}"
        );
    }

    #[test]
    fn mock_rehearsal_is_not_on_the_plan() {
        let p = plan(
            "nika: ok\nmodel: mock/echo\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: hi, max_tokens: 16 }\n",
        );
        assert!(p.is_empty(), "mock is a proven zero: {p:?}");
    }
}
