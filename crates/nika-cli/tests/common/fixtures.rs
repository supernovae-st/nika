//! Test fixtures and helpers

use std::path::PathBuf;

/// Get path to test fixtures directory
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Get path to a specific fixture file
pub fn fixture(name: &str) -> PathBuf {
    fixtures_dir().join(name)
}

/// Create a minimal valid workflow YAML
pub fn minimal_workflow_yaml() -> &'static str {
    r#"
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: hello
    infer:
      prompt: "Say hello"
"#
}

/// Create a workflow with invoke verb (v0.2)
pub fn invoke_workflow_yaml() -> &'static str {
    r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: "echo"
    args: ["mock"]

tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        mode: block
        entity: qr-code
        locale: fr-FR
"#
}

/// Create a workflow with agent verb (v0.2)
pub fn agent_workflow_yaml() -> &'static str {
    r#"
schema: "nika/workflow@0.2"
provider: claude

mcp:
  novanet:
    command: "echo"
    args: ["mock"]

tasks:
  - id: generate
    agent:
      prompt: "Generate content using novanet context"
      mcp:
        - novanet
      max_turns: 5
      stop_conditions:
        - "GENERATION_COMPLETE"
"#
}
