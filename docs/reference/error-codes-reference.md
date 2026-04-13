# 07 -- Error Codes Reference

All Nika errors use the `NikaError` enum with `NIKA-XXX` codes. Errors implement both `thiserror::Error` (for std compatibility) and `miette::Diagnostic` (for fancy terminal display with help text and source annotations).

---

## Error Code Ranges

| Range | Category | Source |
|-------|----------|--------|
| 000-009 | Workflow errors | `error.rs` |
| 010-019 | Schema/validation errors | `error.rs` |
| 020-029 | DAG errors | `error.rs` |
| 030-039 | Provider errors | `error.rs` |
| 040-049 | Template/binding errors | `error.rs` |
| 050-059 | Path/task/security errors | `error.rs` |
| 060-069 | Output errors | `error.rs` |
| 070-079 | With block validation errors | `error.rs` |
| 080-089 | DAG validation errors | `error.rs` |
| 090-099 | JSONPath/IO/Execution errors | `error.rs` |
| 100-109 | MCP errors | `error.rs` |
| 110-119 | Agent errors | `error.rs` |
| 120-129 | Resilience errors | `error.rs` |
| 130-139 | TUI/Config errors | `error.rs` |
| 140-151 | AST analysis errors (Phase 2) | `analyzer/errors.rs` |
| 160-164 | Parse errors (Phase 1) | `raw/parser.rs` |
| 165-166 | Policy/Boot errors | `error.rs` |
| 170-179 | Runtime errors | `error.rs` |
| 200-209 | File tool errors | `tools/mod.rs` |
| 210-219 | Builtin tool errors | `error.rs` |
| 250 | Context error | `error.rs` |
| 251-259 | Media pipeline errors | `media/error.rs` |
| 260-269 | Package URI errors | `error.rs` |
| 270-279 | Skill errors | `error.rs` |
| 280-285 | Artifact/media errors | `error.rs` |
| 290-297 | Media tool errors | `media/error.rs` |
| 300-309 | Structured output errors | `error.rs` |
| 310-319 | Course errors | `error.rs` |

---

## Workflow Errors (000-009)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-001 | `ParseError` | Failed to parse workflow YAML | Check YAML syntax: indentation and quoting |
| NIKA-002 | `InvalidSchemaVersion` | Schema version not recognized | Use `nika/workflow@0.12` |
| NIKA-003 | `WorkflowNotFound` | Workflow file does not exist | Check the file path |
| NIKA-004 | `ValidationError` | Workflow structure invalid | Check structure matches schema |
| NIKA-005 | `SchemaValidationFailed` | JSON Schema validation failed | Check against `schemas/nika-workflow.schema.json` |
| NIKA-006 | `HomeDirectoryNotFound` | Cannot determine home directory | Set `NIKA_HOME` environment variable |

## Schema Errors (010-019)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-013 | `SchemaFileNotFound` | Schema file referenced but missing | Ensure schema file exists relative to workflow |
| NIKA-014 | `SchemaFileInvalid` | Schema file contains invalid JSON | Fix JSON syntax in schema file |

## DAG Errors (020-029)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-020 | `CycleDetected` | Circular dependency in task graph | Remove the cyclic dependency |
| NIKA-021 | `MissingDependency` | Task depends on unknown task | Check `depends_on` task IDs |
| NIKA-022 | `DuplicateTaskId` | Same task ID appears twice | Rename one of the duplicate tasks |
| NIKA-026 | `DependencyChainFailed` | Upstream failure blocked tasks | Fix the failing upstream task |
| NIKA-027 | `TaskCancelled` | Task cancelled due to fail_fast | Another task in for_each batch failed |

## Provider Errors (030-039)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-030 | `ProviderNotConfigured` | Provider not set up | Configure the provider |
| NIKA-031 | `ProviderApiError` | LLM API call failed | Check API key and network |
| NIKA-032 | `MissingApiKey` | API key not found | Set environment variable or use `nika keys set` |
| NIKA-033 | `InvalidConfig` | Configuration value invalid | Fix config file |

## Template/Binding Errors (040-049)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-041 | `TemplateError` | Template substitution failed | Check template syntax |
| NIKA-042 | `BindingNotFound` | Referenced binding does not exist | Declare binding in `with:` block |
| NIKA-043 | `BindingTypeMismatch` | Binding type does not match expected | Check data types in binding |

## Path/Task/Security Errors (050-059)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-050 | `InvalidPath` | Path syntax is invalid | Fix path expression |
| NIKA-052 | `PathNotFound` | JSONPath target not found | Task may not have JSON output |
| NIKA-053 | `BlockedCommand` | Command matches blocklist | Use a different command |
| NIKA-055 | `InvalidTaskId` | Task ID contains invalid characters | Use alphanumeric + underscore + hyphen |
| NIKA-056 | `InvalidDefault` | Default value cannot be parsed | Fix default value syntax |

## Output Errors (060-069)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-060 | `InvalidJson` | Output is not valid JSON | Fix JSON output |
| NIKA-061 | `SchemaFailed` | Output does not match JSON schema | Adjust output or schema |
| NIKA-062 | `SerializationError` | Cannot serialize/deserialize | Check data format |

## With Block Validation Errors (070-079)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-071 | `UnknownAlias` | `{{with.alias}}` not in with: block | Declare alias in task's `with:` |
| NIKA-072 | `NullValue` | Path resolves to null (strict mode) | Provide default with `??` |
| NIKA-073 | `InvalidTraversal` | Cannot traverse non-object/array | Check data structure |
| NIKA-074 | `TemplateParse` | Template syntax error at position | Fix template syntax |

## DAG Validation Errors (080-089)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-080 | `WithUnknownTask` | `with:` references unknown task | Check task ID spelling |
| NIKA-081 | `WithNotUpstream` | `with:` references non-upstream task | Add `depends_on` or reorder |
| NIKA-082 | `WithCircularDep` | `with:` creates circular dependency | Restructure data flow |

## JSONPath/IO/Execution Errors (090-099)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-090 | `JsonPathUnsupported` | Complex JSONPath not supported | Use simple paths like `$.a.b` |
| NIKA-093 | `IoError` | File I/O error | Check file permissions |
| NIKA-094 | `JsonError` | JSON parse/serialize error | Fix JSON syntax |
| NIKA-095 | `YamlParse` | YAML parse error | Check indentation and quoting |
| NIKA-096 | `Execution` | General execution error | Check command and environment |

## MCP Errors (100-109)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-100 | `McpNotConnected` | MCP server not connected | Check server is running |
| NIKA-101 | `McpStartError` | MCP server failed to start | Check command and args |
| NIKA-102 | `McpToolError` | MCP tool call failed | Check parameters and server logs |
| NIKA-103 | `McpResourceNotFound` | MCP resource not found | Check resource URI |
| NIKA-104 | `McpProtocolError` | MCP protocol error | Check protocol compatibility |
| NIKA-105 | `McpNotConfigured` | MCP server not in workflow | Add to `mcp:` block |
| NIKA-106 | `McpInvalidResponse` | MCP response format invalid | Check server implementation |
| NIKA-107 | `McpValidationFailed` | MCP parameter validation failed | Fix tool parameters |
| NIKA-108 | `McpSchemaError` | MCP tool schema error | Check tool schema |
| NIKA-109 | `McpTimeout` | MCP operation timed out | Increase timeout or check server |

## Agent Errors (110-119)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-112 | `GuardrailViolation` | Agent output violates guardrails | Adjust guardrail rules or agent prompt |
| NIKA-113 | `AgentValidationError` | Agent params invalid (empty prompt, bad max_turns) | Fix agent parameters |
| NIKA-115 | `AgentExecutionError` | Agent execution failed | Check provider and tools |
| NIKA-116 | `ThinkingCaptureFailed` | Extended thinking capture error | Check Claude API |

## Resilience Errors (120-129)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-121 | `Timeout` | Operation timed out | Increase timeout |
| NIKA-125 | `McpToolCallFailed` | MCP tool call failed after retries | Check server and retry config |

## TUI/Config Errors (130-139)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-130 | `TuiError` | TUI rendering/interaction error | Report bug |
| NIKA-135 | `ConfigError` | Configuration error | Check config file syntax |

## AST Analysis Errors (140-151)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-140 | `UnknownTask` | Referenced task does not exist | Check spelling (suggestion may be offered) |
| NIKA-141 | `DuplicateTask` | Task ID already defined | Use unique task IDs |
| NIKA-142 | `InvalidSchemaVersion` | Unknown schema version | Use `nika/workflow@0.12` |
| NIKA-143 | `CycleDetected` | Circular dependency detected | Restructure dependencies |
| NIKA-144 | `InvalidWithEntry` | Malformed with: entry | Check binding syntax |
| NIKA-145 | `EmptyWorkflow` | No tasks defined | Add at least one task |
| NIKA-146 | `InvalidVerb` | Unknown verb | Use infer/exec/fetch/invoke/agent |
| NIKA-147 | `MissingAction` | Task has no verb | Add a verb to the task |
| NIKA-148 | `InvalidField` | Unknown field | Check field name |
| NIKA-149 | `InvalidValue` | Wrong value type | Fix value |
| NIKA-150 | `DependsOnSelf` | Task depends on itself | Remove self-reference |
| NIKA-151 | `TransformParseError` | Invalid transform pipe | Check pipe syntax |

## Parse Errors (160-164)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-160 | `Syntax` | Invalid YAML syntax | Fix indentation/quoting |
| NIKA-161 | `MissingField` | Required field missing | Add required field |

## Policy/Boot Errors (165-166)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-165 | `PolicyViolation` | Security policy violated | Check `.nika/config.toml [policy]` |
| NIKA-166 | `BootFailed` | Boot sequence failed | Run `nika doctor` |

## Runtime Errors (170-179)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-171 | `DecomposeTimeout` | Decompose expansion timed out | Reduce max_depth/max_items |

## File Tool Errors (200-209)

Defined in `src/tools/mod.rs` via `ToolErrorCode`:

| Code | Description |
|------|-------------|
| NIKA-200 | File read error |
| NIKA-201 | File write error |
| NIKA-202 | File edit error |
| NIKA-203 | Must read file before editing |
| NIKA-204 | Path outside working directory |
| NIKA-205 | Permission denied |
| NIKA-206 | Invalid glob pattern |
| NIKA-207 | Invalid regex pattern |
| NIKA-208 | File not found |
| NIKA-209 | old_string not unique in file |

## Builtin Tool Errors (210-219)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-210 | `BuiltinToolError` | Builtin tool execution error | Check tool params |
| NIKA-212 | `BuiltinInvalidParams` | Invalid parameters | Check JSON schema |
| NIKA-213 | `AssertionFailed` | nika:assert condition false | Fix assertion condition |
| NIKA-215 | `FileAlreadyExists` | nika:write target exists | Use nika:edit instead |

## Context Error (250)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-250 | `ContextLoadError` | Context file loading failed | Check file path and permissions |

## Media Pipeline Errors (251-259)

| Code | Description |
|------|-------------|
| NIKA-251 | Invalid MIME type |
| NIKA-252 | CAS store error |
| NIKA-253 | Base64 decode error |
| NIKA-254 | Media budget exceeded |
| NIKA-255 | Media processing error |
| NIKA-256 | Media format unsupported |

## Package URI Errors (260-269)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-260 | `InvalidPkgUri` | Invalid `pkg:` URI format | Use `pkg:@scope/name@version/path` |
| NIKA-261 | `PackageNotFound` | Package not in registry | Install with `nika pkg install` |

## Skill Errors (270-279)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-270 | `SkillLoadError` | Skill file loading failed | Check file exists and is readable |

## Artifact/Media Errors (280-285)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-280 | `ArtifactPathError` | Path validation failed | No path traversal patterns |
| NIKA-281 | `ArtifactWriteError` | Write failed | Check permissions and disk space |
| NIKA-282 | `ArtifactSizeExceeded` | File too large | Increase `artifacts.max_size` |
| NIKA-283 | `MediaIntegrityWarning` | CAS file corrupted/deleted | Re-run workflow |
| NIKA-284 | `MediaCleanupError` | GC failed | Check permissions |
| NIKA-285 | `MediaStoreLocked` | Store is locked by running workflow | Wait or use `--force` |

## Media Tool Errors (290-297)

| Code | Description |
|------|-------------|
| NIKA-290 | Media tool error |
| NIKA-291 | Format not supported |
| NIKA-292 | Missing dependency (feature not enabled) |
| NIKA-293 | Operation timed out |
| NIKA-294 | Invalid arguments |
| NIKA-295 | Pipeline error |
| NIKA-296 | Pipeline chain error |
| NIKA-297 | Security violation (unsafe operation) |

## Structured Output Errors (300-309)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-300 | `StructuredOutputExtractionFailed` | Cannot extract JSON from LLM response | Check response format |
| NIKA-301 | `StructuredOutputValidationFailed` | JSON does not match schema | Fix schema or prompt |
| NIKA-302 | `StructuredOutputRepairFailed` | LLM could not repair output | Simplify schema |
| NIKA-303 | `StructuredOutputAllLayersFailed` | All 5 layers failed | Check schema and prompt |

## Course Errors (310-319)

| Code | Variant | Description | Help |
|------|---------|-------------|------|
| NIKA-310 | `CourseNotFound` | Course directory missing | Run `nika init --course` |
| NIKA-311 | `CourseCheckFailed` | Exercise validation failed | Review instructions |
| NIKA-312 | `CourseLevelLocked` | Level prerequisite not met | Complete previous level |
| NIKA-313 | `CourseProgressCorrupted` | Progress file damaged | Delete and restart |
| NIKA-314 | `CourseWatchError` | File watch error | Check permissions |

---

## FixSuggestion Trait

```rust
pub trait FixSuggestion {
    fn fix_suggestion(&self) -> Option<&str>;
}
```

Some error variants implement this trait to provide actionable fix suggestions beyond the `miette` help text.

---

## Error Display

Errors support two display modes:

1. **Standard**: `[NIKA-XXX] Description` via `thiserror::Error`
2. **Fancy**: Colored terminal output with source annotations via `miette::Diagnostic`

The fancy mode shows:
- Error code with hyperlink to docs
- Source code snippet with underlined error location
- Help text with suggested fix
- Related errors (for multi-error collection)
