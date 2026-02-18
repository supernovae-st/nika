# Error Handling Rules for Nika

## Use NikaError, Not anyhow

```rust
// GOOD
fn parse_workflow(yaml: &str) -> Result<Workflow, NikaError> {
    serde_yaml::from_str(yaml)
        .map_err(|e| NikaError::ParseError {
            source: e.to_string(),
            line: e.location().map(|l| l.line())
        })
}

// BAD
fn parse_workflow(yaml: &str) -> anyhow::Result<Workflow> {
    Ok(serde_yaml::from_str(yaml)?)
}
```

## Error Code Assignment

Each error variant MUST have a unique code:

```rust
#[derive(Debug, thiserror::Error)]
pub enum NikaError {
    #[error("[NIKA-001] Failed to parse workflow: {source}")]
    ParseError { source: String, line: Option<usize> },

    #[error("[NIKA-100] MCP server '{name}' not connected")]
    McpNotConnected { name: String },
}
```

## Error Context

Always provide actionable context:

```rust
// GOOD
NikaError::TaskFailed {
    task_id: "generate".to_string(),
    reason: "Provider returned empty response".to_string(),
    suggestion: "Check API key and model availability".to_string(),
}

// BAD
NikaError::TaskFailed("generate failed".to_string())
```
