// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent Skills resolution — the COMPOSER's fs edge for `skills:` (spec
//! `02-verbs.md` §agent skills).
//!
//! The static ladder (`nika-schema`) is pure; only the filesystem
//! question lives here (the `static_read_paths` precedent): read every
//! `skills:` path a workflow references — relative paths resolve from
//! the working directory, like every other relative path in nika — and
//! validate each text with the SAME parser the runtime composes with
//! (`nika_schema::parse_skill` · one voice · check≡run by construction).
//!
//! `nika check` renders the findings as its SKILLS rung (exit 2 · the
//! MODELS-rung pattern); `nika run`/`nika test` refuse on the same rows
//! BEFORE composing, then inject the resolved texts via
//! [`nika_runtime::Runtime::with_skills`].

use std::collections::BTreeMap;

use nika_schema::raw::RawWorkflow;

/// One SKILLS-rung finding — a `skills:` path that does not resolve to
/// a valid Agent Skill (the codes `nika explain` teaches).
pub(crate) struct SkillFinding {
    /// The referencing task id.
    pub task: String,
    /// The path as written in `skills:`.
    pub path: String,
    /// `NIKA-AGENT-003` (missing/unreadable) · `NIKA-AGENT-004` (invalid).
    pub code: &'static str,
    /// The human detail (names the exact repair).
    pub detail: String,
}

/// Everything the composer learned about a workflow's `skills:` — the
/// resolved texts (path-as-written → raw SKILL.md content) plus every
/// finding. `findings.is_empty()` ⇔ every referenced skill loads and
/// parses; the map then covers every referenced path.
pub(crate) struct ResolvedSkills {
    /// Path-as-written → the file's raw text (only entries that loaded
    /// AND parsed — a finding never half-populates the map).
    pub texts: BTreeMap<String, String>,
    /// The SKILLS-rung findings, reference order.
    pub findings: Vec<SkillFinding>,
}

/// Read + validate every `skills:` reference (the ONE fs edge — check ·
/// run · test all call this). Duplicate paths are read once; each
/// referencing task still gets its own finding row when the file is
/// bad (the row names WHO breaks).
pub(crate) fn resolve_skills(wf: &RawWorkflow) -> ResolvedSkills {
    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    // path → the defect detail of an already-diagnosed bad file, so N
    // references to one bad file each get a row without N reads.
    let mut defects: BTreeMap<String, (&'static str, String)> = BTreeMap::new();
    let mut findings = Vec::new();
    for (task, path) in nika_schema::skill_refs(wf) {
        let key = path.value.as_str();
        if let Some((code, detail)) = defects.get(key) {
            findings.push(SkillFinding {
                task: task.to_owned(),
                path: key.to_owned(),
                code,
                detail: detail.clone(),
            });
            continue;
        }
        if texts.contains_key(key) {
            continue; // already resolved for an earlier reference
        }
        match std::fs::read_to_string(key) {
            Err(e) => {
                let detail = format!("cannot read `{key}`: {e}");
                defects.insert(key.to_owned(), ("NIKA-AGENT-003", detail.clone()));
                findings.push(SkillFinding {
                    task: task.to_owned(),
                    path: key.to_owned(),
                    code: "NIKA-AGENT-003",
                    detail,
                });
            }
            Ok(text) => match nika_schema::parse_skill(&text) {
                Ok(_) => {
                    texts.insert(key.to_owned(), text);
                }
                Err(defect) => {
                    let detail = format!("`{key}` is not a valid Agent Skill: {defect}");
                    defects.insert(key.to_owned(), ("NIKA-AGENT-004", detail.clone()));
                    findings.push(SkillFinding {
                        task: task.to_owned(),
                        path: key.to_owned(),
                        code: "NIKA-AGENT-004",
                        detail,
                    });
                }
            },
        }
    }
    ResolvedSkills { texts, findings }
}

/// Render one finding as the shared human row — `nika check`'s SKILLS
/// rung and `nika run`/`nika test`'s refusal print THIS (one voice).
pub(crate) fn finding_row(f: &SkillFinding) -> String {
    format!("[{} · skills] task `{}` · {}", f.code, f.task, f.detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn tmp_dir() -> std::path::PathBuf {
        // Per-PROCESS dir (the #376 fixed-temp-name lesson).
        let dir = std::env::temp_dir().join(format!("nika-skills-tests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmp dir");
        dir
    }

    #[test]
    fn resolves_a_valid_skill_and_carries_its_text() {
        let dir = tmp_dir();
        let path = dir.join("good-SKILL.md");
        std::fs::write(&path, "---\nname: g\ndescription: d\n---\nbody\n").expect("fixture");
        let yaml = format!(
            "nika: v1\nworkflow: w\nmodel: mock/echo\ntasks:\n  - id: go\n    agent: {{ prompt: \"hi\", skills: [\"{}\"] }}\n",
            path.display()
        );
        let resolved = resolve_skills(&parse(&yaml));
        assert!(resolved.findings.is_empty(), "clean resolve");
        assert_eq!(
            resolved
                .texts
                .get(path.to_str().expect("utf8"))
                .map(String::as_str),
            Some("---\nname: g\ndescription: d\n---\nbody\n"),
            "the raw text rides to the runtime seam"
        );
    }

    #[test]
    fn missing_file_is_agent_003_and_invalid_is_agent_004() {
        let dir = tmp_dir();
        let bad = dir.join("frontmatterless-SKILL.md");
        std::fs::write(&bad, "# not a skill\n").expect("fixture");
        let ghost = dir.join("never-written-SKILL.md");
        let yaml = format!(
            "nika: v1\nworkflow: w\nmodel: mock/echo\ntasks:\n  - id: a\n    agent: {{ prompt: \"hi\", skills: [\"{ghost}\"] }}\n  - id: b\n    agent: {{ prompt: \"hi\", skills: [\"{bad}\"] }}\n",
            ghost = ghost.display(),
            bad = bad.display(),
        );
        let resolved = resolve_skills(&parse(&yaml));
        assert_eq!(resolved.findings.len(), 2, "one row per defect");
        assert_eq!(resolved.findings[0].code, "NIKA-AGENT-003");
        assert_eq!(resolved.findings[0].task, "a");
        assert_eq!(resolved.findings[1].code, "NIKA-AGENT-004");
        assert!(
            resolved.findings[1].detail.contains("frontmatter"),
            "the defect names the repair: {}",
            resolved.findings[1].detail
        );
        assert!(
            resolved.texts.is_empty(),
            "a finding never half-populates the runtime map"
        );
        // The shared row shape (one voice across check/run/test).
        let row = finding_row(&resolved.findings[0]);
        assert!(
            row.starts_with("[NIKA-AGENT-003 · skills] task `a`"),
            "{row}"
        );
    }

    #[test]
    fn duplicate_references_read_once_but_each_task_gets_its_row() {
        let dir = tmp_dir();
        let ghost = dir.join("shared-ghost-SKILL.md");
        let yaml = format!(
            "nika: v1\nworkflow: w\nmodel: mock/echo\ntasks:\n  - id: a\n    agent: {{ prompt: \"hi\", skills: [\"{g}\"] }}\n  - id: b\n    agent: {{ prompt: \"hi\", skills: [\"{g}\"] }}\n",
            g = ghost.display(),
        );
        let resolved = resolve_skills(&parse(&yaml));
        assert_eq!(resolved.findings.len(), 2, "both referencing tasks named");
        assert_eq!(resolved.findings[0].task, "a");
        assert_eq!(resolved.findings[1].task, "b");
    }

    #[test]
    fn a_workflow_without_skills_resolves_to_nothing() {
        let resolved = resolve_skills(&parse(
            "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n",
        ));
        assert!(resolved.texts.is_empty());
        assert!(resolved.findings.is_empty());
    }
}
