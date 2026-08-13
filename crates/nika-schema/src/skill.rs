// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent Skills (`SKILL.md`) — the CONSUMER half of the format nika
//! already produces (`nika init` writes `.agents/skills/nika-authoring/
//! SKILL.md` · agentskills.io shape).
//!
//! An Agent Skill is a markdown file opening with a YAML frontmatter
//! block (`---` fences) that MUST carry a non-empty `name` and
//! `description`; the markdown body after the closing fence is the
//! skill's instructions. The `agent:` verb's `skills:` field names such
//! files by path; the COMPOSER (the CLI · never the L3 runtime) reads
//! them, and the resolved texts join the agent's system context in a
//! deterministic `## Skills` section (spec `02-verbs.md` §agent skills).
//!
//! This module is PURE (L0 · zero I/O): it parses skill TEXT and walks a
//! parsed workflow for `skills:` references. Both `nika check` (the
//! static findings · `NIKA-AGENT-003`/`NIKA-AGENT-004`) and the run
//! composer consume THESE functions — one voice, check≡run by
//! construction.

use crate::raw::{RawAction, RawWorkflow};
use crate::source::Spanned;

/// One parsed Agent Skill — the agentskills.io frontmatter contract
/// (`name` + `description` required · other frontmatter keys are the
/// skill author's surface and are tolerated) plus the markdown body.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SkillDoc {
    /// Frontmatter `name:` — non-empty by construction.
    pub name: String,
    /// Frontmatter `description:` — non-empty by construction.
    pub description: String,
    /// The markdown body after the closing `---` fence (may be empty —
    /// a name+description-only skill is legal, if spartan).
    pub body: String,
}

impl SkillDoc {
    /// Construct a skill doc (INV-019 · `#[non_exhaustive]` structs ship
    /// a constructor so harnesses/embedders can build one).
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            body: body.into(),
        }
    }
}

/// Why a text is not a valid Agent Skill (the `NIKA-AGENT-004` class —
/// each variant names the exact repair).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SkillDefect {
    /// The file does not open with a `---` frontmatter fence.
    #[error(
        "no YAML frontmatter — an Agent Skill opens with `---` on line 1 \
         (agentskills.io shape: `---\\nname: …\\ndescription: …\\n---`)"
    )]
    NoFrontmatter,
    /// The opening fence never closes.
    #[error("unterminated frontmatter — the opening `---` has no closing `---` line")]
    UnterminatedFrontmatter,
    /// The frontmatter is not parseable YAML (or not a mapping).
    #[error("frontmatter is not a YAML mapping: {reason}")]
    FrontmatterNotAMapping {
        /// What the YAML parser said (or the shape found).
        reason: String,
    },
    /// `name:` is absent or empty.
    #[error("frontmatter `name:` is missing or empty — a skill must name itself")]
    MissingName,
    /// `description:` is absent or empty.
    #[error(
        "frontmatter `description:` is missing or empty — the description \
         is what tells the agent when the skill applies"
    )]
    MissingDescription,
}

/// Parse one SKILL.md text (agentskills.io shape).
///
/// Frontmatter grammar: line 1 is exactly `---`, the block ends at the
/// next line that is exactly `---`, the YAML between must be a mapping
/// with non-empty `name` and `description` scalars. Unknown frontmatter
/// keys (`license` · `metadata` · client-specific fields) are tolerated —
/// the consumer reads what it needs and never rejects a skill for
/// carrying more.
///
/// # Errors
///
/// A [`SkillDefect`] naming the exact repair (the `NIKA-AGENT-004`
/// finding class).
pub fn parse_skill(text: &str) -> Result<SkillDoc, SkillDefect> {
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return Err(SkillDefect::NoFrontmatter);
    };
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return Err(SkillDefect::NoFrontmatter);
    }
    // Scan for the closing fence · everything between is the YAML block.
    let mut yaml = String::new();
    let mut body = String::new();
    let mut closed = false;
    for line in lines {
        if closed {
            body.push_str(line);
        } else if line.trim_end_matches(['\r', '\n']) == "---" {
            closed = true;
        } else {
            yaml.push_str(line);
        }
    }
    if !closed {
        return Err(SkillDefect::UnterminatedFrontmatter);
    }
    let node =
        marked_yaml::parse_yaml(0, &yaml).map_err(|e| SkillDefect::FrontmatterNotAMapping {
            reason: e.to_string(),
        })?;
    let Some(mapping) = node.as_mapping() else {
        return Err(SkillDefect::FrontmatterNotAMapping {
            reason: "the frontmatter YAML is not a key/value mapping".to_owned(),
        });
    };
    let scalar = |key: &str| -> Option<String> {
        mapping
            .get_node(key)
            .and_then(marked_yaml::Node::as_scalar)
            .map(|s| s.as_str().trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let name = scalar("name").ok_or(SkillDefect::MissingName)?;
    let description = scalar("description").ok_or(SkillDefect::MissingDescription)?;
    Ok(SkillDoc {
        name,
        description,
        body,
    })
}

/// Every `skills:` reference in a parsed workflow — `(task id, path)`
/// in declaration order, main verbs AND `on_finally` mini-tasks (the
/// mini parser shares the verb grammar, so a mini agent may carry
/// skills too). The ONE enumeration `nika check`, the run composer and
/// the resume identity all walk — they cannot drift apart.
#[must_use]
pub fn skill_refs(wf: &RawWorkflow) -> Vec<(&str, &Spanned<String>)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        if let RawAction::Agent(a) = &task.value.action {
            out.extend(a.skills.iter().map(|s| (id, s)));
        }
    }
    out
}

/// One SKILLS finding — a `skills:` reference that does not resolve to
/// a valid Agent Skill (`nika check`'s SKILLS rung · the run/test
/// refusal · the codes `nika explain` teaches).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SkillFinding {
    /// The referencing task id.
    pub task: String,
    /// The path as written in `skills:`.
    pub path: String,
    /// `NIKA-AGENT-003` (missing/unreadable) · `NIKA-AGENT-004` (invalid).
    pub code: &'static str,
    /// The human detail (names the exact repair).
    pub detail: String,
}

impl SkillFinding {
    /// The shared human row — every surface (check rung · run/test
    /// refusal) prints THIS (one voice), fix pointer included.
    #[must_use]
    pub fn row(&self) -> String {
        format!(
            "[{code} · skills] task `{task}` · {detail} · fix: nika explain {code}",
            code = self.code,
            task = self.task,
            detail = self.detail,
        )
    }

    /// The machine row (`check --json` `skill_findings[]`).
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "task": self.task,
            "path": self.path,
            "code": self.code,
            "detail": self.detail,
            "docs_url": format!("{}/{}", crate::error::ERROR_DOCS_BASE, self.code),
        })
    }
}

/// Everything a composer learns about a workflow's `skills:` — the
/// resolved texts (path-as-written → raw SKILL.md content) plus every
/// finding. `findings.is_empty()` ⇔ every referenced skill loads and
/// parses; the map then covers every referenced path.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ResolvedSkills {
    /// Path-as-written → the file's raw text (only entries that loaded
    /// AND parsed — a finding never half-populates the map).
    pub texts: std::collections::BTreeMap<String, String>,
    /// The SKILLS-rung findings, reference order.
    pub findings: Vec<SkillFinding>,
}

impl ResolvedSkills {
    /// The check rung, presentation-free: `None` when the workflow
    /// references no skills (print nothing) · else the green message +
    /// the finding rows (empty rows ⇔ green). The CLI paints; the words
    /// live HERE beside the findings they describe.
    #[must_use]
    pub fn rung(&self) -> Option<(String, Vec<String>)> {
        if self.texts.is_empty() && self.findings.is_empty() {
            return None;
        }
        Some((
            format!(
                "{} skill(s) resolve (agentskills.io shape)",
                self.texts.len()
            ),
            self.findings.iter().map(SkillFinding::row).collect(),
        ))
    }

    /// Insert the `check --json` machine keys (`skills_resolve` +
    /// `skill_findings[]` when red) — the report shape lives beside
    /// `nika_check::CheckReport`'s own serialization (the check ladder
    /// crate — a doc name, not a link: this crate never depends upward).
    pub fn extend_check_json(&self, obj: &mut serde_json::Map<String, serde_json::Value>) {
        obj.insert("skills_resolve".to_owned(), self.findings.is_empty().into());
        if !self.findings.is_empty() {
            let rows = self.findings.iter().map(SkillFinding::json).collect();
            obj.insert("skill_findings".to_owned(), serde_json::Value::Array(rows));
        }
    }
}

/// Resolve every `skills:` reference through an injected reader (the
/// caller's fs edge — this crate stays zero-I/O). The ONE resolution
/// `nika check` · `nika run` · `nika test` all call: same reader shape,
/// same findings, check≡run by construction. Duplicate paths are read
/// once; each referencing task still gets its own finding row when the
/// file is bad (the row names WHO breaks).
pub fn resolve_skills(
    wf: &RawWorkflow,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
) -> ResolvedSkills {
    let mut resolved = ResolvedSkills::default();
    // path → the defect of an already-diagnosed bad file (N references
    // to one bad file each get a row without N reads).
    let mut defects: std::collections::BTreeMap<String, (&'static str, String)> =
        std::collections::BTreeMap::new();
    for (task, path) in skill_refs(wf) {
        let key = path.value.as_str();
        if resolved.texts.contains_key(key) {
            continue; // already resolved for an earlier reference
        }
        // The `skills:` fs edge lives INSIDE the boundary. The grant is
        // judged BEFORE the reader runs · a refused path is never read,
        // so nothing of it can reach the agent's prompt. Absent `permits:`
        // is zero authority (F-O8), and `permits: {}` denies by default
        // (an omitted `fs` block forbids every path · `nika-cap::fit`).
        //
        // Falsified at 0.108.0 before this gate existed: a VALID skill at
        // an absolute path outside `permits: {}` · whose own doc says the
        // workflow "opens no file" · was read, and the engine reported on
        // its CONTENT while the PERMITS and TRIFECTA rungs stayed green.
        let granted = wf
            .permits
            .as_ref()
            .is_some_and(|p| p.value.allows_path(key, false));
        let (code, detail) = match defects.get(key) {
            Some((code, detail)) => (*code, detail.clone()),
            None if !granted => {
                let d = format!(
                    "`{key}` is outside the permits.fs.read boundary · grant the path, \
                     or move the skill inside one the file already grants"
                );
                defects.insert(key.to_owned(), ("NIKA-SEC-004", d.clone()));
                ("NIKA-SEC-004", d)
            }
            None => match read(key) {
                Ok(text) => match parse_skill(&text) {
                    Ok(_) => {
                        resolved.texts.insert(key.to_owned(), text);
                        continue;
                    }
                    Err(defect) => {
                        let d = format!("`{key}` is not a valid Agent Skill: {defect}");
                        defects.insert(key.to_owned(), ("NIKA-AGENT-004", d.clone()));
                        ("NIKA-AGENT-004", d)
                    }
                },
                Err(e) => {
                    let d = format!("cannot read `{key}`: {e}");
                    defects.insert(key.to_owned(), ("NIKA-AGENT-003", d.clone()));
                    ("NIKA-AGENT-003", d)
                }
            },
        };
        resolved.findings.push(SkillFinding {
            task: task.to_owned(),
            path: key.to_owned(),
            code,
            detail,
        });
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    const WELL_FORMED: &str = "---\nname: code-review\ndescription: Review a diff for defects.\nlicense: Apache-2.0\n---\n\n# Code review\n\nLook for bugs first.\n";

    #[test]
    fn parses_the_agentskills_shape() {
        let doc = parse_skill(WELL_FORMED).expect("well-formed skill parses");
        assert_eq!(doc.name, "code-review");
        assert_eq!(doc.description, "Review a diff for defects.");
        assert!(doc.body.contains("# Code review"), "{}", doc.body);
        assert!(
            !doc.body.contains("license"),
            "frontmatter never leaks into the body"
        );
    }

    #[test]
    fn parses_the_skill_nika_init_writes() {
        // The producer half is the fixture: what `nika init` writes, the
        // consumer must accept (the round-trip the issue names).
        let produced = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.agents/plugins/nika/skills/nika-authoring/SKILL.md"
        ));
        let doc = parse_skill(produced).expect("our own skill parses");
        assert_eq!(doc.name, "nika-authoring");
        assert!(doc.description.starts_with("Author, check and repair"));
        assert!(doc.body.contains("nika check"));
    }

    #[test]
    fn unknown_frontmatter_keys_are_tolerated() {
        let text = "---\nname: x\ndescription: y\nmetadata:\n  author: someone\nallowed-tools: [Bash]\n---\nbody\n";
        assert!(parse_skill(text).is_ok(), "extra keys are the author's");
    }

    #[test]
    fn missing_frontmatter_is_the_no_frontmatter_defect() {
        assert_eq!(
            parse_skill("# Just markdown\n"),
            Err(SkillDefect::NoFrontmatter)
        );
        assert_eq!(parse_skill(""), Err(SkillDefect::NoFrontmatter));
        // An unclosed fence is its own defect (names the repair).
        assert_eq!(
            parse_skill("---\nname: x\n"),
            Err(SkillDefect::UnterminatedFrontmatter)
        );
    }

    #[test]
    fn empty_name_or_description_is_a_defect() {
        assert_eq!(
            parse_skill("---\ndescription: y\n---\nbody"),
            Err(SkillDefect::MissingName)
        );
        assert_eq!(
            parse_skill("---\nname: \"\"\ndescription: y\n---\nbody"),
            Err(SkillDefect::MissingName)
        );
        assert_eq!(
            parse_skill("---\nname: x\n---\nbody"),
            Err(SkillDefect::MissingDescription)
        );
        assert_eq!(
            parse_skill("---\nname: x\ndescription: \"  \"\n---\nbody"),
            Err(SkillDefect::MissingDescription)
        );
    }

    #[test]
    fn non_mapping_frontmatter_is_a_defect() {
        let err = parse_skill("---\n- a\n- b\n---\nbody").expect_err("sequence frontmatter");
        assert!(
            matches!(err, SkillDefect::FrontmatterNotAMapping { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn crlf_fences_parse() {
        let text = "---\r\nname: x\r\ndescription: y\r\n---\r\nbody\r\n";
        let doc = parse_skill(text).expect("CRLF skill parses");
        assert_eq!(doc.name, "x");
    }

    /// The `skills:` fs edge is INSIDE the boundary · the reader never runs
    /// for a path the file does not grant. The fixture is a skill that would
    /// PARSE FINE · only a refusal can keep it out of `texts`, so the test
    /// cannot go green on a malformation (the first version of this probe
    /// pointed at an invalid file and passed for the wrong reason).
    #[test]
    fn resolve_skills_refuses_a_path_outside_the_boundary() {
        let cases = [
            // the DECLARED ZERO · its own doc says "opens no file"
            ("permits: {}\n", "declared zero"),
            // absent · zero authority (F-O8)
            ("", "absent permits"),
            // granted, but ELSEWHERE
            (
                "permits:\n  fs:\n    read: [\"allowed/SKILL.md\"]\n",
                "grant elsewhere",
            ),
        ];
        for (permits, label) in cases {
            let yaml = format!(
                "nika: w\nmodel: mock/echo\n{permits}\
                 tasks:\n  a:\n    agent: {{ prompt: \"hi\", skills: [\"outside/SKILL.md\"] }}\n"
            );
            let wf = crate::parse(&yaml, crate::FileId::new(0), crate::ParseMode::Strict)
                .expect("fixture parses");
            let mut reads = 0usize;
            let resolved = resolve_skills(&wf, &mut |_| {
                reads += 1;
                Ok("---\nname: g\ndescription: d\n---\nbody\n".to_owned())
            });
            assert_eq!(reads, 0, "{label}: the reader must never run");
            assert!(resolved.texts.is_empty(), "{label}: nothing may carry");
            assert_eq!(
                resolved.findings.first().map(|f| f.code),
                Some("NIKA-SEC-004"),
                "{label}: the refusal is a boundary escape, not a read defect"
            );
        }
    }

    #[test]
    fn resolve_skills_splits_003_and_004_and_carries_texts() {
        // The pure resolution over an injected reader (the CLI's fs edge
        // stays 3 lines): a good skill carries its raw text; a missing
        // file is NIKA-AGENT-003; an invalid one is NIKA-AGENT-004; a
        // duplicated bad reference gets a row PER TASK without re-reads.
        // The grant is a PRECONDITION of the read since the boundary gate
        // landed · this fixture measures the 003/004 split, not the
        // boundary, so it declares what it reaches (see the sibling test
        // `resolve_skills_refuses_a_path_outside_the_boundary`).
        let yaml = "\
nika: w
model: mock/echo
permits:
  fs:
    read: [\"good/SKILL.md\", \"bad/SKILL.md\", \"ghost/SKILL.md\"]
tasks:
  a:
    agent: { prompt: \"hi\", skills: [\"good/SKILL.md\", \"ghost/SKILL.md\"] }
  b:
    agent: { prompt: \"hi\", skills: [\"bad/SKILL.md\", \"ghost/SKILL.md\"] }
";
        let wf = crate::parse(yaml, crate::FileId::new(0), crate::ParseMode::Strict)
            .expect("fixture parses");
        let mut reads = 0usize;
        let resolved = resolve_skills(&wf, &mut |path| {
            reads += 1;
            match path {
                "good/SKILL.md" => Ok("---\nname: g\ndescription: d\n---\nbody\n".to_owned()),
                "bad/SKILL.md" => Ok("# no frontmatter\n".to_owned()),
                _ => Err("No such file or directory (os error 2)".to_owned()),
            }
        });
        assert_eq!(
            resolved.texts.get("good/SKILL.md").map(String::as_str),
            Some("---\nname: g\ndescription: d\n---\nbody\n"),
            "the raw text rides to the runtime seam"
        );
        let flat: Vec<(&str, &str, &str)> = resolved
            .findings
            .iter()
            .map(|f| (f.task.as_str(), f.code, f.path.as_str()))
            .collect();
        assert_eq!(
            flat,
            vec![
                ("a", "NIKA-AGENT-003", "ghost/SKILL.md"),
                ("b", "NIKA-AGENT-004", "bad/SKILL.md"),
                ("b", "NIKA-AGENT-003", "ghost/SKILL.md"),
            ],
            "one row per referencing task · codes split by defect class"
        );
        assert_eq!(reads, 3, "duplicate references never re-read");
        assert!(
            !resolved.texts.contains_key("bad/SKILL.md"),
            "a finding never half-populates the map"
        );
        // The shared voices: the human row leads with the code + carries
        // the fix pointer; the machine row carries the docs_url.
        let row = resolved.findings[0].row();
        assert!(
            row.starts_with("[NIKA-AGENT-003 · skills] task `a`")
                && row.ends_with("fix: nika explain NIKA-AGENT-003"),
            "{row}"
        );
        let json = resolved.findings[1].json();
        assert_eq!(json["code"], "NIKA-AGENT-004");
        assert!(
            json["docs_url"]
                .as_str()
                .expect("docs_url")
                .ends_with("/NIKA-AGENT-004"),
            "{json:#}"
        );
    }

    #[test]
    fn resolve_skills_is_empty_for_a_skill_less_workflow() {
        let wf = crate::parse(
            "nika: w\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n",
            crate::FileId::new(0),
            crate::ParseMode::Strict,
        )
        .expect("fixture parses");
        let resolved = resolve_skills(&wf, &mut |_| panic!("no reference → no read"));
        assert!(resolved.texts.is_empty() && resolved.findings.is_empty());
    }
}
