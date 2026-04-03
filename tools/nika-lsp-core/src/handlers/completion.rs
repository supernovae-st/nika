//! Completion handler for `.nika.yaml` workflow files.
//!
//! Pure, synchronous completion logic. No async, no server state -- just
//! `(text, offset, context) -> Vec<CompletionItem>`.
//!
//! Ported from the embedded LSP (`nika/src/lsp/handlers/completion.rs`),
//! extended with content/vision, provider/model catalogs, and depends_on
//! completions.

use ls_types::{CompletionItem, CompletionItemKind, Documentation, InsertTextFormat};
use nika_core::catalogs::models::{ModelType, KNOWN_MODELS};
use nika_core::catalogs::providers::{ProviderCategory, KNOWN_PROVIDERS};
use nika_core::catalogs::ProviderStatusInfo;

use crate::analysis::context::{extract_task_ids, ContentFocus, CursorContext, InvokeFocus};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute completions for the given cursor context.
///
/// Pure function -- no async, no state beyond the arguments.
/// `daemon_providers` is optionally passed from the daemon bridge for live key status.
pub fn completions(
    text: &str,
    _offset: u32,
    context: &CursorContext,
    daemon_providers: Option<&[ProviderStatusInfo]>,
) -> Vec<CompletionItem> {
    match context {
        CursorContext::WorkflowRoot { prefix } => workflow_root_completions(prefix),
        CursorContext::TaskField {
            prefix,
            existing_fields,
            ..
        } => task_field_completions(prefix, existing_fields),
        CursorContext::VerbBlock { verb, prefix, .. } => verb_block_completions(verb, prefix),
        CursorContext::WithBlock { .. } => with_block_completions(text),
        CursorContext::Template {
            partial_expr,
            in_transform_chain,
            ..
        } => template_completions(text, partial_expr, *in_transform_chain),
        CursorContext::InvokeBlock { focus, prefix, .. } => invoke_block_completions(focus, prefix),
        CursorContext::McpConfig { prefix, .. } => mcp_config_completions(prefix),
        CursorContext::ProviderContext {
            prefix,
            current_provider,
            ..
        } => provider_completions(prefix, current_provider.as_deref(), daemon_providers),
        CursorContext::ContentPart { focus, prefix, .. } => content_part_completions(focus, prefix),
        CursorContext::ForEach { prefix, .. } => for_each_completions(prefix),
        CursorContext::SchemaBlock { prefix, .. } => schema_block_completions(prefix),
        CursorContext::DependsOn { existing_deps, .. } => {
            depends_on_completions(text, existing_deps)
        }
        CursorContext::Guardrails { prefix, .. } => guardrails_completions(prefix),
        CursorContext::RetryBlock { prefix, .. } => retry_block_completions(prefix),
        CursorContext::LimitsBlock { prefix, .. } => limits_block_completions(prefix),
        CursorContext::Unknown { .. } => vec![],
    }
}

// ---------------------------------------------------------------------------
// Completion providers
// ---------------------------------------------------------------------------

/// Top-level workflow keys: schema, workflow, tasks, mcp, context, inputs, include, edges.
fn workflow_root_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet(
            "schema",
            CompletionItemKind::KEYWORD,
            "schema: nika/workflow@0.12",
            "Required. Schema version for this workflow.",
            "0_schema",
        ),
        item_snippet_fmt(
            "workflow",
            CompletionItemKind::KEYWORD,
            "workflow: ${1:workflow-name}",
            "Optional. Workflow name/identifier.",
            "1_workflow",
        ),
        item_snippet_fmt(
            "tasks",
            CompletionItemKind::KEYWORD,
            "tasks:\n  - id: ${1:task-id}\n    ${2:infer}: ${3:prompt}",
            "Required. List of tasks to execute.",
            "2_tasks",
        ),
        item_snippet_fmt(
            "mcp",
            CompletionItemKind::KEYWORD,
            "mcp:\n  ${1:server-name}:\n    command: ${2:command}\n    args: [${3}]",
            "Optional. MCP server configurations.",
            "3_mcp",
        ),
        item_snippet_fmt(
            "context",
            CompletionItemKind::KEYWORD,
            "context:\n  files:\n    ${1:alias}: ${2:./path/to/file}",
            "Optional. Load files at workflow start.",
            "4_context",
        ),
        item_snippet_fmt(
            "inputs",
            CompletionItemKind::KEYWORD,
            "inputs:\n  ${1:name}:\n    type: ${2:string}\n    description: ${3:description}",
            "Optional. Workflow input parameters.",
            "5_inputs",
        ),
        item_snippet_fmt(
            "include",
            CompletionItemKind::KEYWORD,
            "include:\n  - path: ${1:./partial.nika.yaml}\n    prefix: ${2:partial_}",
            "Optional. Include tasks from other workflows.",
            "6_include",
        ),
        item_snippet_fmt(
            "edges",
            CompletionItemKind::KEYWORD,
            "edges:\n  - from: ${1:task-a}\n    to: ${2:task-b}",
            "Optional. Explicit dependency edges.",
            "7_edges",
        ),
        item_snippet_fmt(
            "provider",
            CompletionItemKind::KEYWORD,
            "provider: ${1|anthropic,openai,mistral,groq,deepseek,gemini,xai,native|}",
            "Optional. Default LLM provider.",
            "80_provider",
        ),
        item_snippet_fmt(
            "model",
            CompletionItemKind::KEYWORD,
            "model: ${1:claude-sonnet-4-6}",
            "Required for LLM tasks. Model for infer/agent verbs.",
            "81_model",
        ),
        item_snippet_fmt(
            "description",
            CompletionItemKind::KEYWORD,
            "description: ${1:Workflow description}",
            "Optional. Human-readable description.",
            "82_description",
        ),
        item_snippet_fmt(
            "skills",
            CompletionItemKind::KEYWORD,
            "skills:\n  ${1:alias}: ${2:./skills/skill.md}",
            "Optional. Skill definitions for agent injection.",
            "83_skills",
        ),
        item_snippet_fmt(
            "agents",
            CompletionItemKind::KEYWORD,
            "agents:\n  ${1:agent-name}:\n    system: ${2:persona}\n    tools: [${3}]",
            "Optional. Reusable agent definitions.",
            "84_agents",
        ),
        item_snippet_fmt(
            "artifacts",
            CompletionItemKind::KEYWORD,
            "artifacts:\n  ${1:default}:\n    path: ${2:./output}",
            "Optional. Artifact output defaults.",
            "85_artifacts",
        ),
        item_snippet_fmt(
            "log",
            CompletionItemKind::KEYWORD,
            "log:\n  level: ${1|info,debug,warn,error|}\n  format: ${2|json,text|}",
            "Optional. Logging configuration.",
            "86_log",
        ),
    ];
    filter_by_prefix(items, prefix)
}

/// Task-level fields: id, verbs, with, depends_on, content, for_each, retry,
/// limits, guardrails, provider, model, etc.
fn task_field_completions(prefix: &str, existing_fields: &[String]) -> Vec<CompletionItem> {
    let mut items = vec![
        item_snippet_fmt(
            "id",
            CompletionItemKind::PROPERTY,
            "id: ${1:task-id}",
            "Required. Unique task identifier.",
            "0_id",
        ),
        // The 5 verbs -- multi-line scaffolds with tab stops
        item_snippet_fmt(
            "infer",
            CompletionItemKind::KEYWORD,
            "infer:\n  prompt: ${1:your prompt here}\n  ${0}",
            "LLM text generation.",
            "1_infer",
        ),
        item_snippet_fmt(
            "exec",
            CompletionItemKind::KEYWORD,
            "exec: ${1:command}\n${0}",
            "Shell command.",
            "1_exec",
        ),
        item_snippet_fmt(
            "fetch",
            CompletionItemKind::KEYWORD,
            "fetch:\n  url: ${1:https://}\n  ${0}",
            "HTTP request.",
            "1_fetch",
        ),
        item_snippet_fmt(
            "invoke",
            CompletionItemKind::KEYWORD,
            "invoke:\n  tool: ${1:nika:tool}\n  params:\n    ${2:key}: ${3:value}\n${0}",
            "MCP tool invocation.",
            "1_invoke",
        ),
        item_snippet_fmt(
            "agent",
            CompletionItemKind::KEYWORD,
            "agent:\n  prompt: ${1:agent goal}\n  mcp: [${2}]\n  max_turns: ${3:10}\n${0}",
            "Multi-turn agentic loop.",
            "1_agent",
        ),
        // Task meta-fields
        item_snippet_fmt(
            "with",
            CompletionItemKind::PROPERTY,
            "with:\n  ${1:alias}: ${2:task-id}",
            "Bind outputs from previous tasks.",
            "2_with",
        ),
        item_snippet_fmt(
            "depends_on",
            CompletionItemKind::PROPERTY,
            "depends_on: [${1}]",
            "Task IDs that must complete before this task runs.",
            "2_depends_on",
        ),
        item_snippet_fmt(
            "content",
            CompletionItemKind::PROPERTY,
            "content:\n  - type: ${1|text,image,image_url|}\n    ${2:text}: ${3:value}",
            "Multimodal content parts (vision support).",
            "2_content",
        ),
        item_snippet_fmt(
            "for_each",
            CompletionItemKind::PROPERTY,
            "for_each: [${1}]\nas: ${2:item}\nconcurrency: ${3:3}",
            "Parallel iteration over an array.",
            "3_for_each",
        ),
        item_snippet_fmt(
            "retry",
            CompletionItemKind::PROPERTY,
            "retry:\n  max_attempts: ${1:3}\n  delay: ${2:1s}",
            "Retry configuration for failed tasks.",
            "3_retry",
        ),
        item_snippet_fmt(
            "timeout",
            CompletionItemKind::PROPERTY,
            "timeout: ${1:30}",
            "Maximum execution time (seconds).",
            "3_timeout",
        ),
        item_snippet_fmt(
            "provider",
            CompletionItemKind::PROPERTY,
            "provider: ${1|claude,openai,mistral,groq,deepseek,gemini,xai|}",
            "Override LLM provider for this task.",
            "4_provider",
        ),
        item_snippet_fmt(
            "model",
            CompletionItemKind::PROPERTY,
            "model: ${1}",
            "Override model for this task.",
            "4_model",
        ),
        item_snippet_fmt(
            "structured",
            CompletionItemKind::PROPERTY,
            "structured:\n  schema:\n    type: object\n    properties:\n      ${1:field}:\n        type: ${2:string}\n    required: [${1:field}]",
            "Enforce JSON schema on task output.",
            "4_structured",
        ),
        item_snippet_fmt(
            "description",
            CompletionItemKind::PROPERTY,
            "description: \"${1}\"",
            "Human-readable task description.",
            "5_description",
        ),
        item_snippet_fmt(
            "artifact",
            CompletionItemKind::PROPERTY,
            "artifact:\n  path: ${1:output.txt}\n  format: ${2|text,json|}",
            "Persist task output to a file.",
            "5_artifact",
        ),
        item_snippet_fmt(
            "log",
            CompletionItemKind::PROPERTY,
            "log:\n  level: ${1|debug,info,warn|}",
            "Task-level log configuration override.",
            "5_log",
        ),
    ];

    // Filter out fields that already exist in the task.
    items.retain(|item| !existing_fields.iter().any(|f| f == &item.label));

    filter_by_prefix(items, prefix)
}

/// Verb-specific sub-fields (model, provider, prompt, system, etc.).
fn verb_block_completions(verb: &str, prefix: &str) -> Vec<CompletionItem> {
    let items = match verb {
        "infer" => vec![
            item_snippet_fmt("prompt", CompletionItemKind::PROPERTY, "prompt: ${1}", "Text prompt (supports {{with.alias}} templates).", "0_prompt"),
            item_snippet_fmt("system", CompletionItemKind::PROPERTY, "system: ${1:You are a helpful assistant.}", "System prompt (supports {{with.alias}} templates).", "1_system"),
            item_snippet_fmt("model", CompletionItemKind::PROPERTY, "model: ${1:claude-sonnet-4-6}", "Model override.", "2_model"),
            item_snippet_fmt("provider", CompletionItemKind::PROPERTY, "provider: ${1|claude,openai,mistral,groq,deepseek,gemini,xai|}", "Provider override.", "2_provider"),
            item_snippet_fmt("temperature", CompletionItemKind::PROPERTY, "temperature: ${1:0.7}", "Sampling temperature (0.0-2.0).", "3_temperature"),
            item_snippet_fmt("max_tokens", CompletionItemKind::PROPERTY, "max_tokens: ${1:1000}", "Maximum output tokens.", "3_max_tokens"),
            item_snippet_fmt("response_format", CompletionItemKind::PROPERTY, "response_format: ${1|json,text,markdown|}", "Response format hint.", "3_response_format"),
            item_snippet_fmt("extended_thinking", CompletionItemKind::PROPERTY, "extended_thinking: true\nthinking_budget: ${1:8192}", "Enable extended thinking (Claude).", "4_thinking"),
            item_snippet_fmt("thinking_budget", CompletionItemKind::PROPERTY, "thinking_budget: ${1:8192}", "Token budget for extended thinking.", "4_thinking_budget"),
            item_snippet_fmt("content", CompletionItemKind::PROPERTY, "content:\n  - type: ${1|text,image,image_url|}\n    ${2:text}: ${3:value}", "Multimodal vision content.", "5_content"),
            item_snippet_fmt("guardrails", CompletionItemKind::PROPERTY, "guardrails:\n  - type: ${1|length,schema,regex,llm|}\n    ${2:max_words}: ${3:500}", "Output guardrails (length, schema, regex, llm).", "6_guardrails"),
        ],
        "exec" => vec![
            item_snippet_fmt("command", CompletionItemKind::PROPERTY, "command: ${1}", "Shell command to run.", "0_command"),
            item_snippet_fmt("shell", CompletionItemKind::PROPERTY, "shell: ${1|true,false|}", "Enable shell mode. Default: false (secure shlex).", "1_shell"),
            item_snippet_fmt("timeout", CompletionItemKind::PROPERTY, "timeout: ${1:30}", "Timeout in seconds.", "2_timeout"),
            item_snippet_fmt("cwd", CompletionItemKind::PROPERTY, "cwd: ${1:.}", "Working directory.", "2_cwd"),
            item_snippet_fmt("env", CompletionItemKind::PROPERTY, "env:\n  ${1:KEY}: ${2:value}", "Environment variables.", "3_env"),
        ],
        "fetch" => vec![
            item_snippet_fmt("url", CompletionItemKind::PROPERTY, "url: ${1:https://}", "Required. Request URL.", "0_url"),
            item_snippet_fmt("method", CompletionItemKind::PROPERTY, "method: ${1|GET,POST,PUT,DELETE,PATCH,HEAD,OPTIONS|}", "HTTP method. Default: GET.", "1_method"),
            item_snippet_fmt("headers", CompletionItemKind::PROPERTY, "headers:\n  ${1:Content-Type}: ${2:application/json}", "HTTP request headers.", "2_headers"),
            item_snippet_fmt("body", CompletionItemKind::PROPERTY, "body: ${1}", "Request body (string).", "3_body"),
            item_snippet_fmt("json", CompletionItemKind::PROPERTY, "json:\n  ${1:key}: ${2:value}", "JSON request body (auto-serialized).", "3_json"),
            item_snippet_fmt("extract", CompletionItemKind::PROPERTY, &format!("extract: ${{1|{}|}}", nika_core::ast::extract::ExtractMode::ALL_NAMES.join(",")), "Post-processing extraction mode (9 modes).", "4_extract"),
            item_snippet_fmt("selector", CompletionItemKind::PROPERTY, "selector: ${1}", "CSS selector (for extract: text/selector) or JSONPath (for extract: jsonpath).", "4_selector"),
            item_snippet_fmt("response", CompletionItemKind::PROPERTY, &format!("response: ${{1|{}|}}", nika_core::ast::extract::ResponseMode::ALL_NAMES.join(",")), "Response mode: full (status+headers+body JSON) or binary (CAS storage).", "4_response"),
            item_snippet_fmt("timeout", CompletionItemKind::PROPERTY, "timeout: ${1:30}", "Timeout in seconds.", "5_timeout"),
            item_snippet_fmt("follow_redirects", CompletionItemKind::PROPERTY, "follow_redirects: ${1|true,false|}", "Follow HTTP redirects. Default: true.", "5_follow"),
        ],
        "invoke" => vec![
            item_snippet_fmt("mcp", CompletionItemKind::PROPERTY, "mcp: ${1:server}", "Required. MCP server name.", "0_mcp"),
            item_snippet_fmt("tool", CompletionItemKind::PROPERTY, "tool: ${1:tool-name}", "Required. MCP tool name (or nika:builtin).", "1_tool"),
            item_snippet_fmt("params", CompletionItemKind::PROPERTY, "params:\n  ${1:key}: ${2:value}", "Tool parameters.", "2_params"),
            item_snippet_fmt("resource", CompletionItemKind::PROPERTY, "resource: ${1:novanet://entity/name}", "MCP resource URI (mutually exclusive with tool:).", "3_resource"),
            item_snippet_fmt("timeout", CompletionItemKind::PROPERTY, "timeout: ${1:60}", "Timeout in seconds (default: 300).", "3_timeout"),
        ],
        "agent" => vec![
            item_snippet_fmt("prompt", CompletionItemKind::PROPERTY, "prompt: ${1}", "Agent goal/prompt.", "0_prompt"),
            item_snippet_fmt("system", CompletionItemKind::PROPERTY, "system: |\n  ${1}", "System prompt for persona.", "1_system"),
            item_snippet_fmt("mcp", CompletionItemKind::PROPERTY, "mcp: [${1}]", "MCP servers to use.", "2_mcp"),
            item_snippet_fmt("tools", CompletionItemKind::PROPERTY, "tools: [${1|builtin,nika:read,nika:write,nika:edit|}]", "Builtin tools.", "2_tools"),
            item_snippet_fmt("skills", CompletionItemKind::PROPERTY, "skills: [${1}]", "Skills to merge into agent.", "2_skills"),
            item_snippet_fmt("max_turns", CompletionItemKind::PROPERTY, "max_turns: ${1:10}", "Maximum conversation turns.", "3_max_turns"),
            item_snippet_fmt("depth_limit", CompletionItemKind::PROPERTY, "depth_limit: ${1:3}", "Max spawn_agent recursion depth.", "3_depth_limit"),
            item_snippet_fmt("token_budget", CompletionItemKind::PROPERTY, "token_budget: ${1:100000}", "Total token budget.", "3_token_budget"),
            item_snippet_fmt("provider", CompletionItemKind::PROPERTY, "provider: ${1|claude,openai,mistral,groq,deepseek,gemini,xai|}", "Provider override.", "4_provider"),
            item_snippet_fmt("model", CompletionItemKind::PROPERTY, "model: ${1}", "Model override.", "4_model"),
            item_snippet_fmt("temperature", CompletionItemKind::PROPERTY, "temperature: ${1:0.7}", "Sampling temperature.", "4_temperature"),
            item_snippet_fmt("max_tokens", CompletionItemKind::PROPERTY, "max_tokens: ${1:4096}", "Max output tokens per turn.", "4_max_tokens"),
            item_snippet_fmt("tool_choice", CompletionItemKind::PROPERTY, "tool_choice: ${1|auto,required,none|}", "Tool selection strategy.", "5_tool_choice"),
            item_snippet_fmt("stop_sequences", CompletionItemKind::PROPERTY, "stop_sequences: [${1}]", "Sequences that stop generation.", "5_stop_sequences"),
            item_snippet_fmt("scope", CompletionItemKind::PROPERTY, "scope: ${1}", "Agent scope preset.", "5_scope"),
            item_snippet_fmt("extended_thinking", CompletionItemKind::PROPERTY, "extended_thinking: true\nthinking_budget: ${1:8192}", "Enable extended thinking.", "6_extended_thinking"),
            item_snippet_fmt("thinking_budget", CompletionItemKind::PROPERTY, "thinking_budget: ${1:8192}", "Token budget for extended thinking.", "6_thinking_budget"),
            item_snippet_fmt("from", CompletionItemKind::PROPERTY, "from: ${1:agent-name}", "Reference a reusable agent definition.", "7_from"),
            item_snippet_fmt("guardrails", CompletionItemKind::PROPERTY, "guardrails:\n  - type: ${1|length,schema,regex,llm|}\n    ${2:max_words}: ${3:500}", "Output guardrails (length, schema, regex, llm).", "8_guardrails"),
        ],
        _ => vec![],
    };
    filter_by_prefix(items, prefix)
}

/// Completions for `with:` block -- task ID references.
fn with_block_completions(text: &str) -> Vec<CompletionItem> {
    extract_task_ids(text)
        .into_iter()
        .map(|id| CompletionItem {
            label: id.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            insert_text: Some(id.clone()),
            detail: Some("Task reference".to_string()),
            documentation: Some(Documentation::String(format!(
                "Reference output from task '{id}'"
            ))),
            sort_text: Some(format!("0_{id}")),
            ..Default::default()
        })
        .collect()
}

/// Completions inside `{{ }}` templates.
fn template_completions(
    text: &str,
    _partial_expr: &str,
    in_transform_chain: bool,
) -> Vec<CompletionItem> {
    if in_transform_chain {
        // Suggest all 39 transform filters (matching nika-core catalog).
        return vec![
            // String transforms
            item_value("upper", "Convert to UPPERCASE.", "00_upper"),
            item_value("lower", "Convert to lowercase.", "01_lower"),
            item_value("trim", "Trim whitespace (both ends).", "02_trim"),
            item_value("trim_start", "Trim leading whitespace.", "03_trim_start"),
            item_value("trim_end", "Trim trailing whitespace.", "04_trim_end"),
            item_value("length", "Get length (string/array).", "05_length"),
            item_value("to_string", "Convert to string.", "06_to_string"),
            // Array transforms
            item_value("first", "First element of array.", "10_first"),
            item_value("last", "Last element of array.", "11_last"),
            item_value("flatten", "Flatten nested arrays.", "12_flatten"),
            item_value("reverse", "Reverse array/string.", "13_reverse"),
            item_value("sort", "Sort array elements.", "14_sort"),
            item_value("unique", "Remove duplicates.", "15_unique"),
            item_value("compact", "Remove null/empty values.", "16_compact"),
            item_value("keys", "Object keys as array.", "17_keys"),
            item_value("values", "Object values as array.", "18_values"),
            // Data transforms (array/object manipulation)
            item_value("pluck(\"field\")", "Extract field from array of objects.", "19_pluck"),
            item_value("where(\"field\", \"value\")", "Filter array by field equality.", "19a_where"),
            item_value("pick(\"f1\", \"f2\")", "Keep only specified object fields.", "19b_pick"),
            item_value("omit(\"f1\", \"f2\")", "Remove specified object fields.", "19c_omit"),
            item_value("sort_by(\"field\")", "Sort array of objects by field.", "19d_sort_by"),
            item_value("group_by(\"field\")", "Group array into object by field.", "19e_group_by"),
            item_value("merge", "Deep merge array of objects.", "19f_merge"),
            item_value("regex(\"pattern\")", "Extract first regex match.", "19g_regex"),
            // Numeric transforms
            item_value("to_number", "Parse as number.", "20_to_number"),
            item_value("round", "Round to integer.", "21_round"),
            item_value("abs", "Absolute value.", "22_abs"),
            item_value("ceil", "Round up.", "23_ceil"),
            item_value("floor", "Round down.", "24_floor"),
            // Type transforms
            item_value("to_bool", "Convert to boolean.", "30_to_bool"),
            item_value("to_json", "Serialize to JSON string.", "31_to_json"),
            item_value("parse_json", "Parse JSON string to value.", "32_parse_json"),
            item_value("type_of", "Get value type name.", "33_type_of"),
            // Parametric transforms
            item_value("join(\", \")", "Join array with separator.", "40_join"),
            item_value("split(\",\")", "Split string by delimiter.", "41_split"),
            item_value("default(\"\")", "Default if null/empty.", "42_default"),
            // Encoding
            item_value("base64_encode", "Encode string to base64.", "45_base64_encode"),
            item_value("base64_decode", "Decode base64 to string.", "46_base64_decode"),
            // System
            item_value("shell", "Shell-escape for safe interpolation.", "50_shell"),
        ];
    }

    let mut items = vec![
        item_snippet_fmt(
            "with.",
            CompletionItemKind::VARIABLE,
            "with.${1:alias}",
            "Reference bound task output.",
            "0_with",
        ),
        item_snippet_fmt(
            "context.files.",
            CompletionItemKind::VARIABLE,
            "context.files.${1:alias}",
            "Reference loaded context file.",
            "1_context",
        ),
        item_snippet_fmt(
            "inputs.",
            CompletionItemKind::VARIABLE,
            "inputs.${1:name}",
            "Reference workflow input parameter.",
            "2_inputs",
        ),
    ];

    // Add $task shorthand for each task.
    for id in extract_task_ids(text) {
        items.push(CompletionItem {
            label: format!("${id}"),
            kind: Some(CompletionItemKind::REFERENCE),
            insert_text: Some(format!("${id}")),
            detail: Some("Task shorthand".to_string()),
            documentation: Some(Documentation::String(format!(
                "Implicit output from task '{id}' (shorthand)"
            ))),
            sort_text: Some(format!("3_{id}")),
            ..Default::default()
        });
    }

    items
}

/// Completions for `invoke:` sub-fields based on focus.
fn invoke_block_completions(focus: &InvokeFocus, prefix: &str) -> Vec<CompletionItem> {
    match focus {
        InvokeFocus::General => {
            let items = vec![
                item_snippet_fmt(
                    "mcp",
                    CompletionItemKind::PROPERTY,
                    "mcp: ${1:server}",
                    "MCP server name.",
                    "0_mcp",
                ),
                item_snippet_fmt(
                    "tool",
                    CompletionItemKind::PROPERTY,
                    "tool: ${1:tool-name}",
                    "MCP tool name.",
                    "1_tool",
                ),
                item_snippet_fmt(
                    "params",
                    CompletionItemKind::PROPERTY,
                    "params:\n  ${1:key}: ${2:value}",
                    "Tool parameters.",
                    "2_params",
                ),
                item_snippet_fmt(
                    "resource",
                    CompletionItemKind::PROPERTY,
                    "resource: ${1:resource-uri}",
                    "MCP resource URI.",
                    "3_resource",
                ),
            ];
            filter_by_prefix(items, prefix)
        }
        InvokeFocus::Tool => {
            // Suggest all 24 builtin nika:* tools + generic MCP tool format.
            let items = vec![
                // Tier 1 — Always-on
                item_value(
                    "nika:import",
                    "Import file into CAS media store.",
                    "00_import",
                ),
                item_value(
                    "nika:dimensions",
                    "Image dimensions (~0.1ms).",
                    "01_dimensions",
                ),
                item_value(
                    "nika:thumbhash",
                    "25-byte image placeholder.",
                    "02_thumbhash",
                ),
                item_value(
                    "nika:dominant_color",
                    "Color palette extraction.",
                    "03_dominant_color",
                ),
                item_value(
                    "nika:pipeline",
                    "Chain ops in-memory (1 read → N ops → 1 write).",
                    "04_pipeline",
                ),
                // Tier 2 — media-core
                item_value(
                    "nika:thumbnail",
                    "SIMD-accelerated resize (Lanczos3).",
                    "10_thumbnail",
                ),
                item_value(
                    "nika:convert",
                    "Format conversion (PNG/JPEG/WebP).",
                    "11_convert",
                ),
                item_value("nika:strip", "Remove metadata (re-encode).", "12_strip"),
                item_value("nika:metadata", "EXIF/audio/video metadata.", "13_metadata"),
                item_value(
                    "nika:optimize",
                    "Lossless PNG optimization (oxipng).",
                    "14_optimize",
                ),
                item_value(
                    "nika:svg_render",
                    "SVG to PNG rasterization.",
                    "15_svg_render",
                ),
                // Tier 3 — Opt-in
                item_value("nika:phash", "Perceptual image hashing.", "20_phash"),
                item_value(
                    "nika:compare",
                    "Visual comparison via perceptual hash.",
                    "21_compare",
                ),
                item_value("nika:pdf_extract", "PDF text extraction.", "22_pdf_extract"),
                item_value(
                    "nika:chart",
                    "Bar/line/pie charts from JSON data.",
                    "23_chart",
                ),
                item_value(
                    "nika:provenance",
                    "C2PA content credentials (sign).",
                    "24_provenance",
                ),
                item_value("nika:verify", "C2PA manifest verification.", "25_verify"),
                item_value(
                    "nika:qr_validate",
                    "QR decode + scan score (0-100).",
                    "26_qr_validate",
                ),
                item_value("nika:quality", "Image quality (DSSIM/SSIM).", "27_quality"),
                item_value(
                    "nika:html_to_md",
                    "HTML to clean Markdown.",
                    "28_html_to_md",
                ),
                item_value(
                    "nika:css_select",
                    "CSS selector extraction.",
                    "29_css_select",
                ),
                item_value(
                    "nika:extract_metadata",
                    "OG/Twitter/JSON-LD metadata.",
                    "30_extract_metadata",
                ),
                item_value(
                    "nika:extract_links",
                    "Link classification.",
                    "31_extract_links",
                ),
                item_value(
                    "nika:readability",
                    "Article content extraction.",
                    "32_readability",
                ),
            ];
            filter_by_prefix(items, prefix)
        }
        _ => vec![],
    }
}

/// Completions for `mcp:` configuration section.
fn mcp_config_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt(
            "command",
            CompletionItemKind::PROPERTY,
            "command: ${1:npx}",
            "Command to start the MCP server.",
            "0_command",
        ),
        item_snippet_fmt(
            "args",
            CompletionItemKind::PROPERTY,
            "args: [${1}]",
            "Arguments to pass to the command.",
            "1_args",
        ),
        item_snippet_fmt(
            "env",
            CompletionItemKind::PROPERTY,
            "env:\n  ${1:KEY}: ${2:value}",
            "Environment variables for the MCP server.",
            "2_env",
        ),
    ];
    filter_by_prefix(items, prefix)
}

/// Provider and model completions from nika-core catalogs.
fn provider_completions(
    prefix: &str,
    current_provider: Option<&str>,
    daemon_providers: Option<&[ProviderStatusInfo]>,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // LLM providers from the catalog.
    for provider in KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
    {
        // Enrich detail with daemon key status if available.
        let detail = if let Some(dp) = daemon_providers {
            if let Some(status) = dp.iter().find(|s| s.id == provider.id) {
                if status.has_key {
                    format!("{} \u{2713}", provider.name) // ✓
                } else {
                    format!("{} \u{2014} no API key", provider.name) // —
                }
            } else {
                provider.name.to_string()
            }
        } else {
            provider.name.to_string()
        };

        // Sort configured providers first when daemon data is available.
        let sort_prefix = if let Some(dp) = daemon_providers {
            if dp.iter().any(|s| s.id == provider.id && s.has_key) {
                "0a" // configured first
            } else {
                "0z" // unconfigured after
            }
        } else {
            "0"
        };

        items.push(CompletionItem {
            label: provider.id.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            insert_text: Some(provider.id.to_string()),
            detail: Some(detail),
            documentation: Some(Documentation::String(provider.description.to_string())),
            sort_text: Some(format!("{}_{}", sort_prefix, provider.id)),
            ..Default::default()
        });

        // Also add aliases.
        for alias in provider.aliases {
            items.push(CompletionItem {
                label: (*alias).to_string(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                insert_text: Some((*alias).to_string()),
                detail: Some(format!("{} (alias for {})", provider.name, provider.id)),
                sort_text: Some(format!("1_{alias}")),
                ..Default::default()
            });
        }
    }

    // If a provider is known, add relevant model names.
    if let Some(provider) = current_provider {
        let model_prefix = match provider {
            "anthropic" | "claude" => Some("claude-"),
            "openai" | "gpt" => Some("gpt-"),
            _ => None,
        };
        if let Some(mp) = model_prefix {
            items.push(CompletionItem {
                label: format!("{mp}..."),
                kind: Some(CompletionItemKind::VALUE),
                insert_text: Some(format!("{mp}${{1}}")),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                detail: Some(format!("Model for {provider}")),
                sort_text: Some(format!("2_{mp}")),
                ..Default::default()
            });
        }
    }

    // Add local models from the catalog (text models only for completions).
    for model in KNOWN_MODELS
        .iter()
        .filter(|m| m.model_type == ModelType::Text)
    {
        items.push(CompletionItem {
            label: model.id.to_string(),
            kind: Some(CompletionItemKind::VALUE),
            insert_text: Some(model.id.to_string()),
            detail: Some(format!("{} ({:.0}B)", model.name, model.param_billions)),
            documentation: Some(Documentation::String(model.description.to_string())),
            sort_text: Some(format!("3_{}", model.id)),
            ..Default::default()
        });
    }

    filter_by_prefix(items, prefix)
}

/// Completions for `content:` blocks (multimodal vision support).
fn content_part_completions(focus: &ContentFocus, prefix: &str) -> Vec<CompletionItem> {
    match focus {
        ContentFocus::PartType => {
            let items = vec![
                item_snippet_fmt(
                    "text",
                    CompletionItemKind::VALUE,
                    "type: text\n  text: ${1:content}",
                    "Text content part.",
                    "0_text",
                ),
                item_snippet_fmt(
                    "image",
                    CompletionItemKind::VALUE,
                    "type: image\n  source: ${1:hash_or_path}\n  detail: ${2|auto,low,high|}",
                    "Image content part (CAS hash or path).",
                    "1_image",
                ),
                item_snippet_fmt(
                    "image_url",
                    CompletionItemKind::VALUE,
                    "type: image_url\n  url: ${1:https://}\n  detail: ${2|auto,low,high|}",
                    "Image URL content part.",
                    "2_image_url",
                ),
            ];
            filter_by_prefix(items, prefix)
        }
        ContentFocus::ImageDetail => {
            vec![
                item_value("auto", "Automatic detail level.", "0_auto"),
                item_value("low", "Low detail (faster, cheaper).", "1_low"),
                item_value("high", "High detail (better quality).", "2_high"),
            ]
        }
        ContentFocus::ImageUrl | ContentFocus::PartField => {
            let items = vec![
                item_snippet_fmt(
                    "text",
                    CompletionItemKind::PROPERTY,
                    "text: ${1}",
                    "Text content.",
                    "0_text",
                ),
                item_snippet_fmt(
                    "source",
                    CompletionItemKind::PROPERTY,
                    "source: ${1}",
                    "CAS hash or file path.",
                    "1_source",
                ),
                item_snippet_fmt(
                    "url",
                    CompletionItemKind::PROPERTY,
                    "url: ${1:https://}",
                    "Image URL.",
                    "1_url",
                ),
                item_snippet_fmt(
                    "detail",
                    CompletionItemKind::PROPERTY,
                    "detail: ${1|auto,low,high|}",
                    "Image detail level.",
                    "2_detail",
                ),
            ];
            filter_by_prefix(items, prefix)
        }
    }
}

/// Completions for `for_each:` block.
fn for_each_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt(
            "items",
            CompletionItemKind::PROPERTY,
            "items: [${1}]",
            "Array to iterate.",
            "0_items",
        ),
        item_snippet_fmt(
            "as",
            CompletionItemKind::PROPERTY,
            "as: ${1:item}",
            "Loop variable name.",
            "1_as",
        ),
        item_snippet_fmt(
            "concurrency",
            CompletionItemKind::PROPERTY,
            "concurrency: ${1:3}",
            "Max parallel iterations.",
            "2_concurrency",
        ),
    ];
    filter_by_prefix(items, prefix)
}

/// Completions for `structured:` / schema block.
fn schema_block_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt(
            "type",
            CompletionItemKind::PROPERTY,
            "type: ${1|object,array,string,number,boolean|}",
            "JSON Schema type.",
            "0_type",
        ),
        item_snippet_fmt(
            "properties",
            CompletionItemKind::PROPERTY,
            "properties:\n  ${1:field}:\n    type: ${2:string}",
            "Object properties.",
            "1_properties",
        ),
        item_snippet_fmt(
            "required",
            CompletionItemKind::PROPERTY,
            "required: [${1}]",
            "Required properties.",
            "2_required",
        ),
        item_snippet_fmt(
            "items",
            CompletionItemKind::PROPERTY,
            "items:\n  type: ${1:string}",
            "Array item schema.",
            "3_items",
        ),
        item_snippet_fmt(
            "description",
            CompletionItemKind::PROPERTY,
            "description: \"${1}\"",
            "Field description.",
            "4_description",
        ),
    ];
    filter_by_prefix(items, prefix)
}

/// Completions for `depends_on:` -- available task IDs.
fn depends_on_completions(text: &str, existing_deps: &[String]) -> Vec<CompletionItem> {
    extract_task_ids(text)
        .into_iter()
        .filter(|id| !existing_deps.contains(id))
        .map(|id| CompletionItem {
            label: id.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            insert_text: Some(id.clone()),
            detail: Some("Task dependency".to_string()),
            documentation: Some(Documentation::String(format!("Add '{id}' as a dependency"))),
            sort_text: Some(format!("0_{id}")),
            ..Default::default()
        })
        .collect()
}

/// Completions for `guardrails:` block.
fn guardrails_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt(
            "input",
            CompletionItemKind::PROPERTY,
            "input: ${1:rule}",
            "Input guardrail.",
            "0_input",
        ),
        item_snippet_fmt(
            "output",
            CompletionItemKind::PROPERTY,
            "output: ${1:rule}",
            "Output guardrail.",
            "1_output",
        ),
        item_snippet_fmt(
            "max_length",
            CompletionItemKind::PROPERTY,
            "max_length: ${1:4096}",
            "Maximum output length.",
            "2_max_length",
        ),
        item_snippet_fmt(
            "forbidden_topics",
            CompletionItemKind::PROPERTY,
            "forbidden_topics: [${1}]",
            "Topics to block.",
            "3_forbidden",
        ),
    ];
    filter_by_prefix(items, prefix)
}

/// Completions for `retry:` block.
fn retry_block_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt(
            "max_attempts",
            CompletionItemKind::PROPERTY,
            "max_attempts: ${1:3}",
            "Maximum retry attempts.",
            "0_max_attempts",
        ),
        item_snippet_fmt(
            "delay",
            CompletionItemKind::PROPERTY,
            "delay: ${1:1s}",
            "Delay between retries.",
            "1_delay",
        ),
        item_snippet_fmt(
            "backoff",
            CompletionItemKind::PROPERTY,
            "backoff: ${1|exponential,linear,fixed|}",
            "Backoff strategy.",
            "2_backoff",
        ),
    ];
    filter_by_prefix(items, prefix)
}

/// Completions for `limits:` / `timeout:` block.
fn limits_block_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt(
            "timeout",
            CompletionItemKind::PROPERTY,
            "timeout: ${1:30}",
            "Timeout in seconds.",
            "0_timeout",
        ),
        item_snippet_fmt(
            "max_tokens",
            CompletionItemKind::PROPERTY,
            "max_tokens: ${1:4096}",
            "Max output tokens.",
            "1_max_tokens",
        ),
    ];
    filter_by_prefix(items, prefix)
}

// ---------------------------------------------------------------------------
// Item constructors
// ---------------------------------------------------------------------------

/// Create a plain-text insertion item.
fn item_snippet(
    label: &str,
    kind: CompletionItemKind,
    insert_text: &str,
    doc: &str,
    sort_key: &str,
) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        insert_text: Some(insert_text.to_string()),
        documentation: Some(Documentation::String(doc.to_string())),
        sort_text: Some(sort_key.to_string()),
        ..Default::default()
    }
}

/// Create a snippet-formatted insertion item.
fn item_snippet_fmt(
    label: &str,
    kind: CompletionItemKind,
    insert_text: &str,
    doc: &str,
    sort_key: &str,
) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        insert_text: Some(insert_text.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        documentation: Some(Documentation::String(doc.to_string())),
        sort_text: Some(sort_key.to_string()),
        ..Default::default()
    }
}

/// Create a simple value completion item.
fn item_value(label: &str, doc: &str, sort_key: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::VALUE),
        insert_text: Some(label.to_string()),
        documentation: Some(Documentation::String(doc.to_string())),
        sort_text: Some(sort_key.to_string()),
        ..Default::default()
    }
}

/// Filter completion items by prefix (case-insensitive).
fn filter_by_prefix(items: Vec<CompletionItem>, prefix: &str) -> Vec<CompletionItem> {
    if prefix.is_empty() {
        return items;
    }
    let lower = prefix.to_lowercase();
    items
        .into_iter()
        .filter(|item| item.label.to_lowercase().starts_with(&lower))
        .collect()
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::context::detect_context;

    /// Helper: detect context and compute completions.
    fn complete_at(text: &str, line: usize, character: usize) -> Vec<CompletionItem> {
        let offset = text_offset(text, line, character);
        let ctx = detect_context(text, offset, None);
        completions(text, offset, &ctx, None)
    }

    /// Convert line/character to byte offset.
    fn text_offset(text: &str, line: usize, character: usize) -> u32 {
        let mut offset = 0usize;
        for (i, l) in text.lines().enumerate() {
            if i == line {
                return (offset + character.min(l.len())) as u32;
            }
            offset += l.len() + 1;
        }
        text.len() as u32
    }

    // -----------------------------------------------------------------------
    // WorkflowRoot completions
    // -----------------------------------------------------------------------

    #[test]
    fn workflow_root_has_schema() {
        let items = complete_at("", 0, 0);
        assert!(items.iter().any(|i| i.label == "schema"));
    }

    #[test]
    fn workflow_root_has_tasks() {
        let items = complete_at("", 0, 0);
        assert!(items.iter().any(|i| i.label == "tasks"));
    }

    #[test]
    fn workflow_root_has_mcp() {
        let items = complete_at("", 0, 0);
        assert!(items.iter().any(|i| i.label == "mcp"));
    }

    #[test]
    fn workflow_root_has_inputs() {
        let items = complete_at("", 0, 0);
        assert!(items.iter().any(|i| i.label == "inputs"));
    }

    #[test]
    fn workflow_root_has_edges() {
        let items = complete_at("", 0, 0);
        assert!(items.iter().any(|i| i.label == "edges"));
    }

    #[test]
    fn workflow_root_sorted() {
        let items = complete_at("", 0, 0);
        let sort_keys: Vec<_> = items
            .iter()
            .filter_map(|i| i.sort_text.as_deref())
            .collect();
        let mut sorted = sort_keys.clone();
        sorted.sort();
        assert_eq!(sort_keys, sorted, "Root items should be pre-sorted");
    }

    // -----------------------------------------------------------------------
    // TaskField completions
    // -----------------------------------------------------------------------

    #[test]
    fn task_field_has_verbs() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    ";
        let items = complete_at(yaml, 3, 4);
        assert!(items.iter().any(|i| i.label == "infer"), "Missing infer");
        assert!(items.iter().any(|i| i.label == "exec"), "Missing exec");
        assert!(items.iter().any(|i| i.label == "fetch"), "Missing fetch");
        assert!(items.iter().any(|i| i.label == "invoke"), "Missing invoke");
        assert!(items.iter().any(|i| i.label == "agent"), "Missing agent");
    }

    #[test]
    fn task_field_has_with() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    ";
        let items = complete_at(yaml, 3, 4);
        assert!(items.iter().any(|i| i.label == "with"));
    }

    #[test]
    fn task_field_has_content() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    ";
        let items = complete_at(yaml, 3, 4);
        assert!(
            items.iter().any(|i| i.label == "content"),
            "Missing content field"
        );
    }

    #[test]
    fn task_field_has_depends_on() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    ";
        let items = complete_at(yaml, 3, 4);
        assert!(items.iter().any(|i| i.label == "depends_on"));
    }

    // -----------------------------------------------------------------------
    // VerbBlock completions
    // -----------------------------------------------------------------------

    #[test]
    fn infer_block_has_prompt_and_model() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      ";
        let items = complete_at(yaml, 4, 6);
        assert!(items.iter().any(|i| i.label == "prompt"), "Missing prompt");
        assert!(items.iter().any(|i| i.label == "model"), "Missing model");
        assert!(
            items.iter().any(|i| i.label == "temperature"),
            "Missing temperature"
        );
        assert!(items.iter().any(|i| i.label == "system"), "Missing system");
    }

    #[test]
    fn exec_block_has_command_and_shell() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    exec:
      ";
        let items = complete_at(yaml, 4, 6);
        assert!(items.iter().any(|i| i.label == "command"));
        assert!(items.iter().any(|i| i.label == "shell"));
    }

    #[test]
    fn fetch_block_has_url_and_method() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    fetch:
      ";
        let items = complete_at(yaml, 4, 6);
        assert!(items.iter().any(|i| i.label == "url"));
        assert!(items.iter().any(|i| i.label == "method"));
        assert!(items.iter().any(|i| i.label == "headers"));
        assert!(items.iter().any(|i| i.label == "body"));
    }

    #[test]
    fn invoke_block_has_mcp_and_tool() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    invoke:
      ";
        let items = complete_at(yaml, 4, 6);
        assert!(items.iter().any(|i| i.label == "mcp"));
        assert!(items.iter().any(|i| i.label == "tool"));
        assert!(items.iter().any(|i| i.label == "params"));
    }

    #[test]
    fn agent_block_has_prompt_mcp_max_turns() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    agent:
      ";
        let items = complete_at(yaml, 4, 6);
        assert!(items.iter().any(|i| i.label == "prompt"), "Missing prompt");
        assert!(items.iter().any(|i| i.label == "mcp"), "Missing mcp");
        assert!(
            items.iter().any(|i| i.label == "max_turns"),
            "Missing max_turns"
        );
        assert!(
            items.iter().any(|i| i.label == "extended_thinking"),
            "Missing extended_thinking"
        );
    }

    // -----------------------------------------------------------------------
    // Content completions (NEW -- vision support)
    // -----------------------------------------------------------------------

    #[test]
    fn content_part_type_suggestions() {
        let items = content_part_completions(&ContentFocus::PartType, "");
        assert!(items.iter().any(|i| i.label == "text"));
        assert!(items.iter().any(|i| i.label == "image"));
        assert!(items.iter().any(|i| i.label == "image_url"));
    }

    #[test]
    fn content_image_detail_values() {
        let items = content_part_completions(&ContentFocus::ImageDetail, "");
        assert!(items.iter().any(|i| i.label == "auto"));
        assert!(items.iter().any(|i| i.label == "low"));
        assert!(items.iter().any(|i| i.label == "high"));
    }

    #[test]
    fn content_part_field_suggestions() {
        let items = content_part_completions(&ContentFocus::PartField, "");
        assert!(items.iter().any(|i| i.label == "text"));
        assert!(items.iter().any(|i| i.label == "source"));
        assert!(items.iter().any(|i| i.label == "detail"));
    }

    // -----------------------------------------------------------------------
    // Provider/model completions (from nika-core catalogs)
    // -----------------------------------------------------------------------

    #[test]
    fn provider_completions_include_llm_providers() {
        let items = provider_completions("", None, None);
        assert!(
            items.iter().any(|i| i.label == "anthropic"),
            "Missing anthropic"
        );
        assert!(items.iter().any(|i| i.label == "openai"), "Missing openai");
        assert!(
            items.iter().any(|i| i.label == "mistral"),
            "Missing mistral"
        );
        assert!(items.iter().any(|i| i.label == "groq"), "Missing groq");
        assert!(
            items.iter().any(|i| i.label == "deepseek"),
            "Missing deepseek"
        );
        assert!(items.iter().any(|i| i.label == "gemini"), "Missing gemini");
        assert!(items.iter().any(|i| i.label == "xai"), "Missing xai");
    }

    #[test]
    fn provider_completions_include_aliases() {
        let items = provider_completions("", None, None);
        assert!(
            items.iter().any(|i| i.label == "claude"),
            "Missing claude alias"
        );
        assert!(items.iter().any(|i| i.label == "gpt"), "Missing gpt alias");
    }

    #[test]
    fn provider_completions_include_local_models() {
        let items = provider_completions("", None, None);
        // Should include known local models from catalog.
        assert!(
            items.iter().any(|i| i.label == "qwen3:8b"),
            "Missing qwen3:8b from model catalog"
        );
    }

    #[test]
    fn provider_completions_filter_by_prefix() {
        let items = provider_completions("an", None, None);
        assert!(items.iter().any(|i| i.label == "anthropic"));
        assert!(!items.iter().any(|i| i.label == "openai"));
    }

    #[test]
    fn provider_completion_with_daemon_shows_key_status() {
        use nika_core::catalogs::{KeySource, ProviderCategory, ProviderStatusInfo};
        let providers = vec![
            ProviderStatusInfo {
                id: "anthropic".into(),
                name: "Anthropic Claude".into(),
                has_key: true,
                source: KeySource::Env,
                category: ProviderCategory::Llm,
                env_var: "ANTHROPIC_API_KEY".into(),
            },
            ProviderStatusInfo {
                id: "openai".into(),
                name: "OpenAI".into(),
                has_key: false,
                source: KeySource::NotFound,
                category: ProviderCategory::Llm,
                env_var: "OPENAI_API_KEY".into(),
            },
        ];
        let items = provider_completions("", None, Some(&providers));
        let anthropic = items.iter().find(|i| i.label == "anthropic").unwrap();
        assert!(
            anthropic.detail.as_ref().unwrap().contains("✓"),
            "Expected checkmark for configured provider: {:?}",
            anthropic.detail
        );
        let openai = items.iter().find(|i| i.label == "openai").unwrap();
        assert!(
            openai.detail.as_ref().unwrap().contains("no API key"),
            "Expected 'no API key' for unconfigured provider: {:?}",
            openai.detail
        );
    }

    #[test]
    fn provider_completion_without_daemon_shows_all() {
        let items = provider_completions("", None, None);
        assert!(items.len() >= 8, "Should list all providers without status");
    }

    // -----------------------------------------------------------------------
    // DependsOn completions
    // -----------------------------------------------------------------------

    #[test]
    fn depends_on_lists_available_tasks() {
        let text = "\
tasks:
  - id: step1
    infer: hello
  - id: step2
    infer: world
";
        let items = depends_on_completions(text, &[]);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|i| i.label == "step1"));
        assert!(items.iter().any(|i| i.label == "step2"));
    }

    #[test]
    fn depends_on_excludes_existing() {
        let text = "\
tasks:
  - id: step1
    infer: hello
  - id: step2
    infer: world
";
        let items = depends_on_completions(text, &["step1".to_string()]);
        assert_eq!(items.len(), 1);
        assert!(items.iter().any(|i| i.label == "step2"));
    }

    // -----------------------------------------------------------------------
    // MCP config completions
    // -----------------------------------------------------------------------

    #[test]
    fn mcp_config_has_command_args_env() {
        let items = mcp_config_completions("");
        assert!(items.iter().any(|i| i.label == "command"));
        assert!(items.iter().any(|i| i.label == "args"));
        assert!(items.iter().any(|i| i.label == "env"));
    }

    // -----------------------------------------------------------------------
    // Template completions
    // -----------------------------------------------------------------------

    #[test]
    fn template_has_with_and_context() {
        let text = "\
tasks:
  - id: step1
    infer: hello
";
        let items = template_completions(text, "", false);
        assert!(items.iter().any(|i| i.label == "with."));
        assert!(items.iter().any(|i| i.label == "context.files."));
        assert!(items.iter().any(|i| i.label == "inputs."));
    }

    #[test]
    fn template_transform_chain_suggests_filters() {
        let text = "";
        let items = template_completions(text, "with.data | ", true);
        assert!(items.iter().any(|i| i.label == "upper"));
        assert!(items.iter().any(|i| i.label == "lower"));
        assert!(items.iter().any(|i| i.label == "trim"));
        assert!(items.iter().any(|i| i.label == "to_json"));
        assert!(items.iter().any(|i| i.label == "first"));
        assert!(items.iter().any(|i| i.label == "shell"));
        // Sprint 1 transforms
        assert!(items.iter().any(|i| i.label.starts_with("pluck")));
        assert!(items.iter().any(|i| i.label.starts_with("where")));
        assert!(items.iter().any(|i| i.label.starts_with("pick")));
        assert!(items.iter().any(|i| i.label.starts_with("omit")));
        assert!(items.iter().any(|i| i.label.starts_with("sort_by")));
        assert!(items.iter().any(|i| i.label.starts_with("group_by")));
        assert!(items.iter().any(|i| i.label == "merge"));
        assert!(items.iter().any(|i| i.label.starts_with("regex")));
        assert!(items.iter().any(|i| i.label == "base64_encode"));
        assert!(items.iter().any(|i| i.label == "base64_decode"));
        assert_eq!(items.len(), 39, "should offer all transforms");
    }

    #[test]
    fn template_includes_task_shorthands() {
        let text = "\
tasks:
  - id: generate
    infer: hello
  - id: process
";
        let items = template_completions(text, "", false);
        assert!(items.iter().any(|i| i.label == "$generate"));
        assert!(items.iter().any(|i| i.label == "$process"));
    }

    // -----------------------------------------------------------------------
    // Sort order and kind
    // -----------------------------------------------------------------------

    #[test]
    fn completion_items_have_correct_kinds() {
        let items = workflow_root_completions("");
        for item in &items {
            assert!(item.kind.is_some(), "Item {} missing kind", item.label);
        }
    }

    #[test]
    fn completion_items_have_sort_text() {
        let items = workflow_root_completions("");
        for item in &items {
            assert!(
                item.sort_text.is_some(),
                "Item {} missing sort_text",
                item.label
            );
        }
    }

    // -----------------------------------------------------------------------
    // Empty/unknown context
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_context_returns_empty() {
        let ctx = CursorContext::Unknown {
            prefix: String::new(),
        };
        let items = completions("", 0, &ctx, None);
        assert!(items.is_empty());
    }

    // -----------------------------------------------------------------------
    // Retry and limits
    // -----------------------------------------------------------------------

    #[test]
    fn retry_block_has_max_attempts() {
        let items = retry_block_completions("");
        assert!(items.iter().any(|i| i.label == "max_attempts"));
        assert!(items.iter().any(|i| i.label == "delay"));
        assert!(items.iter().any(|i| i.label == "backoff"));
    }

    #[test]
    fn limits_block_has_timeout() {
        let items = limits_block_completions("");
        assert!(items.iter().any(|i| i.label == "timeout"));
        assert!(items.iter().any(|i| i.label == "max_tokens"));
    }

    // -----------------------------------------------------------------------
    // Schema block
    // -----------------------------------------------------------------------

    #[test]
    fn schema_block_has_type_and_properties() {
        let items = schema_block_completions("");
        assert!(items.iter().any(|i| i.label == "type"));
        assert!(items.iter().any(|i| i.label == "properties"));
        assert!(items.iter().any(|i| i.label == "required"));
    }

    // -----------------------------------------------------------------------
    // ForEach completions
    // -----------------------------------------------------------------------

    #[test]
    fn for_each_has_items_as_concurrency() {
        let items = for_each_completions("");
        assert!(items.iter().any(|i| i.label == "items"));
        assert!(items.iter().any(|i| i.label == "as"));
        assert!(items.iter().any(|i| i.label == "concurrency"));
    }

    // -----------------------------------------------------------------------
    // Guardrails completions
    // -----------------------------------------------------------------------

    #[test]
    fn guardrails_has_input_output() {
        let items = guardrails_completions("");
        assert!(items.iter().any(|i| i.label == "input"));
        assert!(items.iter().any(|i| i.label == "output"));
    }

    // -----------------------------------------------------------------------
    // Prefix filtering
    // -----------------------------------------------------------------------

    #[test]
    fn prefix_filter_narrows_results() {
        let items = workflow_root_completions("sch");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "schema");
    }

    #[test]
    fn prefix_filter_case_insensitive() {
        let items = workflow_root_completions("SCH");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "schema");
    }

    // -----------------------------------------------------------------------
    // Integration: full pipeline
    // -----------------------------------------------------------------------

    #[test]
    fn integration_full_workflow_completion() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    with:
      data: step1
    infer:
      prompt: \"Process {{with.data}}\"
";
        // Root level completions.
        let root = complete_at(yaml, 0, 0);
        assert!(!root.is_empty());

        // Task field completions.
        // Position on a blank line after task content (would need careful positioning).
        // Just verify no panics.
        let _ = complete_at(yaml, 3, 4);
    }

    // -----------------------------------------------------------------------
    // Verb scaffolds
    // -----------------------------------------------------------------------

    #[test]
    fn verb_scaffolds_are_multiline_snippets() {
        let items = task_field_completions("", &[]);
        let verbs = ["infer", "exec", "fetch", "invoke", "agent"];
        for verb in verbs {
            let item = items
                .iter()
                .find(|i| i.label == verb)
                .unwrap_or_else(|| panic!("Missing verb completion: {verb}"));
            assert_eq!(
                item.insert_text_format,
                Some(InsertTextFormat::SNIPPET),
                "Verb '{verb}' must use SNIPPET format"
            );
            let text = item.insert_text.as_deref().unwrap();
            assert!(
                text.contains("${0}"),
                "Verb '{verb}' scaffold must contain final tab stop ${{0}}"
            );
        }

        // Verify specific scaffold content
        let infer = items.iter().find(|i| i.label == "infer").unwrap();
        assert!(infer.insert_text.as_deref().unwrap().contains("prompt:"));

        let invoke = items.iter().find(|i| i.label == "invoke").unwrap();
        let invoke_text = invoke.insert_text.as_deref().unwrap();
        assert!(invoke_text.contains("tool:"));
        assert!(invoke_text.contains("params:"));

        let agent = items.iter().find(|i| i.label == "agent").unwrap();
        let agent_text = agent.insert_text.as_deref().unwrap();
        assert!(agent_text.contains("prompt:"));
        assert!(agent_text.contains("mcp:"));
        assert!(agent_text.contains("max_turns:"));
    }
}
