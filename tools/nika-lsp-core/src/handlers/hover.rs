//! Hover handler — rich documentation on hover.
//!
//! Protocol-agnostic: returns `HoverResult` with markdown content.
//! The tower-lsp shim in each binary converts to `ls_types::Hover`.

use crate::analysis::context::CursorContext;

/// Protocol-agnostic hover result.
#[derive(Debug, Clone)]
pub struct HoverResult {
    /// Markdown content for the hover tooltip.
    pub contents: String,
    /// Optional highlight range (start_offset, end_offset) in the document.
    pub range: Option<(u32, u32)>,
}

/// Compute hover documentation for the given cursor context.
///
/// Returns rich markdown documentation for verbs, fields, bindings,
/// templates, root keys, and content parts.
pub fn hover(_text: &str, _offset: u32, context: &CursorContext) -> Option<HoverResult> {
    match context {
        CursorContext::VerbBlock { ref verb, .. } => verb_hover(verb),
        CursorContext::TaskField { prefix, .. } => field_hover(prefix),
        CursorContext::WorkflowRoot { prefix } => root_key_hover(prefix),
        CursorContext::ContentPart { focus, .. } => content_hover(focus),
        CursorContext::WithBlock { alias, .. } => {
            if let Some(alias) = alias {
                Some(HoverResult {
                    contents: format!(
                        "## Binding: `{}`\n\n\
                        References data from another task.\n\n\
                        Access via `{{{{with.{}}}}}` in prompts.",
                        alias, alias
                    ),
                    range: None,
                })
            } else {
                Some(HoverResult {
                    contents: "## `with:` — Data Bindings\n\n\
                        Bind outputs from upstream tasks.\n\n\
                        ```yaml\nwith:\n  result: $step1       # Bind step1's output\n  lazy_val:            # Lazy binding\n    path: future_task\n    lazy: true\n    default: \"fallback\"\n```\n\n\
                        Access via `{{with.alias}}` in prompts."
                        .to_string(),
                    range: None,
                })
            }
        }
        CursorContext::DependsOn { .. } => Some(HoverResult {
            contents: "## `depends_on:` — Execution Dependencies\n\n\
                Pure ordering edges. Task waits for listed tasks to complete.\n\n\
                ```yaml\ndepends_on: [step1, step2]\n```\n\n\
                Note: `with:` bindings imply `depends_on` automatically."
                .to_string(),
            range: None,
        }),
        CursorContext::Template {
            in_transform_chain: true,
            partial_expr,
            ..
        } => transform_hover(partial_expr),
        CursorContext::Template {
            partial_expr,
            in_transform_chain: false,
            ..
        } => template_hover(partial_expr),
        CursorContext::ForEach { .. } => Some(HoverResult {
            contents: FOREACH_DOC.to_string(),
            range: None,
        }),
        CursorContext::InvokeBlock { .. } => verb_hover("invoke"),
        CursorContext::McpConfig { .. } => Some(HoverResult {
            contents: MCP_DOC.to_string(),
            range: None,
        }),
        CursorContext::ProviderContext { .. } => Some(HoverResult {
            contents: PROVIDER_DOC.to_string(),
            range: None,
        }),
        CursorContext::SchemaBlock { .. } => field_hover("schema:"),
        CursorContext::RetryBlock { .. } => Some(HoverResult {
            contents: "## `retry:` — Retry Policy\n\n\
                Retry failed tasks with configurable backoff.\n\n\
                ```yaml\nretry:\n  max_attempts: 3\n  delay: 2\n  backoff: exponential\n```"
                .to_string(),
            range: None,
        }),
        CursorContext::LimitsBlock { .. } => Some(HoverResult {
            contents: "## `limits:` — Resource Limits\n\n\
                Constrain task resource usage.\n\n\
                ```yaml\nlimits:\n  max_tokens: 4096\n  max_cost: 0.10\n```"
                .to_string(),
            range: None,
        }),
        CursorContext::Guardrails { .. } => Some(HoverResult {
            contents: "## `guard:` — Output Guardrails\n\n\
                Validate task output with rules.\n\n\
                ```yaml\nguard:\n  - rule: \"Output must be valid JSON\"\n    on_fail: error\n```\n\n\
                **on_fail:** `error` (default), `warn`, `retry`"
                .to_string(),
            range: None,
        }),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Verb Documentation
// ═══════════════════════════════════════════════════════════════════════════

fn verb_hover(verb: &str) -> Option<HoverResult> {
    let doc = match verb {
        "infer" => {
            "## `infer:` — LLM Generation\n\n\
            Generates text using an LLM provider.\n\n\
            **Shorthand:**\n\
            ```yaml\ninfer: \"Generate a headline\"\n```\n\n\
            **Full form:**\n\
            ```yaml\ninfer:\n  prompt: \"Generate a headline\"\n  model: claude-sonnet-4-6\n  temperature: 0.7\n  system: \"You are a copywriter\"\n  max_tokens: 100\n  extended_thinking: true\n  thinking_budget: 8192\n```\n\n\
            **Vision:** Use `content:` for multimodal (text + image) inputs."
        }
        "exec" => {
            "## `exec:` — Shell Command\n\n\
            Executes a shell command.\n\n\
            **Shorthand:**\n\
            ```yaml\nexec: \"npm run build\"\n```\n\n\
            **Full form:**\n\
            ```yaml\nexec:\n  command: \"npm run build\"\n  shell: false  # Default: shlex parsing (secure)\n  timeout: 30\n  cwd: ./project\n```\n\n\
            **Security:** `shell: false` (default) prevents shell injection."
        }
        "fetch" => {
            "## `fetch:` — HTTP Request\n\n\
            Makes an HTTP request.\n\n\
            ```yaml\nfetch:\n  url: \"https://api.example.com/data\"\n  method: GET\n  headers:\n    Authorization: \"Bearer $TOKEN\"\n  extract: markdown\n  response: full\n```\n\n\
            ### Extract Modes (9)\n\
            `markdown` · `article` · `text` · `selector` · `metadata` · `links` · `jsonpath` · `feed` · `llm_txt`\n\n\
            ### Response Modes\n\
            `full` (JSON with status/headers/body) · `binary` (CAS hash)"
        }
        "invoke" => {
            "## `invoke:` — MCP Tool Call\n\n\
            Calls a tool on an MCP server.\n\n\
            ```yaml\ninvoke:\n  mcp: novanet\n  tool: novanet_context\n  params:\n    mode: \"page\"\n    focus_key: \"qr-code\"\n```\n\n\
            **Builtin tools:** `nika:sleep`, `nika:log`, `nika:emit`, `nika:assert`, `nika:import`, `nika:thumbnail`, ..."
        }
        "agent" => {
            "## `agent:` — Agentic Loop\n\n\
            Runs a multi-turn agent with tool access.\n\n\
            ```yaml\nagent:\n  prompt: \"Research and summarize\"\n  model: claude-sonnet-4-6\n  mcp: [novanet, perplexity]\n  max_turns: 10\n  depth_limit: 3\n  tools: [nika:read, nika:write]\n  extended_thinking: true\n```"
        }
        _ => return None,
    };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Field Documentation
// ═══════════════════════════════════════════════════════════════════════════

fn field_hover(prefix: &str) -> Option<HoverResult> {
    let key = prefix.trim().trim_end_matches(':');
    let doc = match key {
        "id" => "## `id:` — Task Identifier\n\nUnique identifier for the task. Used in bindings and `depends_on:`.\n\n```yaml\n- id: my_task\n  infer: \"...\"\n```",
        "with" => "## `with:` — Data Bindings\n\nBind outputs from upstream tasks to local aliases.\n\n```yaml\nwith:\n  result: $step1\n  data: $fetch_api\n```\n\nAccess via `{{with.alias}}` in prompts.",
        "depends_on" => "## `depends_on:` — Execution Dependencies\n\nPure ordering edges. Task waits for listed tasks.\n\n```yaml\ndepends_on: [step1, step2]\n```\n\nNote: `with:` bindings imply `depends_on` automatically.",
        "content" => "## `content:` — Vision Content\n\nMultimodal parts (text + image) for vision-capable LLMs.\n\n```yaml\ncontent:\n  - type: image\n    source: \"{{with.photo.media[0].hash}}\"\n    detail: high\n  - type: text\n    text: \"Describe this image\"\n```",
        "for_each" => FOREACH_DOC,
        "timeout" => "## `timeout:` — Task Timeout\n\nMaximum execution time **in seconds**.\n\n```yaml\ntimeout: 30  # 30 seconds\n```",
        "retry" => "## `retry:` — Retry Policy\n\nRetry failed tasks with configurable backoff.\n\n```yaml\nretry:\n  max_attempts: 3\n  delay: 2\n  backoff: exponential\n```",
        "guard" => "## `guard:` — Output Guardrails\n\nValidate task output with rules.\n\n```yaml\nguard:\n  - rule: \"Output must be valid JSON\"\n    on_fail: error\n```",
        "output" => "## `output:` — Output Format\n\nControl task output format.\n\n```yaml\noutput:\n  format: json\n```",
        "structured" => "## `structured:` — JSON Schema Output\n\nForce LLM to output valid JSON matching a schema.\n\n```yaml\nstructured:\n  schema:\n    type: object\n    properties:\n      title: { type: string }\n    required: [\"title\"]\n```",
        "on_error" => "## `on_error:` — Error Handling\n\nControl behavior when task fails.\n\n`continue` · `fail` (default) · `skip`",
        "as" => "## `as:` — Loop Variable Name\n\nName for the current iteration item in `for_each:`.\n\n```yaml\nfor_each: [1, 2, 3]\nas: num\n```\n\nAccess via `{{item}}` or `{{num}}` in the task.",
        "concurrency" => "## `concurrency:` — Parallel Limit\n\nMax parallel iterations for `for_each:` loops.\n\n```yaml\nconcurrency: 5\n```",
        "fail_fast" => "## `fail_fast:` — Stop on First Failure\n\nAbort remaining iterations if one fails.\n\n```yaml\nfail_fast: true\n```",
        "description" => "## `description:` — Workflow Description\n\nHuman-readable description of the workflow.",
        _ => return None,
    };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Root Key Documentation
// ═══════════════════════════════════════════════════════════════════════════

fn root_key_hover(prefix: &str) -> Option<HoverResult> {
    let key = prefix.trim().trim_end_matches(':');
    let doc = match key {
        "schema" => "## `schema:` — Workflow Schema Version\n\nDeclares the Nika workflow schema version.\n\n```yaml\nschema: \"@0.12\"\n```\n\n**Current version:** `@0.12`",
        "workflow" => "## `workflow:` — Workflow Name\n\nHuman-readable name for the workflow.\n\n```yaml\nworkflow: my-pipeline\n```",
        "tasks" => "## `tasks:` — Task List\n\nArray of tasks forming the workflow DAG.\n\n```yaml\ntasks:\n  - id: step1\n    infer: \"Generate\"\n  - id: step2\n    depends_on: [step1]\n    exec: \"echo done\"\n```",
        "mcp" => MCP_DOC,
        "context" => "## `context:` — File Loading\n\nLoad files at workflow start.\n\n```yaml\ncontext:\n  files:\n    brand: ./context/brand.md\n  session: .nika/sessions/prev.json\n```\n\nAccess via `{{context.files.alias}}`.",
        "include" => "## `include:` — DAG Fusion\n\nMerge tasks from external workflows.\n\n```yaml\ninclude:\n  - path: ./partials/setup.nika.yaml\n    prefix: setup_\n```",
        "provider" => PROVIDER_DOC,
        "inputs" => "## `inputs:` — Workflow Parameters\n\nDeclare input parameters for the workflow.\n\n```yaml\ninputs:\n  topic:\n    type: string\n    description: \"Topic to research\"\n    default: \"AI\"\n```\n\nAccess via `{{inputs.topic}}`.",
        "edges" => "## `edges:` — Explicit DAG Edges\n\nDeclare edges between tasks explicitly.\n\n```yaml\nedges:\n  - from: step1\n    to: step2\n```",
        _ => return None,
    };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Content / Template / Transform Hover
// ═══════════════════════════════════════════════════════════════════════════

fn content_hover(focus: &crate::analysis::context::ContentFocus) -> Option<HoverResult> {
    use crate::analysis::context::ContentFocus;
    let doc = match focus {
        ContentFocus::PartType => "## Content Type\n\n`text` · `image` · `image_url`\n\nUse `image` with CAS hashes, `image_url` with URLs.",
        ContentFocus::ImageDetail => "## Image Detail\n\n`auto` · `low` · `high`\n\nControls image resolution sent to the LLM.",
        ContentFocus::ImageUrl => "## Image URL\n\nDirect URL to an image for vision-capable LLMs.",
        ContentFocus::PartField => "## Content Part Field\n\nField within a content part (type, source, detail, text).",
    };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

fn template_hover(expr: &str) -> Option<HoverResult> {
    let trimmed = expr.trim();
    if trimmed.starts_with("with.") {
        let alias = trimmed.strip_prefix("with.").unwrap_or(trimmed);
        let top = alias.split('.').next().unwrap_or(alias);
        Some(HoverResult {
            contents: format!(
                "## Binding Reference: `{}`\n\n\
                Data from task bound via `with:`.\n\n\
                ```yaml\nwith:\n  {}: $source_task\n```",
                top, top
            ),
            range: None,
        })
    } else if trimmed.starts_with("context.") {
        Some(HoverResult {
            contents: "## Context File Reference\n\nData loaded via `context:` block at workflow start.".to_string(),
            range: None,
        })
    } else if trimmed.starts_with("inputs.") {
        Some(HoverResult {
            contents: "## Input Parameter\n\nWorkflow input declared in `inputs:` block.".to_string(),
            range: None,
        })
    } else if trimmed.starts_with("item") || trimmed.starts_with("index") {
        Some(HoverResult {
            contents: "## Loop Variable\n\nCurrent item/index from `for_each:` iteration.".to_string(),
            range: None,
        })
    } else {
        Some(HoverResult {
            contents: "## Template Expression\n\nAccess data: `{{with.alias}}`, `{{inputs.param}}`, `{{context.files.name}}`".to_string(),
            range: None,
        })
    }
}

fn transform_hover(expr: &str) -> Option<HoverResult> {
    let t = expr.rsplit('|').next()?.trim();
    let doc = match t {
        "upper" | "uppercase" => "`uppercase` — Convert to UPPERCASE",
        "lower" | "lowercase" => "`lowercase` — Convert to lowercase",
        "trim" => "`trim` — Strip leading/trailing whitespace",
        "trim_start" => "`trim_start` — Strip leading whitespace",
        "trim_end" => "`trim_end` — Strip trailing whitespace",
        "length" | "len" => "`length` — Count characters",
        "reverse" => "`reverse` — Reverse string",
        "base64_encode" => "`base64_encode` — Encode as Base64",
        "base64_decode" => "`base64_decode` — Decode from Base64",
        "url_encode" => "`url_encode` — URL-encode string",
        "url_decode" => "`url_decode` — URL-decode string",
        "json" | "to_json" => "`json` — Serialize to JSON string",
        "from_json" => "`from_json` — Parse JSON string to object",
        "lines" => "`lines` — Split into array of lines",
        "words" => "`words` — Split into array of words",
        "first" => "`first` — First element of array",
        "last" => "`last` — Last element of array",
        "flatten" => "`flatten` — Flatten nested arrays",
        "unique" => "`unique` — Remove duplicates",
        "sort" => "`sort` — Sort array",
        "count" => "`count` — Count array elements",
        "sum" => "`sum` — Sum numeric array",
        "min" => "`min` — Minimum value",
        "max" => "`max` — Maximum value",
        "join" => "`join` — Join array with separator",
        "split" => "`split` — Split string into array",
        "replace" => "`replace` — Replace substring",
        "default" => "`default` — Fallback value if null/empty",
        _ => return None,
    };
    Some(HoverResult {
        contents: format!("## Pipe Transform\n\n{}\n\n```yaml\n{{{{with.data | {}}}}}\n```", doc, t),
        range: None,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared Documentation Constants
// ═══════════════════════════════════════════════════════════════════════════

const FOREACH_DOC: &str = "## `for_each:` — Parallel Iteration\n\n\
    Execute task for each item in an array.\n\n\
    ```yaml\nfor_each: [\"fr-FR\", \"en-US\", \"de-DE\"]\nas: locale\nconcurrency: 5\nfail_fast: true\ninfer: \"Generate for {{with.locale}}\"\n```";

const MCP_DOC: &str = "## `mcp:` — MCP Server Configuration\n\n\
    Configure MCP servers for `invoke:` and `agent:` tasks.\n\n\
    ```yaml\nmcp:\n  novanet:\n    command: node\n    args: [\"./dist/index.js\"]\n    env:\n      NEO4J_URI: \"bolt://localhost:7687\"\n```";

const PROVIDER_DOC: &str = "## `provider:` — Default LLM Provider\n\n\
    Set the default LLM provider for the workflow.\n\n\
    ```yaml\nprovider: anthropic\n```\n\n\
    **Providers:** `anthropic` · `openai` · `mistral` · `groq` · `deepseek` · `gemini` · `xai` · `native`\n\n\
    For local inference, use `provider: native` with mistral.rs.";

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_verbs_have_hover() {
        for v in ["infer", "exec", "fetch", "invoke", "agent"] {
            let result = verb_hover(v);
            assert!(result.is_some(), "Missing hover for verb: {}", v);
            assert!(
                result.unwrap().contents.len() > 50,
                "Hover too short for verb: {}",
                v
            );
        }
    }

    #[test]
    fn unknown_verb_returns_none() {
        assert!(verb_hover("unknown").is_none());
    }

    #[test]
    fn all_fields_have_hover() {
        for f in [
            "id",
            "with",
            "depends_on",
            "content",
            "for_each",
            "timeout",
            "retry",
            "guard",
            "output",
            "structured",
            "on_error",
            "as",
            "concurrency",
            "fail_fast",
            "description",
        ] {
            let result = field_hover(f);
            assert!(result.is_some(), "Missing hover for field: {}", f);
        }
    }

    #[test]
    fn all_root_keys_have_hover() {
        for k in [
            "schema", "workflow", "tasks", "mcp", "context", "include", "provider", "inputs",
            "edges",
        ] {
            let result = root_key_hover(k);
            assert!(result.is_some(), "Missing hover for root key: {}", k);
        }
    }

    #[test]
    fn content_hover_all_variants() {
        use crate::analysis::context::ContentFocus;
        for focus in [
            ContentFocus::PartType,
            ContentFocus::ImageDetail,
            ContentFocus::ImageUrl,
            ContentFocus::PartField,
        ] {
            assert!(
                content_hover(&focus).is_some(),
                "Missing hover for content focus: {:?}",
                focus
            );
        }
    }

    #[test]
    fn template_hover_binding() {
        let r = template_hover("with.result").unwrap();
        assert!(r.contents.contains("Binding Reference"));
    }

    #[test]
    fn template_hover_context() {
        let r = template_hover("context.files.brand").unwrap();
        assert!(r.contents.contains("Context File"));
    }

    #[test]
    fn template_hover_inputs() {
        let r = template_hover("inputs.topic").unwrap();
        assert!(r.contents.contains("Input Parameter"));
    }

    #[test]
    fn template_hover_loop_var() {
        let r = template_hover("item").unwrap();
        assert!(r.contents.contains("Loop Variable"));
    }

    #[test]
    fn transform_hover_common() {
        for t in ["upper", "lower", "trim", "json", "length", "first", "last", "sort"] {
            let expr = format!("with.data | {}", t);
            let r = transform_hover(&expr);
            assert!(r.is_some(), "Missing transform hover for: {}", t);
        }
    }

    #[test]
    fn transform_hover_unknown_returns_none() {
        assert!(transform_hover("with.data | nonexistent").is_none());
    }

    #[test]
    fn hover_with_context_verb() {
        let ctx = CursorContext::VerbBlock {
            task_id: None,
            verb: "infer".to_string(),
            existing_subfields: vec![],
            prefix: String::new(),
        };
        let r = hover("", 0, &ctx);
        assert!(r.is_some());
        assert!(r.unwrap().contents.contains("LLM Generation"));
    }

    #[test]
    fn hover_with_context_unknown() {
        let ctx = CursorContext::Unknown {
            prefix: String::new(),
        };
        assert!(hover("", 0, &ctx).is_none());
    }

    #[test]
    fn hover_depends_on() {
        let ctx = CursorContext::DependsOn {
            task_id: None,
            existing_deps: vec![],
            prefix: String::new(),
        };
        let r = hover("", 0, &ctx).unwrap();
        assert!(r.contents.contains("Execution Dependencies"));
    }

    #[test]
    fn hover_retry() {
        let ctx = CursorContext::RetryBlock {
            task_id: None,
            prefix: String::new(),
        };
        let r = hover("", 0, &ctx).unwrap();
        assert!(r.contents.contains("Retry Policy"));
    }

    #[test]
    fn hover_guardrails() {
        let ctx = CursorContext::Guardrails {
            task_id: None,
            guardrail_type: None,
            prefix: String::new(),
        };
        let r = hover("", 0, &ctx).unwrap();
        assert!(r.contents.contains("Guardrails"));
    }
}
