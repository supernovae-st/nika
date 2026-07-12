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
        for mini in &task.value.on_finally {
            if let RawAction::Agent(a) = &mini.value.action {
                out.extend(a.skills.iter().map(|s| (id, s)));
            }
        }
    }
    out
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

    #[test]
    fn skill_refs_walks_main_and_finally_actions() {
        let yaml = "\
nika: v1
workflow: w
tasks:
  - id: a
    agent:
      prompt: \"go\"
      skills: [\"s1/SKILL.md\", \"s2/SKILL.md\"]
  - id: b
    exec: { command: \"echo hi\" }
    on_finally:
      - agent:
          prompt: \"wrap up\"
          skills: [\"s3/SKILL.md\"]
";
        let wf = crate::parse(yaml, crate::FileId::new(0), crate::ParseMode::Strict)
            .expect("fixture parses");
        let refs = skill_refs(&wf);
        let flat: Vec<(&str, &str)> = refs.iter().map(|(id, s)| (*id, s.value.as_str())).collect();
        assert_eq!(
            flat,
            vec![
                ("a", "s1/SKILL.md"),
                ("a", "s2/SKILL.md"),
                ("b", "s3/SKILL.md"),
            ]
        );
    }
}
