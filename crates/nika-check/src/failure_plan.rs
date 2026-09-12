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
    /// `priced_image_over_cap` · `unpriced_cloud_cap`. The historical
    /// `host_passwd_read` slug covers confirmed read/write fs refusals.
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
        .filter(|e| e.category == "fs" && !e.floor)
        .filter(|e| {
            wf.tasks.iter().any(|t| {
                t.value.id.value == e.task
                    && matches!(&t.value.action, RawAction::Invoke(inv) if inv.tool().is_some_and(|tool| matches!(tool.value.as_str(), "nika:read" | "nika:write")))
            })
        })
        .map(|e| {
            FailurePlanEntry::new(
                "host_passwd_read",
                if e.undeclared { "NIKA-AUTH-006" } else { "NIKA-SEC-004" },
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

    fn fs_fixture(tool: &str, path: &str, declared: bool) -> RawWorkflow {
        let permits = if declared {
            format!(
                "permits: {{ tools: ['{tool}'], fs: {{ read: ['./safe/**'], write: ['./safe/**'] }} }}\n"
            )
        } else {
            String::new()
        };
        let content = if tool == "nika:write" {
            ", content: hi"
        } else {
            ""
        };
        let source = format!(
            "nika: fs-plan\nconst:\n  target: /outside/nika/secret.txt\n{permits}tasks:\n  probe:\n    invoke: {{ tool: '{tool}', args: {{ path: '{path}'{content} }} }}\n"
        );
        parse(&source, FileId::new(0), ParseMode::Strict).expect("fixture")
    }

    #[test]
    fn host_shapes_survive_new_wording_for_literals_and_consts() {
        for tool in ["nika:read", "nika:write"] {
            for path in [
                "/outside/nika/secret.txt",
                "../secret.txt",
                "${{ const.target }}",
                "./other/file.txt",
            ] {
                let wf = fs_fixture(tool, path, true);
                let report = crate::check(&wf);
                let plan = passwd_reads(&wf, &report);
                assert_eq!(plan.len(), 1, "{tool}, {path}: {plan:?}");
                assert_eq!(plan[0].shape, "host_passwd_read");
                assert_eq!(plan[0].code, "NIKA-SEC-004");
            }
        }
    }

    #[test]
    fn changing_only_the_diagnostic_text_keeps_the_refusal_classification() {
        let wf = fs_fixture("nika:read", "/etc/passwd", true);
        let mut report = crate::check(&wf);
        let before = passwd_reads(&wf, &report);
        assert_eq!(before.len(), 1);
        for escape in &mut report.capability_escapes {
            escape.detail = "an independently worded diagnostic".to_owned();
        }
        let after = passwd_reads(&wf, &report);
        assert_eq!(after.len(), before.len());
        assert_eq!(after[0].shape, before[0].shape);
        assert_eq!(after[0].code, before[0].code);
        assert_eq!(after[0].task, before[0].task);
        assert_eq!(after[0].message, "an independently worded diagnostic");
    }

    #[test]
    fn an_absent_boundary_keeps_its_host_shape_and_its_authority_code() {
        for tool in ["nika:read", "nika:write"] {
            for path in [
                "/outside/nika/secret.txt",
                "../secret.txt",
                "${{ const.target }}",
                "./safe/file.txt",
            ] {
                let wf = fs_fixture(tool, path, false);
                let report = crate::check(&wf);
                let plan = passwd_reads(&wf, &report);
                assert_eq!(plan.len(), 1, "{tool}, {path}: {plan:?}");
                assert_eq!(plan[0].code, "NIKA-AUTH-006");
            }
        }
    }

    #[test]
    fn an_unrelated_capability_cannot_spoof_a_host_shape_through_its_message() {
        let wf = fs_fixture("nika:read", "/etc/passwd", true);
        let mut report = crate::check(&wf);
        for escape in &mut report.capability_escapes {
            escape.category = "net";
        }
        assert!(passwd_reads(&wf, &report).is_empty());
        for escape in &mut report.capability_escapes {
            escape.category = "fs";
            escape.floor = true;
        }
        assert!(passwd_reads(&wf, &report).is_empty());
    }

    #[test]
    fn clean_reads_and_tool_only_refusals_do_not_gain_a_filesystem_plan() {
        let wf = fs_fixture("nika:read", "./safe/file.txt", true);
        let report = crate::check(&wf);
        assert!(report.is_clean());
        assert!(passwd_reads(&wf, &report).is_empty());
        let source = "nika: denied-tool\npermits: { tools: [], fs: { read: ['./safe/**'] } }\ntasks:\n  probe:\n    invoke: { tool: nika:read, args: { path: './safe/file.txt' } }\n";
        let wf = parse(source, FileId::new(0), ParseMode::Strict).expect("fixture");
        let report = crate::check(&wf);
        assert!(!report.is_clean());
        assert!(
            report
                .capability_escapes
                .iter()
                .all(|e| e.category == "tools")
        );
        assert!(passwd_reads(&wf, &report).is_empty());
    }

    #[test]
    fn a_tool_name_containing_a_builtin_name_is_not_that_builtin() {
        let wf = fs_fixture("nika:read", "/etc/passwd", true);
        let report = crate::check(&wf);
        let other = fs_fixture("nika:read_extra", "/etc/passwd", true);
        assert!(passwd_reads(&other, &report).is_empty());
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
