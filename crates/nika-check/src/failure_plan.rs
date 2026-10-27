// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `failure_plan[]` — the P0 shapes a run would refuse, named on
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
    /// `priced_image_over_cap` · `priced_speech_over_cap` ·
    /// `unpriced_cloud_cap` · `unpriced_media_cap`. The historical
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

/// Project the P0 shapes off the judged workflow + report.
#[must_use]
pub(crate) fn collect(wf: &RawWorkflow, report: &CheckReport) -> Vec<FailurePlanEntry> {
    let mut out = Vec::new();
    out.extend(passwd_reads(wf, report));
    out.extend(exec_cat_host(wf, report));
    out.extend(priced_image(report));
    out.extend(unpriced_cloud(report));
    out.extend(unpriced_media(report));
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

/// A paid-media call the catalog floors (`cost.media`, the one
/// classification admission floors with: an explicit `provider:` or the one
/// its `model:` names, and its model): a cap below its floor refuses before
/// the request. An image floors per image; speech per character.
fn priced_image(report: &CheckReport) -> Vec<FailurePlanEntry> {
    report
        .cost
        .media
        .iter()
        .filter_map(|call| {
            let floor = call.usd_per_call?;
            let shape = if call.tool.ends_with("tts_generate") {
                "priced_speech_over_cap"
            } else {
                "priced_image_over_cap"
            };
            let callee = match &call.model {
                Some(model) => format!("`{}` (`{model}`)", call.provider),
                None => format!("`{}`", call.provider),
            };
            Some(FailurePlanEntry::new(
                shape,
                "NIKA-1709",
                call.task.clone(),
                format!(
                    "{} on {callee} has catalog floor ${floor:.6} — \
                     `--max-cost-usd` below that floor refuses NIKA-1709 before HTTP",
                    call.tool
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

/// A paid-media call with no catalog price (`cost.media`): any
/// `--max-cost-usd` refuses the run before its request, as it refuses an
/// unpriced cloud model.
fn unpriced_media(report: &CheckReport) -> Vec<FailurePlanEntry> {
    report
        .cost
        .media
        .iter()
        .filter(|call| call.unpriced())
        .map(|call| {
            let callee = match &call.model {
                Some(model) => format!("`{}` (`{model}`)", call.provider),
                None => format!("`{}`", call.provider),
            };
            FailurePlanEntry::new(
                "unpriced_media_cap",
                "NIKA-1709",
                call.task.clone(),
                format!(
                    "{} on {callee} has no catalog price — `--max-cost-usd` cannot bound \
                     unknown spend (NIKA-1709 before the media request)",
                    call.tool
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
                assert_eq!(plan.len(), 1, "{tool}, {path}");
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
                assert_eq!(plan.len(), 1, "{tool}, {path}");
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

    /// The priced floor reads the one media classification admission floors
    /// with: an xAI call named by its `model:` is on the plan beside the one
    /// that names `provider: xai` (both count at admission). A provider only
    /// the run decides is on neither list: the run judges it.
    #[test]
    fn priced_image_follows_the_admission_classification() {
        let p = plan(
            "nika: grok\ninputs:\n  who: { type: string, default: xai }\npermits: { tools: [\"nika:image_generate\"], fs: { write: [\"./out/**\"] } }\ntasks:\n  a:\n    invoke: { tool: \"nika:image_generate\", args: { model: grok-imagine-image, prompt: x, output_dir: \"./out\" } }\n  b:\n    invoke: { tool: \"nika:image_generate\", args: { provider: xai, prompt: x, output_dir: \"./out\" } }\n  c:\n    invoke: { tool: \"nika:image_generate\", args: { provider: \"${{ inputs.who }}\", prompt: x, output_dir: \"./out\" } }\n",
        );
        let shapes: Vec<(&str, &str)> = (p.iter()).map(|e| (e.shape, e.task.as_str())).collect();
        assert_eq!(
            shapes,
            [
                ("priced_image_over_cap", "a"),
                ("priced_image_over_cap", "b")
            ],
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

    /// C-D3 · a paid media call the catalog cannot price is on the plan as the
    /// same NIKA-1709 refusal an unpriced cloud model carries; a priced (xAI),
    /// mock, local or keyless call is not an unpriced one.
    #[test]
    fn unpriced_media_is_on_the_plan_and_unmetered_media_is_not() {
        fn unpriced(p: &[FailurePlanEntry]) -> Vec<(&str, &str)> {
            p.iter()
                .filter(|e| e.shape == "unpriced_media_cap")
                .map(|e| (e.code.as_str(), e.task.as_str()))
                .collect()
        }
        let media = |tool: &str, args: &str| {
            plan(&format!(
                "nika: c-d3\npermits: {{ tools: [\"{tool}\"], fs: {{ write: [\"./out/**\"] }} }}\ntasks:\n  speak:\n    invoke: {{ tool: \"{tool}\", args: {{ {args}, output_dir: \"./out\" }} }}\n"
            ))
        };
        // Still unpriced: OpenAI's default voice, an `ElevenLabs` id outside
        // the price page's families, the image models shut down 2026-12-01,
        // a Gemini id the table does not list.
        for (tool, args) in [
            ("nika:tts_generate", "provider: openai, text: hi"),
            (
                "nika:tts_generate",
                "model: eleven_multilingual_v1, text: hi",
            ),
            ("nika:tts_generate", "model: gpt-4o-mini-tts, text: hi"),
            (
                "nika:image_generate",
                "model: gpt-image-1.5, prompt: a star",
            ),
            (
                "nika:image_generate",
                "model: gemini-2.0-flash-preview-image-generation, prompt: a star",
            ),
        ] {
            let p = media(tool, args);
            assert_eq!(
                unpriced(&p),
                [("NIKA-1709", "speak")],
                "{tool} {{ {args} }}: {p:?}"
            );
            assert!(
                p.iter()
                    .any(|e| e.shape == "unpriced_media_cap"
                        && e.message.contains("no catalog price")),
                "{p:?}"
            );
        }
        for (tool, args) in [
            ("nika:image_generate", "provider: xai, prompt: a star"),
            ("nika:image_generate", "provider: openai, prompt: a star"),
            (
                "nika:image_generate",
                "model: gemini-3.1-flash-image, prompt: a star",
            ),
            (
                "nika:image_generate",
                "model: gpt-image-2.5-flare, prompt: a star",
            ),
            ("nika:image_generate", "provider: mock, prompt: a star"),
            ("nika:image_generate", "provider: local, prompt: a star"),
            ("nika:tts_generate", "provider: elevenlabs, text: hi"),
            ("nika:tts_generate", "model: tts-1, text: hi"),
            ("nika:tts_generate", "provider: mock, text: hi"),
            ("nika:tts_generate", "provider: local, text: hi"),
        ] {
            let p = media(tool, args);
            assert!(unpriced(&p).is_empty(), "{tool} {{ {args} }}: {p:?}");
        }
    }

    /// C-D3 · a priced speech call floors per character: a cap below it
    /// refuses before the request, on its own slug.
    #[test]
    fn a_priced_speech_floor_is_on_the_plan() {
        let p = plan(
            "nika: say\npermits: { tools: [\"nika:tts_generate\"], fs: { write: [\"./out/**\"] } }\ntasks:\n  say:\n    invoke: { tool: \"nika:tts_generate\", args: { model: tts-1-hd, text: hello, output_dir: \"./out\" } }\n",
        );
        let entry = (p.iter())
            .find(|e| e.shape == "priced_speech_over_cap")
            .expect("the speech floor is on the plan");
        assert_eq!(
            (entry.code.as_str(), entry.task.as_str()),
            ("NIKA-1709", "say")
        );
        assert!(
            entry.message.contains("`openai` (`tts-1-hd`)") && entry.message.contains("$0.000150"),
            "{}",
            entry.message
        );
    }
}
