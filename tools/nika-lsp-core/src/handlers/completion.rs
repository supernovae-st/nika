//! Completion handler for `.nika.yaml` workflow files.
//!
//! Pure, synchronous completion logic. No async, no server state -- just
//! `(text, offset, context) -> Vec<CompletionItem>`.
//!
//! Ported from the embedded LSP (`nika/src/lsp/handlers/completion.rs`),
//! extended with content/vision, provider/model catalogs, and depends_on
//! completions.

use ls_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat,
};
use nika_core::catalogs::models::{ModelType, KNOWN_MODELS};
use nika_core::catalogs::providers::{ProviderCategory, KNOWN_PROVIDERS};

use crate::analysis::context::{
    extract_task_ids, ContentFocus, CursorContext, InvokeFocus,
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compute completions for the given cursor context.
///
/// Pure function -- no async, no state beyond the arguments.
pub fn completions(text: &str, _offset: u32, context: &CursorContext) -> Vec<CompletionItem> {
    match context {
        CursorContext::WorkflowRoot { prefix } => workflow_root_completions(prefix),
        CursorContext::TaskField {
            prefix,
            existing_fields,
            ..
        } => task_field_completions(prefix, existing_fields),
        CursorContext::VerbBlock {
            verb, prefix, ..
        } => verb_block_completions(verb, prefix),
        CursorContext::WithBlock { .. } => with_block_completions(text),
        CursorContext::Template {
            partial_expr,
            in_transform_chain,
            ..
        } => template_completions(text, partial_expr, *in_transform_chain),
        CursorContext::InvokeBlock {
            focus, prefix, ..
        } => invoke_block_completions(focus, prefix),
        CursorContext::McpConfig { prefix, .. } => mcp_config_completions(prefix),
        CursorContext::ProviderContext {
            prefix,
            current_provider,
            ..
        } => provider_completions(prefix, current_provider.as_deref()),
        CursorContext::ContentPart {
            focus, prefix, ..
        } => content_part_completions(focus, prefix),
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

/// Top-level workflow keys: schema, workflow, tasks, mcp, context, inputs, imports, edges.
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
        // The 5 verbs
        item_snippet_fmt(
            "infer",
            CompletionItemKind::KEYWORD,
            "infer: ${1:prompt}",
            "LLM text generation.",
            "1_infer",
        ),
        item_snippet_fmt(
            "exec",
            CompletionItemKind::KEYWORD,
            "exec: ${1:command}",
            "Shell command.",
            "1_exec",
        ),
        item_snippet_fmt(
            "fetch",
            CompletionItemKind::KEYWORD,
            "fetch:\n  url: ${1:https://}\n  method: ${2:GET}",
            "HTTP request.",
            "1_fetch",
        ),
        item_snippet_fmt(
            "invoke",
            CompletionItemKind::KEYWORD,
            "invoke:\n  mcp: ${1:server}\n  tool: ${2:tool-name}\n  params:\n    ${3:key}: ${4:value}",
            "MCP tool invocation.",
            "1_invoke",
        ),
        item_snippet_fmt(
            "agent",
            CompletionItemKind::KEYWORD,
            "agent:\n  prompt: ${1:goal}\n  mcp: [${2:server}]\n  max_turns: ${3:10}",
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
            "guardrails",
            CompletionItemKind::PROPERTY,
            "guardrails:\n  ${1:input}: ${2:rule}",
            "Input/output guardrails.",
            "3_guardrails",
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
            item_snippet_fmt("prompt", CompletionItemKind::PROPERTY, "prompt: ${1}", "Text prompt.", "0_prompt"),
            item_snippet_fmt("system", CompletionItemKind::PROPERTY, "system: ${1:You are a helpful assistant.}", "System prompt.", "1_system"),
            item_snippet_fmt("model", CompletionItemKind::PROPERTY, "model: ${1:claude-sonnet-4-6}", "Model override.", "2_model"),
            item_snippet_fmt("provider", CompletionItemKind::PROPERTY, "provider: ${1|claude,openai,mistral,groq,deepseek,gemini,xai|}", "Provider override.", "2_provider"),
            item_snippet_fmt("temperature", CompletionItemKind::PROPERTY, "temperature: ${1:0.7}", "Sampling temperature.", "3_temperature"),
            item_snippet_fmt("max_tokens", CompletionItemKind::PROPERTY, "max_tokens: ${1:1000}", "Maximum output tokens.", "3_max_tokens"),
            item_snippet_fmt("content", CompletionItemKind::PROPERTY, "content:\n  - type: ${1|text,image,image_url|}\n    ${2:text}: ${3:value}", "Multimodal content.", "4_content"),
        ],
        "exec" => vec![
            item_snippet_fmt("command", CompletionItemKind::PROPERTY, "command: ${1}", "Shell command to run.", "0_command"),
            item_snippet_fmt("shell", CompletionItemKind::PROPERTY, "shell: ${1|true,false|}", "Enable shell mode. Default: false (secure).", "1_shell"),
        ],
        "fetch" => vec![
            item_snippet_fmt("url", CompletionItemKind::PROPERTY, "url: ${1:https://}", "Required. Request URL.", "0_url"),
            item_snippet_fmt("method", CompletionItemKind::PROPERTY, "method: ${1|GET,POST,PUT,DELETE,PATCH|}", "HTTP method. Default: GET.", "1_method"),
            item_snippet_fmt("headers", CompletionItemKind::PROPERTY, "headers:\n  ${1:Content-Type}: ${2:application/json}", "HTTP request headers.", "2_headers"),
            item_snippet_fmt("body", CompletionItemKind::PROPERTY, "body: ${1}", "Request body (string or object).", "3_body"),
            item_snippet_fmt("retry", CompletionItemKind::PROPERTY, "retry:\n  max_attempts: ${1:3}\n  delay: ${2:1s}", "Retry configuration.", "4_retry"),
        ],
        "invoke" => vec![
            item_snippet_fmt("mcp", CompletionItemKind::PROPERTY, "mcp: ${1:server}", "Required. MCP server name.", "0_mcp"),
            item_snippet_fmt("tool", CompletionItemKind::PROPERTY, "tool: ${1:tool-name}", "Required. MCP tool name.", "1_tool"),
            item_snippet_fmt("params", CompletionItemKind::PROPERTY, "params:\n  ${1:key}: ${2:value}", "Tool parameters.", "2_params"),
            item_snippet_fmt("resource", CompletionItemKind::PROPERTY, "resource: ${1:resource-uri}", "MCP resource URI (alternative to tool).", "3_resource"),
        ],
        "agent" => vec![
            item_snippet_fmt("prompt", CompletionItemKind::PROPERTY, "prompt: ${1}", "Agent goal/prompt.", "0_prompt"),
            item_snippet_fmt("system", CompletionItemKind::PROPERTY, "system: |\n  ${1}", "System prompt for persona.", "1_system"),
            item_snippet_fmt("mcp", CompletionItemKind::PROPERTY, "mcp: [${1}]", "MCP servers to use.", "2_mcp"),
            item_snippet_fmt("tools", CompletionItemKind::PROPERTY, "tools: [${1|builtin,nika:read,nika:write,nika:edit|}]", "Builtin tools.", "2_tools"),
            item_snippet_fmt("max_turns", CompletionItemKind::PROPERTY, "max_turns: ${1:10}", "Maximum conversation turns.", "3_max_turns"),
            item_snippet_fmt("depth_limit", CompletionItemKind::PROPERTY, "depth_limit: ${1:3}", "Max spawn_agent recursion depth.", "3_depth_limit"),
            item_snippet_fmt("provider", CompletionItemKind::PROPERTY, "provider: ${1|claude,openai,mistral,groq,deepseek,gemini,xai|}", "Provider override.", "4_provider"),
            item_snippet_fmt("model", CompletionItemKind::PROPERTY, "model: ${1}", "Model override.", "4_model"),
            item_snippet_fmt("extended_thinking", CompletionItemKind::PROPERTY, "extended_thinking: true\nthinking_budget: ${1:8192}", "Enable extended thinking.", "5_extended_thinking"),
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
        // Suggest transform filters.
        return vec![
            item_value("upper", "Convert to uppercase.", "0_upper"),
            item_value("lower", "Convert to lowercase.", "1_lower"),
            item_value("trim", "Trim whitespace.", "2_trim"),
            item_value("json", "Parse as JSON.", "3_json"),
            item_value("length", "Get length.", "4_length"),
            item_value("default(\"\")", "Default if empty.", "5_default"),
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
                item_snippet_fmt("mcp", CompletionItemKind::PROPERTY, "mcp: ${1:server}", "MCP server name.", "0_mcp"),
                item_snippet_fmt("tool", CompletionItemKind::PROPERTY, "tool: ${1:tool-name}", "MCP tool name.", "1_tool"),
                item_snippet_fmt("params", CompletionItemKind::PROPERTY, "params:\n  ${1:key}: ${2:value}", "Tool parameters.", "2_params"),
                item_snippet_fmt("resource", CompletionItemKind::PROPERTY, "resource: ${1:resource-uri}", "MCP resource URI.", "3_resource"),
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
fn provider_completions(prefix: &str, current_provider: Option<&str>) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // LLM providers from the catalog.
    for provider in KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
    {
        items.push(CompletionItem {
            label: provider.id.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            insert_text: Some(provider.id.to_string()),
            detail: Some(provider.name.to_string()),
            documentation: Some(Documentation::String(provider.description.to_string())),
            sort_text: Some(format!("0_{}", provider.id)),
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
                item_snippet_fmt("text", CompletionItemKind::PROPERTY, "text: ${1}", "Text content.", "0_text"),
                item_snippet_fmt("source", CompletionItemKind::PROPERTY, "source: ${1}", "CAS hash or file path.", "1_source"),
                item_snippet_fmt("url", CompletionItemKind::PROPERTY, "url: ${1:https://}", "Image URL.", "1_url"),
                item_snippet_fmt("detail", CompletionItemKind::PROPERTY, "detail: ${1|auto,low,high|}", "Image detail level.", "2_detail"),
            ];
            filter_by_prefix(items, prefix)
        }
    }
}

/// Completions for `for_each:` block.
fn for_each_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt("items", CompletionItemKind::PROPERTY, "items: [${1}]", "Array to iterate.", "0_items"),
        item_snippet_fmt("as", CompletionItemKind::PROPERTY, "as: ${1:item}", "Loop variable name.", "1_as"),
        item_snippet_fmt("concurrency", CompletionItemKind::PROPERTY, "concurrency: ${1:3}", "Max parallel iterations.", "2_concurrency"),
    ];
    filter_by_prefix(items, prefix)
}

/// Completions for `structured:` / schema block.
fn schema_block_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt("type", CompletionItemKind::PROPERTY, "type: ${1|object,array,string,number,boolean|}", "JSON Schema type.", "0_type"),
        item_snippet_fmt("properties", CompletionItemKind::PROPERTY, "properties:\n  ${1:field}:\n    type: ${2:string}", "Object properties.", "1_properties"),
        item_snippet_fmt("required", CompletionItemKind::PROPERTY, "required: [${1}]", "Required properties.", "2_required"),
        item_snippet_fmt("items", CompletionItemKind::PROPERTY, "items:\n  type: ${1:string}", "Array item schema.", "3_items"),
        item_snippet_fmt("description", CompletionItemKind::PROPERTY, "description: \"${1}\"", "Field description.", "4_description"),
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
            documentation: Some(Documentation::String(format!(
                "Add '{id}' as a dependency"
            ))),
            sort_text: Some(format!("0_{id}")),
            ..Default::default()
        })
        .collect()
}

/// Completions for `guardrails:` block.
fn guardrails_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt("input", CompletionItemKind::PROPERTY, "input: ${1:rule}", "Input guardrail.", "0_input"),
        item_snippet_fmt("output", CompletionItemKind::PROPERTY, "output: ${1:rule}", "Output guardrail.", "1_output"),
        item_snippet_fmt("max_length", CompletionItemKind::PROPERTY, "max_length: ${1:4096}", "Maximum output length.", "2_max_length"),
        item_snippet_fmt("forbidden_topics", CompletionItemKind::PROPERTY, "forbidden_topics: [${1}]", "Topics to block.", "3_forbidden"),
    ];
    filter_by_prefix(items, prefix)
}

/// Completions for `retry:` block.
fn retry_block_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt("max_attempts", CompletionItemKind::PROPERTY, "max_attempts: ${1:3}", "Maximum retry attempts.", "0_max_attempts"),
        item_snippet_fmt("delay", CompletionItemKind::PROPERTY, "delay: ${1:1s}", "Delay between retries.", "1_delay"),
        item_snippet_fmt("backoff", CompletionItemKind::PROPERTY, "backoff: ${1|exponential,linear,fixed|}", "Backoff strategy.", "2_backoff"),
    ];
    filter_by_prefix(items, prefix)
}

/// Completions for `limits:` / `timeout:` block.
fn limits_block_completions(prefix: &str) -> Vec<CompletionItem> {
    let items = vec![
        item_snippet_fmt("timeout", CompletionItemKind::PROPERTY, "timeout: ${1:30}", "Timeout in seconds.", "0_timeout"),
        item_snippet_fmt("max_tokens", CompletionItemKind::PROPERTY, "max_tokens: ${1:4096}", "Max output tokens.", "1_max_tokens"),
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
        let ctx = detect_context(text, offset as u32, None);
        completions(text, offset as u32, &ctx)
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
        assert!(items.iter().any(|i| i.label == "content"), "Missing content field");
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
        assert!(items.iter().any(|i| i.label == "temperature"), "Missing temperature");
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
        assert!(items.iter().any(|i| i.label == "max_turns"), "Missing max_turns");
        assert!(items.iter().any(|i| i.label == "extended_thinking"), "Missing extended_thinking");
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
        let items = provider_completions("", None);
        assert!(items.iter().any(|i| i.label == "anthropic"), "Missing anthropic");
        assert!(items.iter().any(|i| i.label == "openai"), "Missing openai");
        assert!(items.iter().any(|i| i.label == "mistral"), "Missing mistral");
        assert!(items.iter().any(|i| i.label == "groq"), "Missing groq");
        assert!(items.iter().any(|i| i.label == "deepseek"), "Missing deepseek");
        assert!(items.iter().any(|i| i.label == "gemini"), "Missing gemini");
        assert!(items.iter().any(|i| i.label == "xai"), "Missing xai");
    }

    #[test]
    fn provider_completions_include_aliases() {
        let items = provider_completions("", None);
        assert!(items.iter().any(|i| i.label == "claude"), "Missing claude alias");
        assert!(items.iter().any(|i| i.label == "gpt"), "Missing gpt alias");
    }

    #[test]
    fn provider_completions_include_local_models() {
        let items = provider_completions("", None);
        // Should include known local models from catalog.
        assert!(
            items.iter().any(|i| i.label == "qwen3:8b"),
            "Missing qwen3:8b from model catalog"
        );
    }

    #[test]
    fn provider_completions_filter_by_prefix() {
        let items = provider_completions("an", None);
        assert!(items.iter().any(|i| i.label == "anthropic"));
        assert!(!items.iter().any(|i| i.label == "openai"));
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
        assert!(items.iter().any(|i| i.label == "json"));
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
        let items = completions("", 0, &ctx);
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
}
