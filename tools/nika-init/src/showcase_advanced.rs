// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Showcase advanced workflows — 15 production-grade workflows
//!
//! These push Nika to its limits: agents, MCP, media tools, vision,
//! for_each, structured output, guardrails, limits, diamond DAGs.
//!
//! All workflows use `{{PROVIDER}}`/`{{MODEL}}` placeholders for LLM tasks.
//! Metadata annotated in YAML comments: `requires_llm: true`, `category: advanced`.

use super::WorkflowTemplate;

const SHOWCASE_01: &str = include_str!("showcase_advanced/01.yaml");
const SHOWCASE_02: &str = include_str!("showcase_advanced/02.yaml");
const SHOWCASE_03: &str = include_str!("showcase_advanced/03.yaml");
const SHOWCASE_04: &str = include_str!("showcase_advanced/04.yaml");
const SHOWCASE_05: &str = include_str!("showcase_advanced/05.yaml");
const SHOWCASE_06: &str = include_str!("showcase_advanced/06.yaml");
const SHOWCASE_07: &str = include_str!("showcase_advanced/07.yaml");
const SHOWCASE_08: &str = include_str!("showcase_advanced/08.yaml");
const SHOWCASE_09: &str = include_str!("showcase_advanced/09.yaml");
const SHOWCASE_10: &str = include_str!("showcase_advanced/10.yaml");
const SHOWCASE_11: &str = include_str!("showcase_advanced/11.yaml");
const SHOWCASE_12: &str = include_str!("showcase_advanced/12.yaml");
const SHOWCASE_13: &str = include_str!("showcase_advanced/13.yaml");
const SHOWCASE_14: &str = include_str!("showcase_advanced/14.yaml");
const SHOWCASE_15: &str = include_str!("showcase_advanced/15.yaml");

/// Return all 15 showcase advanced workflows
pub fn get_showcase_advanced_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "01-image-optimization-pipeline.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_01,
        },
        WorkflowTemplate {
            filename: "02-qr-code-validator.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_02,
        },
        WorkflowTemplate {
            filename: "03-pdf-text-extractor.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_03,
        },
        WorkflowTemplate {
            filename: "04-chart-generator.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_04,
        },
        WorkflowTemplate {
            filename: "05-website-screenshot-analyzer.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_05,
        },
        WorkflowTemplate {
            filename: "06-multi-provider-debate.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_06,
        },
        WorkflowTemplate {
            filename: "07-agent-code-reviewer.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_07,
        },
        WorkflowTemplate {
            filename: "08-agent-file-organizer.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_08,
        },
        WorkflowTemplate {
            filename: "09-research-agent-with-web.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_09,
        },
        WorkflowTemplate {
            filename: "10-parallel-web-scraper.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_10,
        },
        WorkflowTemplate {
            filename: "11-content-factory.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_11,
        },
        WorkflowTemplate {
            filename: "12-data-etl-pipeline.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_12,
        },
        WorkflowTemplate {
            filename: "13-multi-step-agent.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_13,
        },
        WorkflowTemplate {
            filename: "14-guardrailed-agent.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_14,
        },
        WorkflowTemplate {
            filename: "15-full-orchestration.nika.yaml",
            tier_dir: "showcase-advanced",
            content: SHOWCASE_15,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_showcase_count() {
        assert_eq!(
            get_showcase_advanced_workflows().len(),
            15,
            "Must have exactly 15 showcase workflows"
        );
    }

    #[test]
    fn test_showcase_filenames_unique() {
        let workflows = get_showcase_advanced_workflows();
        let mut names: Vec<&str> = workflows.iter().map(|w| w.filename).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "All filenames must be unique");
    }

    #[test]
    fn test_showcase_all_have_schema() {
        for w in &get_showcase_advanced_workflows() {
            assert!(
                w.content.contains("schema: \"nika/workflow@0.12\""),
                "Workflow {} must declare schema",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_all_have_workflow_name() {
        for w in &get_showcase_advanced_workflows() {
            assert!(
                w.content.contains("workflow:"),
                "{} needs workflow:",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_all_have_tasks() {
        for w in &get_showcase_advanced_workflows() {
            assert!(w.content.contains("tasks:"), "{} needs tasks:", w.filename);
        }
    }

    #[test]
    fn test_showcase_all_have_provider_model() {
        for w in &get_showcase_advanced_workflows() {
            assert!(
                w.content.contains("{{PROVIDER}}"),
                "{} needs PROVIDER",
                w.filename
            );
            assert!(
                w.content.contains("{{MODEL}}"),
                "{} needs MODEL",
                w.filename
            );
        }
    }

    #[test]
    fn test_showcase_all_marked_requires_llm() {
        for w in &get_showcase_advanced_workflows() {
            assert!(w.content.contains("requires_llm: true"), "{}", w.filename);
        }
    }

    #[test]
    fn test_showcase_all_marked_category_advanced() {
        for w in &get_showcase_advanced_workflows() {
            assert!(w.content.contains("category: advanced"), "{}", w.filename);
        }
    }

    #[test]
    fn test_showcase_features_coverage() {
        let all: String = get_showcase_advanced_workflows()
            .iter()
            .map(|w| w.content)
            .collect::<Vec<_>>()
            .join("\n");

        for verb in ["exec:", "fetch:", "infer:", "invoke:", "agent:"] {
            assert!(all.contains(verb), "Must cover {verb}");
        }
        for feat in [
            "for_each:",
            "structured:",
            "guardrails:",
            "limits:",
            "completion:",
            "mcp:",
            "inputs:",
        ] {
            assert!(all.contains(feat), "Must use {feat}");
        }
        for tool in [
            "nika:pipeline",
            "nika:chart",
            "nika:qr_validate",
            "nika:pdf_extract",
            "nika:glob",
            "nika:read",
            "nika:write",
            "nika:grep",
            "nika:log",
        ] {
            assert!(all.contains(tool), "Must use {tool}");
        }
        assert!(all.contains("type: image"), "Must use vision");
        assert!(
            all.contains("extract: markdown"),
            "Must use extract: markdown"
        );
        assert!(
            all.contains("response: binary"),
            "Must use response: binary"
        );
    }

    #[test]
    fn test_showcase_unique_ids_per_workflow() {
        for w in &get_showcase_advanced_workflows() {
            let mut ids: Vec<&str> = w
                .content
                .lines()
                .filter_map(|l| l.trim().strip_prefix("- id: ").map(str::trim))
                .collect();
            let len = ids.len();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), len, "{} has duplicate IDs", w.filename);
        }
    }

    #[test]
    fn test_showcase_depends_on_valid() {
        for w in &get_showcase_advanced_workflows() {
            let ids: Vec<String> = w
                .content
                .lines()
                .filter_map(|l| {
                    l.trim()
                        .strip_prefix("- id: ")
                        .map(|s| s.trim().to_string())
                })
                .collect();
            for line in w.content.lines() {
                if let Some(deps) = line.trim().strip_prefix("depends_on: [") {
                    for dep in deps.trim_end_matches(']').split(',') {
                        let dep = dep.trim();
                        if !dep.is_empty() {
                            assert!(
                                ids.contains(&dep.to_string()),
                                "{}: unknown dep '{dep}'",
                                w.filename
                            );
                        }
                    }
                }
            }
        }
    }
}
