// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `egress` rule set — spec `02-verbs.md` §Project files (« normative for linters ·
//! advisory »): a `prompt:` or `system:` text holding a PEM private-key block is a credential
//! bound for the model provider.
//!
//! The rule judges inline text and the text of a prompt file alike: given the workflow its
//! project files composed into (`nika_schema::compose_project_files`), a file's text is the
//! field's text. An unread file holds no text to judge. It refuses nothing and never repeats the
//! text it found.

use nika_schema::raw::{RawAction, RawWorkflow};

use super::preference_rules::Lint;

/// Run the `egress/001 private-key-in-prompt` rule over a workflow. Output is deterministic: task
/// order, one lint per `infer:` or `agent:` task whose prompt or system text holds a line
/// `-----BEGIN … PRIVATE KEY-----`.
#[must_use]
pub fn egress(wf: &RawWorkflow) -> Vec<Lint> {
    let mut lints = Vec::new();
    for task in &wf.tasks {
        let t = &task.value;
        let (prompt, system) = match &t.action {
            RawAction::Infer(a) => (&a.prompt, a.system.as_ref()),
            RawAction::Agent(a) => (&a.prompt, a.system.as_ref()),
            _ => continue,
        };
        if std::iter::once(prompt)
            .chain(system)
            .any(|text| holds_private_key(&text.value))
        {
            lints.push(Lint::new(
                "egress/001",
                t.id.value.clone(),
                t.id.span,
                "a `prompt:` or `system:` text holds a PEM private-key block: a credential bound for the model provider (spec 02 §Project files)".to_owned(),
                "a secret the provider needs belongs in `secrets:`, whose egress laws apply; keep keys out of prompt text and prompt files".to_owned(),
            ));
        }
    }
    lints
}

/// Whether `text` holds a line `-----BEGIN … PRIVATE KEY-----` (a certificate or a public key
/// is not one).
fn holds_private_key(text: &str) -> bool {
    text.lines().any(|line| {
        (line.trim().strip_prefix("-----BEGIN "))
            .and_then(|rest| rest.strip_suffix("-----"))
            .is_some_and(|label| label.ends_with("PRIVATE KEY"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;
    use nika_schema::{ProjectFileRead, compose_project_files};

    fn lints_of(yaml: &str) -> Vec<(&'static str, String)> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        egress(&wf)
            .into_iter()
            .map(|lint| (lint.rule, lint.task_id))
            .collect()
    }

    /// A block scalar holding a PEM block under `label`, its body a placeholder.
    fn block(label: &str) -> String {
        format!(
            "|\n        Use this:\n        -----BEGIN {label}-----\n        not-a-key\n        -----END {label}-----\n"
        )
    }

    #[test]
    fn a_private_key_block_in_a_prompt_or_a_system_fires_once_per_task() {
        for label in ["PRIVATE KEY", "RSA PRIVATE KEY", "OPENSSH PRIVATE KEY"] {
            let prompt = format!(
                "nika: eg\ntasks:\n  brief:\n    infer:\n      prompt: {}",
                block(label)
            );
            assert_eq!(
                lints_of(&prompt),
                [("egress/001", "brief".to_owned())],
                "{label}"
            );
            let system = format!(
                "nika: eg\ntasks:\n  brief:\n    agent:\n      prompt: \"Sign the release\"\n      system: {}",
                block(label)
            );
            assert_eq!(
                lints_of(&system),
                [("egress/001", "brief".to_owned())],
                "{label}"
            );
        }
    }

    #[test]
    fn a_certificate_or_a_public_key_is_silent() {
        for label in ["CERTIFICATE", "PUBLIC KEY"] {
            let yaml = format!(
                "nika: eg\ntasks:\n  brief:\n    infer:\n      prompt: {}",
                block(label)
            );
            assert!(lints_of(&yaml).is_empty(), "{label}");
        }
    }

    #[test]
    fn a_prompt_file_is_judged_once_composed() {
        let yaml = "nika: eg\npermits:\n  fs: { read: [\"prompts/**\"] }\ntasks:\n  brief:\n    infer:\n      prompt: { file: \"prompts/sign.md\" }\n";
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        assert!(
            egress(&wf).is_empty(),
            "an unread file holds no text to judge"
        );
        let text = "Use this:\n-----BEGIN PRIVATE KEY-----\nnot-a-key\n-----END PRIVATE KEY-----\n";
        let mut read =
            |_: &str| -> Result<Vec<u8>, ProjectFileRead> { Ok(text.as_bytes().to_vec()) };
        let composed = compose_project_files(&wf, ".", &mut read);
        assert!(composed.findings.is_empty(), "the file composes");
        let fired: Vec<_> = (egress(&composed.workflow).into_iter())
            .map(|lint| (lint.rule, lint.task_id))
            .collect();
        assert_eq!(fired, [("egress/001", "brief".to_owned())]);
    }
}
