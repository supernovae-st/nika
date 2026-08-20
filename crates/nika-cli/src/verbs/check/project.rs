// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The project-file lane of `nika check`.
//!
//! `check` used to apply the nine-key WORKFLOW envelope to every document,
//! so a project file came back `NIKA-PARSE-002 missing … tasks` — on a file
//! `nika init --project-file` had just written — and obeying that finding
//! converts a correct project file into a broken workflow.
//!
//! The discriminant lives in [`nika_vocab::project`], beside the grammar it
//! describes; this module is only the verdict a human reads.

use nika_cli_host::output::VerbOutput;
use nika_vocab::project;

/// Judge a project document with the grammar that governs it.
///
/// Exit codes match the workflow lane (0 clean · 2 findings); the text
/// is [`nika_display::project_render`]'s.
#[must_use]
pub(super) fn judge(path: &str, yaml: &str, json: bool) -> VerbOutput {
    match project::parse(yaml) {
        Ok(parsed) => {
            let name = parsed.name.as_deref().unwrap_or("<unnamed>");
            if json {
                return VerbOutput::ok(format!(
                    "{{\"report_version\":1,\"file\":{path:?},\"kind\":\"project\",\
                     \"clean\":true,\"name\":{name:?},\"findings\":[]}}"
                ));
            }
            let governs = nika_display::project_render::governs(
                parsed.ceiling,
                parsed.traces.is_some(),
                parsed.registry.is_some(),
                parsed.arm().len(),
            );
            VerbOutput::ok(nika_display::project_render::verdict(path, name, &governs))
        }
        Err(err) => {
            let slug = err.kind().spec_code();
            if json {
                let detail = err.detail();
                return VerbOutput {
                    text: format!(
                        "{{\"report_version\":1,\"file\":{path:?},\"kind\":\"project\",\
                         \"clean\":false,\"findings\":[{{\"code\":{slug:?},\
                         \"message\":{detail:?}}}]}}"
                    ),
                    code: nika_cli_host::output::exit::FILE,
                };
            }
            VerbOutput::file(nika_display::project_render::refusal(
                path,
                err.line(),
                slug,
                err.detail(),
                err.remedy(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_project_is_green_and_says_what_it_governs() {
        let out = judge("nika.yaml", "nika: my-project\nceiling: 0.50\n", false);
        assert_eq!(out.code, nika_cli_host::output::exit::OK, "{}", out.text);
        assert!(out.text.contains("my-project"), "{}", out.text);
        assert!(out.text.contains("ceiling $0.50"), "{}", out.text);
    }

    #[test]
    fn a_bare_project_says_it_governs_nothing() {
        let out = judge("nika.yaml", "nika: my-project\n", false);
        assert_eq!(out.code, nika_cli_host::output::exit::OK, "{}", out.text);
        assert!(out.text.contains("governs nothing"), "{}", out.text);
    }

    #[test]
    fn a_broken_project_refuses_in_its_own_vocabulary() {
        let out = judge("nika.yaml", "nika: my-project\nceling: 0.50\n", false);
        assert_eq!(out.code, nika_cli_host::output::exit::FILE, "{}", out.text);
        assert!(out.text.contains("project."), "{}", out.text);
        assert!(
            !out.text.contains("tasks"),
            "a project refusal must never demand `tasks:`: {}",
            out.text
        );
    }
}
