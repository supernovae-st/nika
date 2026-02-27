//! Agent Sniper Tests - Comprehensive AgentParams coverage.
//!
//! Tests every field of AgentParams:
//! - prompt, system, provider, model
//! - mcp, max_turns, token_budget
//! - stop_conditions, scope
//! - extended_thinking, thinking_budget
//! - depth_limit

use nika::serde_yaml;
use nika::ast::{TaskAction, Workflow};
use nika::dag::Dag;
use std::path::PathBuf;

/// Get path to examples directory
fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

/// Parse a workflow file and return it
fn parse_workflow(filename: &str) -> Workflow {
    let path = examples_dir().join(filename);
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_yaml::from_str(&content).unwrap_or_else(|e| panic!("Failed to parse {}: {}", filename, e))
}

// ============================================================================
// AGENT PARAMS FIELD TESTS
// ============================================================================

#[test]
fn test_agent_minimal_only_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: minimal
    agent:
      prompt: "Say hello"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.prompt, "Say hello");
        assert!(agent.system.is_none());
        assert!(agent.provider.is_none());
        assert!(agent.model.is_none());
        assert!(agent.mcp.is_empty());
        assert!(agent.max_turns.is_none());
        assert!(agent.token_budget.is_none());
        assert!(agent.stop_conditions.is_empty());
        assert!(agent.scope.is_none());
        assert!(agent.extended_thinking.is_none());
        assert!(agent.thinking_budget.is_none());
        assert!(agent.depth_limit.is_none());
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_system_prompt() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: with_system
    agent:
      prompt: "What is your role?"
      system: "You are a QR code expert. Always respond in JSON."
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.prompt, "What is your role?");
        assert_eq!(
            agent.system.as_deref(),
            Some("You are a QR code expert. Always respond in JSON.")
        );
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_explicit_provider_and_model() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: explicit
    agent:
      prompt: "Generate text"
      provider: openai
      model: gpt-4o
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.provider.as_deref(), Some("openai"));
        assert_eq!(agent.model.as_deref(), Some("gpt-4o"));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_single_mcp() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

mcp:
  novanet:
    command: "echo"

tasks:
  - id: with_mcp
    agent:
      prompt: "Use novanet tools"
      mcp: ["novanet"]
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.mcp, vec!["novanet".to_string()]);
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_multiple_mcp() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

mcp:
  novanet:
    command: "echo"
  perplexity:
    command: "echo"
  firecrawl:
    command: "echo"

tasks:
  - id: multi_mcp
    agent:
      prompt: "Use all tools"
      mcp: ["novanet", "perplexity", "firecrawl"]
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.mcp.len(), 3);
        assert!(agent.mcp.contains(&"novanet".to_string()));
        assert!(agent.mcp.contains(&"perplexity".to_string()));
        assert!(agent.mcp.contains(&"firecrawl".to_string()));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_max_turns() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: limited
    agent:
      prompt: "Count to 100"
      max_turns: 5
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.max_turns, Some(5));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_token_budget() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: budgeted
    agent:
      prompt: "Write an essay"
      token_budget: 1000
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.token_budget, Some(1000));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_stop_conditions() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: stoppable
    agent:
      prompt: "Generate until done"
      stop_conditions:
        - "DONE"
        - "COMPLETE"
        - "FINISHED"
        - "END"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.stop_conditions.len(), 4);
        assert!(agent.stop_conditions.contains(&"DONE".to_string()));
        assert!(agent.stop_conditions.contains(&"COMPLETE".to_string()));
        assert!(agent.stop_conditions.contains(&"FINISHED".to_string()));
        assert!(agent.stop_conditions.contains(&"END".to_string()));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_scope() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: scoped
    agent:
      prompt: "Generate for scope"
      scope: "qrcode-ai:fr-FR:landing-page"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.scope.as_deref(), Some("qrcode-ai:fr-FR:landing-page"));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_extended_thinking() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: thinking
    agent:
      prompt: "Solve this puzzle step by step"
      extended_thinking: true
      thinking_budget: 5000
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.extended_thinking, Some(true));
        assert_eq!(agent.thinking_budget, Some(5000));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_depth_limit() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: nested
    agent:
      prompt: "Spawn child agents"
      depth_limit: 3
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        assert_eq!(agent.depth_limit, Some(3));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_all_fields() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

mcp:
  novanet:
    command: "echo"
  perplexity:
    command: "echo"

tasks:
  - id: full
    agent:
      prompt: "Complete research task"
      system: "You are a senior researcher"
      provider: claude
      model: claude-sonnet-4-6
      mcp: ["novanet", "perplexity"]
      max_turns: 10
      token_budget: 5000
      stop_conditions: ["DONE", "ABORT"]
      scope: "research:comprehensive"
      extended_thinking: true
      thinking_budget: 3000
      depth_limit: 2
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
        // All fields should be populated
        assert_eq!(agent.prompt, "Complete research task");
        assert_eq!(agent.system.as_deref(), Some("You are a senior researcher"));
        assert_eq!(agent.provider.as_deref(), Some("claude"));
        assert_eq!(agent.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(
            agent.mcp,
            vec!["novanet".to_string(), "perplexity".to_string()]
        );
        assert_eq!(agent.max_turns, Some(10));
        assert_eq!(agent.token_budget, Some(5000));
        assert_eq!(
            agent.stop_conditions,
            vec!["DONE".to_string(), "ABORT".to_string()]
        );
        assert_eq!(agent.scope.as_deref(), Some("research:comprehensive"));
        assert_eq!(agent.extended_thinking, Some(true));
        assert_eq!(agent.thinking_budget, Some(3000));
        assert_eq!(agent.depth_limit, Some(2));
    } else {
        panic!("Expected agent task");
    }
}

// ============================================================================
// AGENT WITH BINDINGS TESTS
// ============================================================================

#[test]
fn test_agent_prompt_with_mustache_binding() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: setup
    exec: "echo 'context'"

  - id: agent_bind
    use:
      ctx: setup
    agent:
      prompt: "Use this context: {{use.ctx}}"

flows:
  - source: setup
    target: agent_bind
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[1].action {
        assert!(agent.prompt.contains("{{use.ctx}}"));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_prompt_with_dollar_binding() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: setup
    exec: "echo 'context'"

  - id: agent_bind
    use:
      ctx: setup
    agent:
      prompt: "Use this context: $ctx"

flows:
  - source: setup
    target: agent_bind
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[1].action {
        assert!(agent.prompt.contains("$ctx"));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_mixed_binding_syntaxes() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: setup
    exec: "echo '{\"a\": 1, \"b\": 2}'"
    output:
      format: json

  - id: agent_mixed
    use:
      data: setup
    agent:
      prompt: |
        Process this data:
        Mustache: {{use.data}}
        Dollar: $data
        Both should work!

flows:
  - source: setup
    target: agent_mixed
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");

    if let TaskAction::Agent { agent } = &workflow.tasks[1].action {
        assert!(agent.prompt.contains("{{use.data}}"));
        assert!(agent.prompt.contains("$data"));
    } else {
        panic!("Expected agent task");
    }
}

// ============================================================================
// AGENT WITH FOR_EACH TESTS
// ============================================================================

#[test]
fn test_agent_with_for_each_array() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: parallel_agents
    for_each: ["fr-FR", "en-US", "de-DE", "es-ES", "ja-JP"]
    as: locale
    concurrency: 5
    agent:
      prompt: "Generate content for locale {{use.locale}}"
      max_turns: 1
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let task = &workflow.tasks[0];

    assert!(task.for_each.is_some());
    assert_eq!(task.for_each_as.as_deref(), Some("locale"));
    assert_eq!(task.concurrency, Some(5));

    if let TaskAction::Agent { agent } = &task.action {
        assert!(agent.prompt.contains("{{use.locale}}"));
        assert_eq!(agent.max_turns, Some(1));
    } else {
        panic!("Expected agent task");
    }
}

#[test]
fn test_agent_with_for_each_binding() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: get_locales
    exec: "echo '[\"fr-FR\", \"en-US\"]'"
    output:
      format: json

  - id: parallel_agents
    use:
      locales: get_locales
    for_each: "{{use.locales}}"
    as: loc
    agent:
      prompt: "Generate for {{use.loc}}"

flows:
  - source: get_locales
    target: parallel_agents
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    let task = &workflow.tasks[1];

    // for_each can be a binding expression
    assert!(task.for_each.is_some());
    assert_eq!(task.for_each_as.as_deref(), Some("loc"));
}

// ============================================================================
// AGENT CHAIN TESTS
// ============================================================================

#[test]
fn test_agent_chain_research_write_review() {
    let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: research
    agent:
      prompt: "Research QR codes"
      max_turns: 3

  - id: write
    use:
      research: research
    agent:
      prompt: "Write article based on: {{use.research}}"
      max_turns: 2

  - id: review
    use:
      article: write
    agent:
      prompt: "Review article: $article"
      max_turns: 2

flows:
  - source: research
    target: write
  - source: write
    target: review
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Should parse");
    assert_eq!(workflow.tasks.len(), 3);
    assert_eq!(workflow.flows.len(), 2);

    // Verify DAG
    let graph = Dag::from_workflow(&workflow);
    assert!(
        graph.detect_cycles().is_ok(),
        "Agent chain should have no cycles"
    );

    // Verify chain order
    let deps_write = graph.get_dependencies("write");
    assert!(deps_write.iter().any(|d| d.as_ref() == "research"));

    let deps_review = graph.get_dependencies("review");
    assert!(deps_review.iter().any(|d| d.as_ref() == "write"));
}

// ============================================================================
// PRODUCTION WORKFLOW TESTS
// ============================================================================

#[test]
fn test_parse_agent_sniper_complete() {
    let workflow = parse_workflow("test-agent-sniper-complete.nika.yaml");

    // Should have many tasks
    assert!(workflow.tasks.len() >= 10, "Should have many agent tasks");

    // Should have MCP config
    assert!(workflow.mcp.is_some());
    let mcp = workflow.mcp.as_ref().unwrap();
    assert!(mcp.contains_key("novanet"));

    // Count agent tasks
    let agent_count = workflow
        .tasks
        .iter()
        .filter(|t| matches!(&t.action, TaskAction::Agent { .. }))
        .count();
    assert!(agent_count >= 10, "Should have at least 10 agent tasks");
}

#[test]
fn test_agent_sniper_dag_valid() {
    let workflow = parse_workflow("test-agent-sniper-complete.nika.yaml");

    let graph = Dag::from_workflow(&workflow);

    // No cycles allowed
    assert!(
        graph.detect_cycles().is_ok(),
        "Agent sniper workflow should have no cycles"
    );

    // Should have final tasks
    let final_tasks = graph.get_final_tasks();
    assert!(
        !final_tasks.is_empty(),
        "Should have final aggregation task"
    );
}

// ============================================================================
// PROVIDER VARIATIONS
// ============================================================================

#[test]
fn test_agent_all_providers() {
    let providers = ["claude", "openai", "mistral", "groq", "deepseek", "ollama"];

    for provider in providers {
        let yaml = format!(
            r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: agent_{provider}
    agent:
      prompt: "Test {provider}"
      provider: {provider}
"#,
            provider = provider
        );

        let workflow: Workflow = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("Should parse provider {}: {}", provider, e));

        if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
            assert_eq!(agent.provider.as_deref(), Some(provider));
        } else {
            panic!("Expected agent task for provider {}", provider);
        }
    }
}

#[test]
fn test_agent_model_variations() {
    let models = [
        ("claude", "claude-sonnet-4-6"),
        ("claude", "claude-opus-4-5-20251101"),
        ("openai", "gpt-4o"),
        ("openai", "gpt-4o-mini"),
        ("mistral", "mistral-large-latest"),
        ("groq", "llama-3.3-70b-versatile"),
        ("deepseek", "deepseek-chat"),
    ];

    for (provider, model) in models {
        let yaml = format!(
            r#"
schema: "nika/workflow@0.5"
provider: claude

tasks:
  - id: agent_test
    agent:
      prompt: "Test model"
      provider: {provider}
      model: {model}
"#,
            provider = provider,
            model = model
        );

        let workflow: Workflow = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("Should parse {}/{}: {}", provider, model, e));

        if let TaskAction::Agent { agent } = &workflow.tasks[0].action {
            assert_eq!(agent.provider.as_deref(), Some(provider));
            assert_eq!(agent.model.as_deref(), Some(model));
        } else {
            panic!("Expected agent task");
        }
    }
}
