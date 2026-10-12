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
//! describes; this module is only the verdict a human reads. The beats a
//! project declares also meet the arming door's own laws, the ones
//! `nika arm` and `nika serve` apply, so a cadence they would refuse is
//! refused here, before anyone arms it.

use nika_cadence::error::CadenceError;
use nika_cli_host::output::VerbOutput;
use nika_vocab::project;

use crate::verbs::arm::{self, Refusal};

/// Judge a project document with the grammar that governs it.
///
/// Exit codes match the workflow lane (0 clean · 2 findings); the text
/// is [`nika_display::project_render`]'s.
#[must_use]
pub(super) fn judge(path: &str, yaml: &str, json: bool) -> VerbOutput {
    match project::parse(yaml) {
        Ok(parsed) => {
            let beats = parsed.arm().len();
            if beats > 0
                && let Err(refused) = arm::judged(yaml, beats)
            {
                return VerbOutput::file(laws_refusal(path, &refused, json));
            }
            let name = parsed.name.as_deref().unwrap_or("<unnamed>");
            if json {
                let text = nika_display::project_render::json(path, true, Some(name), "", "");
                return VerbOutput::ok(text);
            }
            let governs = nika_display::project_render::governs(
                parsed.ceiling,
                parsed.traces.is_some(),
                parsed.registry.is_some(),
                parsed.arm().len(),
            );
            VerbOutput::ok(nika_display::project_render::verdict(path, name, &governs))
        }
        Err(err) => VerbOutput::file(refusal(path, &err, json, err.detail())),
    }
}

/// A workflow cannot be judged under an invalid ambient project.
#[must_use]
pub(super) fn ambient_refusal(err: &project::ProjectError, json: bool) -> VerbOutput {
    let path = err
        .path()
        .unwrap_or_else(|| std::path::Path::new(project::FILE_NAME))
        .display()
        .to_string();
    VerbOutput::env(refusal(&path, err, json, &err.to_string()))
}

fn refusal(path: &str, err: &project::ProjectError, json: bool, message: &str) -> String {
    let slug = err.kind().spec_code();
    if json {
        return nika_display::project_render::json(path, false, None, slug, message);
    }
    nika_display::project_render::refusal(path, err.line(), slug, err.detail(), err.remedy())
}

/// The arming door's refusal as project findings: one per law broken, each with its code, the
/// beat it rides on and its fix, in the file's order.
fn laws_refusal(path: &str, refused: &Refusal, json: bool) -> String {
    let rows: Vec<[String; 3]> = match refused {
        Refusal::Unread(error) => vec![row(error)],
        Refusal::Laws(errors) => errors.iter().map(row).collect(),
        Refusal::Disagree { shape, grammar } => vec![[
            "cadence.grammar".to_owned(),
            format!("the project shape counts {shape} beats, the cadence grammar {grammar}"),
            "this is an engine fault, not a file fault: report it with the file".to_owned(),
        ]],
    };
    if json {
        let findings: Vec<[&str; 2]> = (rows.iter())
            .map(|[code, message, _]| [code.as_str(), message.as_str()])
            .collect();
        return nika_display::project_render::json_refusals(path, &findings);
    }
    let shown: Vec<[&str; 3]> = (rows.iter())
        .map(|[code, detail, fix]| [code.as_str(), detail.as_str(), fix.as_str()])
        .collect();
    nika_display::project_render::refusals(path, &shown)
}

/// One law broken: its code, what it refuses (on which beat) and its fix.
fn row(error: &CadenceError) -> [String; 3] {
    let detail = match error.on() {
        Some(beat) => format!("{beat} · {}", error.detail()),
        None => error.detail().to_owned(),
    };
    let code = error.kind().spec_code().to_owned();
    [code, detail, error.remedy().to_owned()]
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

    /// A beat whose cadence the arming door refuses is refused by `check` in the door's own
    /// words, with its fix, instead of passing here and failing when it is armed.
    #[test]
    fn a_cadence_the_arming_door_refuses_is_refused_here() {
        let yaml = concat!(
            "nika: my-project\n",
            "arm:\n",
            "  - workflow: nightly.nika\n",
            "    cadence: \"0 9 * * 1\"\n",
            "    plafond: 0.50\n",
            "    manqué: sauter\n",
        );
        let out = judge("nika.yaml", yaml, false);
        assert_eq!(out.code, nika_cli_host::output::exit::FILE, "{}", out.text);
        assert!(
            out.text.contains("cadence.tz-missing · nightly.nika · "),
            "{}",
            out.text
        );
        assert!(
            out.text.contains("fix: Prefix the cadence with `TZ="),
            "{}",
            out.text
        );
        let wire: serde_json::Value = serde_json::from_str(&judge("nika.yaml", yaml, true).text)
            .expect("the machine verdict is JSON");
        assert_eq!(wire["clean"], false, "{wire}");
        assert_eq!(wire["findings"][0]["code"], "cadence.tz-missing", "{wire}");
    }

    /// Every law the beats break is named at once, in the file's order, as `nika arm` names them.
    #[test]
    fn every_law_the_beats_break_is_named() {
        let yaml = concat!(
            "nika: my-project\n",
            "arm:\n",
            "  - workflow: a.nika\n",
            "    cadence: \"TZ=Mars/Olympus 0 9 * * 1\"\n",
            "    plafond: 0.50\n",
            "    manqué: sauter\n",
            "  - workflow: b.nika\n",
            "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
            "    plafond: 0.50\n",
            "    manqué: sauter\n",
            "    décalage: \"+7m\"\n",
        );
        let wire: serde_json::Value = serde_json::from_str(&judge("nika.yaml", yaml, true).text)
            .expect("the machine verdict is JSON");
        let codes: Vec<&str> = (wire["findings"].as_array().into_iter().flatten())
            .filter_map(|finding| finding["code"].as_str())
            .collect();
        assert_eq!(
            codes,
            ["cadence.tz-unknown", "cadence.decalage-inconnu"],
            "{wire}"
        );
    }

    /// A project the arming door admits stays green and says what it arms.
    #[test]
    fn a_project_the_arming_door_admits_stays_green() {
        let yaml = concat!(
            "nika: my-project\n",
            "arm:\n",
            "  - workflow: nightly.nika\n",
            "    cadence: \"TZ=Europe/Paris lundi 9h07\"\n",
            "    plafond: 0.50\n",
            "    manqué: sauter\n",
        );
        let out = judge("nika.yaml", yaml, false);
        assert_eq!(out.code, nika_cli_host::output::exit::OK, "{}", out.text);
        assert!(out.text.contains("1 beat armed"), "{}", out.text);
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
