//! Integration tests for Use Case Workflows
//!
//! Tests parsing and DAG validation of the 5 use case workflows:
//! - UC1: Multi-Locale Page Generation
//! - UC2: SEO Content Sprint
//! - UC3: Entity Knowledge Retrieval
//! - UC4: Block Generation with Locale Context
//! - UC5: Semantic Content Planning
//!
//! ## Running
//!
//! ```bash
//! cargo nextest run use_case_workflows
//! ```

use nika::Workflow;
use std::fs;
use std::path::Path;

// ═══════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════

fn load_workflow(filename: &str) -> Workflow {
    let path = format!("examples/{}", filename);
    let yaml =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path, e));
    serde_yaml::from_str(&yaml).unwrap_or_else(|e| panic!("Failed to parse {}: {}", path, e))
}

fn assert_workflow_valid(workflow: &Workflow) {
    workflow
        .validate_schema()
        .expect("Workflow schema validation failed");
}

fn assert_task_exists(workflow: &Workflow, task_id: &str) {
    let exists = workflow.tasks.iter().any(|t| t.id == task_id);
    assert!(exists, "Task '{}' not found in workflow", task_id);
}

fn assert_mcp_server(workflow: &Workflow, server_name: &str) {
    let mcp = workflow.mcp.as_ref().expect("No MCP configuration");
    assert!(
        mcp.contains_key(server_name),
        "MCP server '{}' not found in workflow",
        server_name
    );
}

// ═══════════════════════════════════════════════════════════════
// UC1: Multi-Locale Page Generation
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_uc1_workflow_parses() {
    let workflow = load_workflow("uc1-generate-page-multilingual.nika.yaml");
    assert_workflow_valid(&workflow);
}

#[test]
fn test_uc1_has_correct_tasks() {
    let workflow = load_workflow("uc1-generate-page-multilingual.nika.yaml");

    // Input task (page_key provides the focus key)
    assert_task_exists(&workflow, "page_key");

    // Generation tasks (fan-out)
    assert_task_exists(&workflow, "gen_fr");
    assert_task_exists(&workflow, "gen_es");
    assert_task_exists(&workflow, "gen_de");
    assert_task_exists(&workflow, "gen_ja");
    assert_task_exists(&workflow, "gen_pt");

    // Report task (fan-in)
    assert_task_exists(&workflow, "report");

    assert_eq!(workflow.tasks.len(), 7);
}

#[test]
fn test_uc1_has_mcp_config() {
    let workflow = load_workflow("uc1-generate-page-multilingual.nika.yaml");
    assert_mcp_server(&workflow, "novanet");
}

#[test]
fn test_uc1_has_correct_flows() {
    let workflow = load_workflow("uc1-generate-page-multilingual.nika.yaml");

    // Should have 2 flows:
    // 1. entity_key → [gen_fr, gen_es, gen_de, gen_ja, gen_pt]
    // 2. [gen_fr, gen_es, gen_de, gen_ja, gen_pt] → report
    assert_eq!(workflow.flows.len(), 2);
}

// ═══════════════════════════════════════════════════════════════
// UC2: SEO Content Sprint
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_uc2_workflow_parses() {
    let workflow = load_workflow("uc2-seo-content-sprint.nika.yaml");
    assert_workflow_valid(&workflow);
}

#[test]
fn test_uc2_has_correct_tasks() {
    let workflow = load_workflow("uc2-seo-content-sprint.nika.yaml");

    // Discovery + Strategy
    assert_task_exists(&workflow, "discover_entities");
    assert_task_exists(&workflow, "seo_strategy");

    // SEO generation tasks
    assert_task_exists(&workflow, "seo_fr_menu");
    assert_task_exists(&workflow, "seo_en_menu");
    assert_task_exists(&workflow, "seo_mx_menu");

    // Report
    assert_task_exists(&workflow, "seo_report");

    assert_eq!(workflow.tasks.len(), 6);
}

#[test]
fn test_uc2_has_correct_flows() {
    let workflow = load_workflow("uc2-seo-content-sprint.nika.yaml");

    // Should have 3 flows:
    // 1. discover_entities → seo_strategy
    // 2. seo_strategy → [seo_fr_menu, seo_en_menu, seo_mx_menu]
    // 3. [seo_fr_menu, seo_en_menu, seo_mx_menu] → seo_report
    assert_eq!(workflow.flows.len(), 3);
}

// ═══════════════════════════════════════════════════════════════
// UC3: Entity Knowledge Retrieval
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_uc3_workflow_parses() {
    let workflow = load_workflow("uc3-entity-knowledge-retrieval.nika.yaml");
    assert_workflow_valid(&workflow);
}

#[test]
fn test_uc3_has_correct_tasks() {
    let workflow = load_workflow("uc3-entity-knowledge-retrieval.nika.yaml");

    // Input
    assert_task_exists(&workflow, "entity_key");

    // Parallel traversals (fan-out)
    assert_task_exists(&workflow, "describe");
    assert_task_exists(&workflow, "requires");
    assert_task_exists(&workflow, "enables");
    assert_task_exists(&workflow, "similar_to");
    assert_task_exists(&workflow, "industries");

    // Synthesis (fan-in)
    assert_task_exists(&workflow, "knowledge_map");

    assert_eq!(workflow.tasks.len(), 7);
}

#[test]
fn test_uc3_has_correct_flows() {
    let workflow = load_workflow("uc3-entity-knowledge-retrieval.nika.yaml");

    // Should have 2 flows:
    // 1. entity_key → [describe, requires, enables, similar_to, industries]
    // 2. [describe, requires, enables, similar_to, industries] → knowledge_map
    assert_eq!(workflow.flows.len(), 2);
}

// ═══════════════════════════════════════════════════════════════
// UC4: Block Generation with Locale Context
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_uc4_workflow_parses() {
    let workflow = load_workflow("uc4-block-generation-locale-aware.nika.yaml");
    assert_workflow_valid(&workflow);
}

#[test]
fn test_uc4_has_correct_tasks() {
    let workflow = load_workflow("uc4-block-generation-locale-aware.nika.yaml");

    // Inputs
    assert_task_exists(&workflow, "entity_key");
    assert_task_exists(&workflow, "locale_key");

    // Context loading (fan-out)
    assert_task_exists(&workflow, "entity_context");
    assert_task_exists(&workflow, "locale_voice");
    assert_task_exists(&workflow, "locale_expressions");
    assert_task_exists(&workflow, "locale_taboos");

    // Generation + Validation
    assert_task_exists(&workflow, "generate_hero");
    assert_task_exists(&workflow, "validate_block");

    assert_eq!(workflow.tasks.len(), 8);
}

#[test]
fn test_uc4_has_correct_flows() {
    let workflow = load_workflow("uc4-block-generation-locale-aware.nika.yaml");

    // Should have 4 flows:
    // 1. [entity_key, locale_key] → entity_context
    // 2. locale_key → [locale_voice, locale_expressions, locale_taboos]
    // 3. [entity_context, locale_voice, locale_expressions, locale_taboos] → generate_hero
    // 4. generate_hero → validate_block
    assert_eq!(workflow.flows.len(), 4);
}

// ═══════════════════════════════════════════════════════════════
// UC5: Semantic Content Planning
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_uc5_workflow_parses() {
    let workflow = load_workflow("uc5-semantic-content-planning.nika.yaml");
    assert_workflow_valid(&workflow);
}

#[test]
fn test_uc5_has_correct_tasks() {
    let workflow = load_workflow("uc5-semantic-content-planning.nika.yaml");

    // Discovery
    assert_task_exists(&workflow, "discover_pillars");

    // Coverage tasks (fan-out)
    assert_task_exists(&workflow, "coverage_fr");
    assert_task_exists(&workflow, "coverage_en");
    assert_task_exists(&workflow, "coverage_es");
    assert_task_exists(&workflow, "coverage_de");
    assert_task_exists(&workflow, "coverage_pt");
    assert_task_exists(&workflow, "coverage_ja");

    // Analysis + Plan (fan-in)
    assert_task_exists(&workflow, "gap_analysis");
    assert_task_exists(&workflow, "sprint_plan");

    assert_eq!(workflow.tasks.len(), 9);
}

#[test]
fn test_uc5_has_correct_flows() {
    let workflow = load_workflow("uc5-semantic-content-planning.nika.yaml");

    // Should have 3 flows:
    // 1. discover_pillars → [coverage_fr, coverage_en, coverage_es, coverage_de, coverage_pt, coverage_ja]
    // 2. [discover_pillars, coverage_*] → gap_analysis
    // 3. gap_analysis → sprint_plan
    assert_eq!(workflow.flows.len(), 3);
}

// ═══════════════════════════════════════════════════════════════
// Cross-Workflow Validation
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_all_use_case_workflows_exist() {
    let examples_dir = Path::new("examples");

    let expected_files = [
        "uc1-generate-page-multilingual.nika.yaml",
        "uc2-seo-content-sprint.nika.yaml",
        "uc3-entity-knowledge-retrieval.nika.yaml",
        "uc4-block-generation-locale-aware.nika.yaml",
        "uc5-semantic-content-planning.nika.yaml",
    ];

    for filename in expected_files {
        let path = examples_dir.join(filename);
        assert!(path.exists(), "Missing use case workflow: {}", filename);
    }
}

#[test]
fn test_all_use_case_workflows_use_schema_v02() {
    let workflows = [
        "uc1-generate-page-multilingual.nika.yaml",
        "uc2-seo-content-sprint.nika.yaml",
        "uc3-entity-knowledge-retrieval.nika.yaml",
        "uc4-block-generation-locale-aware.nika.yaml",
        "uc5-semantic-content-planning.nika.yaml",
    ];

    for filename in workflows {
        let workflow = load_workflow(filename);
        assert_eq!(
            workflow.schema, "nika/workflow@0.2",
            "{} should use schema v0.2",
            filename
        );
    }
}

#[test]
fn test_all_use_case_workflows_have_novanet_mcp() {
    let workflows = [
        "uc1-generate-page-multilingual.nika.yaml",
        "uc2-seo-content-sprint.nika.yaml",
        "uc3-entity-knowledge-retrieval.nika.yaml",
        "uc4-block-generation-locale-aware.nika.yaml",
        "uc5-semantic-content-planning.nika.yaml",
    ];

    for filename in workflows {
        let workflow = load_workflow(filename);
        assert_mcp_server(&workflow, "novanet");
    }
}

// ═══════════════════════════════════════════════════════════════
// MCP Transport Path Validation (Phase 2.4 Gap Fix)
// ═══════════════════════════════════════════════════════════════

/// Assert that MCP command is portable (no absolute paths)
fn assert_mcp_path_portable(workflow: &nika::Workflow, filename: &str) {
    let mcp = workflow.mcp.as_ref().expect("No MCP configuration");

    for (server_name, config) in mcp.iter() {
        // Command should not be an absolute path
        assert!(
            !config.command.starts_with('/'),
            "{}: MCP server '{}' uses absolute path '{}' - use relative path or cargo command instead",
            filename,
            server_name,
            config.command
        );

        // Args should not contain absolute paths either
        for arg in &config.args {
            assert!(
                !arg.starts_with("/Users/") && !arg.starts_with("/home/"),
                "{}: MCP server '{}' has absolute path in args: '{}' - use relative path instead",
                filename,
                server_name,
                arg
            );
        }
    }
}

#[test]
fn test_all_use_case_workflows_use_portable_mcp_paths() {
    let workflows = [
        "uc1-generate-page-multilingual.nika.yaml",
        "uc2-seo-content-sprint.nika.yaml",
        "uc3-entity-knowledge-retrieval.nika.yaml",
        "uc4-block-generation-locale-aware.nika.yaml",
        "uc5-semantic-content-planning.nika.yaml",
        "uc6-multi-agent-research.nika.yaml",
        "uc7-quality-gate-pipeline.nika.yaml",
        "uc8-cross-locale-orchestration.nika.yaml",
        "uc9-full-page-pipeline.nika.yaml",
        "uc10-competitive-intelligence.nika.yaml",
    ];

    for filename in workflows {
        let path = Path::new("examples").join(filename);
        if path.exists() {
            let workflow = load_workflow(filename);
            assert_mcp_path_portable(&workflow, filename);
        }
    }
}

#[test]
fn test_invoke_novanet_example_uses_portable_path() {
    let workflow = load_workflow("invoke-novanet.yaml");
    assert_mcp_path_portable(&workflow, "invoke-novanet.yaml");
}

#[test]
fn test_agent_novanet_example_uses_portable_path() {
    let path = Path::new("examples/agent-novanet.yaml");
    if path.exists() {
        let workflow = load_workflow("agent-novanet.yaml");
        assert_mcp_path_portable(&workflow, "agent-novanet.yaml");
    }
}
