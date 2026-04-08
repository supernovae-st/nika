// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hover handler — rich documentation on hover.
//!
//! Protocol-agnostic: returns `HoverResult` with markdown content.
//! The tower-lsp shim in each binary converts to `ls_types::Hover`.

use nika_core::catalogs::WorkflowRunInfo;

use crate::analysis::context::CursorContext;

/// Protocol-agnostic hover result.
#[derive(Debug, Clone)]
pub struct HoverResult {
    /// Markdown content for the hover tooltip.
    pub contents: String,
    /// Optional highlight range (start_offset, end_offset) in the document.
    pub range: Option<(u32, u32)>,
}

/// Optional daemon data for enriched hover.
pub struct DaemonHoverData<'a> {
    /// Recent workflow runs (from GetWorkflowHistory).
    pub workflow_history: &'a [WorkflowRunInfo],
}

/// Compute hover documentation for the given cursor context.
///
/// Returns rich markdown documentation for verbs, fields, bindings,
/// templates, root keys, and content parts.
/// `daemon_data` optionally enriches hover with live run history.
pub fn hover(
    _text: &str,
    _offset: u32,
    context: &CursorContext,
    daemon_data: Option<&DaemonHoverData<'_>>,
) -> Option<HoverResult> {
    match context {
        CursorContext::VerbBlock {
            ref verb,
            ref prefix,
            ..
        } => {
            // Try sub-field hover first (e.g., "prompt:" inside infer:)
            let sub_key = prefix.trim().trim_end_matches(':');
            if !sub_key.is_empty() {
                if let Some(r) = verb_subfield_hover(verb, sub_key) {
                    return Some(r);
                }
            }
            verb_hover(verb)
        }
        CursorContext::TaskField { prefix, .. } => field_hover(prefix),
        CursorContext::WorkflowRoot { prefix } => {
            let mut result = root_key_hover(prefix)?;
            // Enrich with workflow run history if available
            if prefix.trim().trim_end_matches(':') == "workflow" {
                if let Some(data) = daemon_data {
                    if !data.workflow_history.is_empty() {
                        result.contents.push_str("\n\n---\n\n**Recent runs:**\n");
                        for run in data.workflow_history.iter().take(3) {
                            let icon = match run.exit_code {
                                Some(0) => "\u{2713}", // ✓
                                Some(_) => "\u{2717}", // ✗
                                None => "\u{23F3}",    // ⏳
                            };
                            result.contents.push_str(&format!(
                                "\n- {} {} \u{2014} {}",
                                icon, run.state, run.created_at
                            ));
                        }
                    }
                }
            }
            Some(result)
        }
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
            contents: "## `guardrails:` — Output Guardrails\n\n\
                Validate LLM output. 4 types: `length`, `schema`, `regex`, `llm`.\n\n\
                ```yaml\nguardrails:\n  - type: length\n    max_words: 500\n    on_failure: retry\n```\n\n\
                **on_failure:** `retry` (default), `escalate`, `fail`"
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
            Calls a tool on an MCP server, or reads a resource.\n\n\
            ```yaml\ninvoke:\n  mcp: novanet\n  tool: novanet_context\n  params:\n    mode: \"page\"\n```\n\n\
            **Resource read:**\n\
            ```yaml\ninvoke:\n  mcp: novanet\n  resource: \"novanet://entity/qr-code\"\n```\n\n\
            **Builtin tools:** `nika:sleep`, `nika:log`, `nika:emit`, `nika:assert`, `nika:import`, `nika:thumbnail`, ..."
        }
        "agent" => {
            "## `agent:` — Agentic Loop\n\n\
            Runs a multi-turn agent with tool access.\n\n\
            ```yaml\nagent:\n  prompt: \"Research and summarize\"\n  model: claude-sonnet-4-6\n  mcp: [novanet, perplexity]\n  max_turns: 10\n  depth_limit: 3\n  tools: [nika:read, nika:write]\n  extended_thinking: true\n  skills: [research, summarize]\n  guardrails:\n    - type: length\n      max_words: 500\n  completion:\n    mode: explicit\n  limits:\n    max_cost_usd: 0.50\n```"
        }
        _ => return None,
    };
    Some(HoverResult {
        contents: doc.to_string(),
        range: None,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Verb Sub-field Documentation
// ═══════════════════════════════════════════════════════════════════════════

fn verb_subfield_hover(verb: &str, key: &str) -> Option<HoverResult> {
    let doc = match key {
        "prompt" => match verb {
            "infer" => "## `prompt:` — Text Prompt\n\nThe text sent to the LLM. Supports `{{with.alias}}` templates.\n\nOptional when `content:` is present (vision/multimodal).",
            "agent" => "## `prompt:` — Agent Goal\n\nThe goal/instruction for the agentic loop.\n\nThe agent will use tools to accomplish this goal.",
            _ => return None,
        },
        "system" => "## `system:` — System Prompt\n\nSystem-level instructions for the LLM.\n\n```yaml\nsystem: |\n  You are a senior researcher.\n  Always cite sources.\n```",
        "model" => "## `model:` — Model Override\n\nOverride the default model for this verb.\n\n**Claude:** `claude-sonnet-4-6` · `claude-opus-4` · `claude-haiku-4-5`\n\n**OpenAI:** `gpt-4o` · `gpt-4o-mini` · `o1` · `o3`\n\n**Others:** `mistral-large` · `gemma-3-27b` · `deepseek-chat`",
        "temperature" => "## `temperature:` — Sampling Temperature\n\nControls randomness. Range: `0.0` (deterministic) to `2.0` (creative).\n\n| Value | Use Case |\n|-------|----------|\n| 0.0 | Factual extraction |\n| 0.3-0.5 | Analysis, summaries |\n| 0.7 | General creative |\n| 1.0+ | Brainstorming |",
        "max_tokens" => "## `max_tokens:` — Maximum Output Tokens\n\nLimit the LLM response length.\n\n| Model | Max Context |\n|-------|-------------|\n| claude-sonnet-4-6 | 8,192 output |\n| gpt-4o | 16,384 output |\n| o1 | 100,000 output |",
        "extended_thinking" | "thinking" => "## `extended_thinking:` — Chain-of-Thought Reasoning\n\n**Claude-only.** Enables step-by-step reasoning.\n\n```yaml\nextended_thinking: true\nthinking_budget: 8192\n```\n\nRequires a Claude model. Other providers will error (NIKA-032).",
        "thinking_budget" => "## `thinking_budget:` — Thinking Token Budget\n\nMax tokens for the thinking/reasoning phase.\n\nOnly used when `extended_thinking: true`.",
        "command" => "## `command:` — Shell Command\n\nThe command to execute.\n\n```yaml\ncommand: \"npm run build\"\n```\n\nWith `shell: false` (default), uses secure shlex parsing.",
        "shell" => "## `shell:` — Shell Mode\n\n`true` — Run via `sh -c` (allows pipes, redirects, env expansion)\n\n`false` (default) — Secure shlex parsing (prevents injection)",
        "cwd" | "working_dir" => "## `cwd:` — Working Directory\n\nSet the working directory for the command.\n\n```yaml\ncwd: ./project\n```",
        "env" => "## `env:` — Environment Variables\n\nEnvironment variables passed to the command.\n\n```yaml\nenv:\n  NODE_ENV: production\n  API_KEY: $SECRET\n```",
        "url" => "## `url:` — Request URL\n\n**Required.** The HTTP URL to fetch.\n\n```yaml\nurl: \"https://api.example.com/data\"\n```",
        "method" => "## `method:` — HTTP Method\n\n`GET` (default) · `POST` · `PUT` · `DELETE` · `PATCH` · `HEAD` · `OPTIONS`",
        "headers" => "## `headers:` — HTTP Headers\n\n```yaml\nheaders:\n  Authorization: \"Bearer {{inputs.token}}\"\n  Content-Type: application/json\n```",
        "body" => "## `body:` — Request Body\n\nRaw string body for POST/PUT requests.\n\nFor JSON, prefer `json:` which auto-serializes.",
        "json" => "## `json:` — JSON Request Body\n\nJSON object auto-serialized as request body.\n\n```yaml\njson:\n  query: \"{{with.topic}}\"\n  limit: 10\n```",
        "extract" => "## `extract:` — Post-Processing Mode\n\n9 extraction modes for HTML/JSON responses:\n\n| Mode | Description |\n|------|-------------|\n| `markdown` | Clean Markdown via htmd |\n| `article` | Main content via Readability |\n| `text` | Visible text (+ optional `selector:`) |\n| `selector` | Raw HTML by CSS selector |\n| `metadata` | OG, Twitter Cards, JSON-LD |\n| `links` | Link classification |\n| `jsonpath` | JSONPath query (use `selector:`) |\n| `feed` | RSS/Atom/JSON Feed parsing |\n| `llm_txt` | AI-era .well-known/llm.txt |",
        "selector" => "## `selector:` — CSS/JSONPath Selector\n\nUsed with `extract: text`, `extract: selector`, or `extract: jsonpath`.\n\n```yaml\nextract: selector\nselector: \"article.main h1\"\n```",
        "response" => "## `response:` — Response Mode\n\n`full` — JSON with status, headers, body, final URL\n\n`binary` — Store in CAS, return hash for media pipeline",
        "follow_redirects" => "## `follow_redirects:` — Follow Redirects\n\n`true` (default) — Follow HTTP 3xx redirects\n\n`false` — Return redirect response directly",
        "tool" => "## `tool:` — MCP Tool Name\n\n**Required.** Name of the MCP tool to invoke.\n\n```yaml\ntool: novanet_context\n```\n\n**Builtin tools:** `nika:sleep` · `nika:log` · `nika:import` · `nika:thumbnail` · ...",
        "mcp" => "## `mcp:` — MCP Server\n\nThe MCP server providing the tool.\n\n```yaml\nmcp: novanet\n```\n\nFor agents: array of servers `mcp: [novanet, perplexity]`",
        "params" => "## `params:` — Tool Parameters\n\nJSON parameters passed to the MCP tool.\n\n```yaml\nparams:\n  mode: page\n  entity_id: qr-code\n```",
        "resource" => "## `resource:` — MCP Resource URI\n\nRead a resource (mutually exclusive with `tool:`).\n\n```yaml\nresource: \"novanet://entity/qr-code\"\n```",
        "tools" => "## `tools:` — Agent Tools\n\nBuiltin tools available to the agent.\n\n```yaml\ntools: [nika:read, nika:write, nika:edit]\n```",
        "skills" => "## `skills:` — Agent Skills\n\nSkills to inject into the agent's system prompt.\n\n```yaml\nskills: [research, summarize]\n```\n\nMust be defined in workflow-level `skills:` block.",
        "max_turns" | "max_iterations" => "## `max_turns:` — Maximum Iterations\n\nMax conversation turns before stopping the agent loop.\n\n```yaml\nmax_turns: 10\n```",
        "depth_limit" => "## `depth_limit:` — Recursion Depth\n\nMax `spawn_agent` recursion depth.\n\n```yaml\ndepth_limit: 3\n```",
        "token_budget" => "## `token_budget:` — Total Token Budget\n\nMax total tokens (input + output) across all turns.\n\n```yaml\ntoken_budget: 100000\n```",
        "from" => "## `from:` — Agent Definition Reference\n\nReference a reusable agent defined in `agents:` block.\n\n```yaml\nagents:\n  researcher:\n    system: \"You are a researcher\"\n    tools: [perplexity/search]\n\ntasks:\n  - id: research\n    agent:\n      from: researcher\n      prompt: \"Find papers on AI\"\n```",
        "tool_choice" => "## `tool_choice:` — Tool Selection Strategy\n\n`auto` (default) — Model decides when to use tools\n\n`required` — Must use a tool every turn\n\n`none` — Disable tool use",
        "scope" => "## `scope:` — Agent Scope Preset\n\n`full` — All configured tools\n\n`minimal` — Restricted tool set\n\n`debug` — Verbose logging enabled",
        "stop_sequences" => "## `stop_sequences:` — Stop Sequences\n\nStrings that cause generation to stop.\n\n```yaml\nstop_sequences: [\"END\", \"DONE\"]\n```",
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
        "guard" => "## `guardrails:` — Output Guardrails\n\nValidate LLM output. 4 types: `length`, `schema`, `regex`, `llm`.\n\n```yaml\nguardrails:\n  - type: length\n    max_words: 500\n    on_failure: retry\n```",
        "output" => "## `output:` — Output Format\n\nControl task output format.\n\n```yaml\noutput:\n  format: json\n```",
        "structured" => "## `structured:` — JSON Schema Output\n\nForce LLM to output valid JSON matching a schema.\n\n```yaml\nstructured:\n  schema:\n    type: object\n    properties:\n      title: { type: string }\n    required: [\"title\"]\n```",
        "on_error" => "## `on_error:` — Error Handling\n\nControl behavior when task fails.\n\n`continue` · `fail` (default) · `skip`",
        "as" => "## `as:` — Loop Variable Name\n\nName for the current iteration item in `for_each:`.\n\n```yaml\nfor_each: [1, 2, 3]\nas: num\n```\n\nAccess via `{{with.item}}` or `{{with.num}}` in the task.",
        "concurrency" => "## `concurrency:` — Parallel Limit\n\nMax parallel iterations for `for_each:` loops.\n\n```yaml\nconcurrency: 5\n```",
        "fail_fast" => "## `fail_fast:` — Stop on First Failure\n\nAbort remaining iterations if one fails.\n\n```yaml\nfail_fast: true\n```",
        "description" => "## `description:` — Workflow Description\n\nHuman-readable description of the workflow.",
        "guardrails" => "## `guardrails:` — Output Guardrails\n\nValidate LLM output. 4 types: `length`, `schema`, `regex`, `llm`.\n\n```yaml\nguardrails:\n  - type: length\n    max_words: 500\n    on_failure: retry\n  - type: regex\n    pattern: \"^\\\\{\"\n    message: \"Must be JSON\"\n```",
        "completion" => "## `completion:` — Agent Completion Config\n\nHow the agent signals task completion.\n\n```yaml\ncompletion:\n  mode: explicit  # explicit | natural | pattern\n  signal:\n    tool: nika:complete\n```",
        "limits" => "## `limits:` — Agent Execution Limits\n\nResource limits for agent loops.\n\n```yaml\nlimits:\n  max_turns: 20\n  max_cost_usd: 0.50\n  max_duration_secs: 120\n```",
        "skills" => "## `skills:` — Skill Injection\n\nSkills to inject into agent prompt.\n\n```yaml\nskills: [research, summarize]\n```\n\nSkill aliases must be defined in workflow-level `skills:` block.",
        "artifact" => "## `artifact:` — Output Artifact\n\nSave task output to file.\n\n```yaml\nartifact:\n  path: ./output/result.md\n  source: result\n  mode: overwrite\n```",
        "decompose" => "## `decompose:` — Runtime DAG Expansion\n\nExpand task into sub-tasks at runtime.\n\n```yaml\ndecompose:\n  strategy: semantic\n  traverse: HAS_CHILD\n  source: $parent\n  max_items: 10\n```",
        "tool_choice" => "## `tool_choice:` — Tool Selection Mode\n\nControl agent tool usage.\n\n`auto` (default) · `required` · `none`",
        "scope" => "## `scope:` — Agent Scope Preset\n\n`full` (all tools) · `minimal` (restricted) · `debug` (verbose logging)",
        "resource" => "## `resource:` — MCP Resource URI\n\nRead a resource from an MCP server (mutually exclusive with `tool:`).\n\n```yaml\ninvoke:\n  mcp: novanet\n  resource: \"novanet://entity/qr-code\"\n```",
        "provider" => "## `provider:` — Task Provider Override\n\nOverride the default LLM provider for this task.\n\n```yaml\nprovider: openai\n```\n\n**Providers:** `anthropic` · `openai` · `mistral` · `groq` · `deepseek` · `gemini` · `xai` · `native`",
        "model" => "## `model:` — Task Model Override\n\nOverride the default model for this task.\n\n```yaml\nmodel: gpt-4o\n```",
        "log" => "## `log:` — Task Logging Override\n\nOverride logging configuration for this task.\n\n```yaml\nlog:\n  level: debug\n```",
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
        "skills" => "## `skills:` — Skill Definitions\n\nMap skill aliases to file paths for prompt augmentation.\n\n```yaml\nskills:\n  research: ./skills/research.md\n  summarize: pkg:@supernovae/summarize\n```\n\nReferenced by agent `skills:` arrays.",
        "agents" => "## `agents:` — Reusable Agent Definitions\n\nDefine agent configs reusable across tasks.\n\n```yaml\nagents:\n  researcher:\n    system: \"You are a researcher\"\n    tools: [nika:read, perplexity/search]\n```\n\nReference via `from: researcher` in agent tasks.",
        "pkg" => "## `pkg:` — Package Includes\n\nInclude packages from the Nika registry.\n\n```yaml\npkg:\n  include:\n    - pkg:@supernovae/seo@1.0\n```",
        "artifacts" => "## `artifacts:` — Workflow Artifact Defaults\n\nDefault artifact output configuration.",
        "log" => "## `log:` — Logging Configuration\n\nWorkflow-level logging settings.\n\n```yaml\nlog:\n  level: info\n  format: json\n```",
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
            contents:
                "## Context File Reference\n\nData loaded via `context:` block at workflow start."
                    .to_string(),
            range: None,
        })
    } else if trimmed.starts_with("inputs.") {
        Some(HoverResult {
            contents: "## Input Parameter\n\nWorkflow input declared in `inputs:` block."
                .to_string(),
            range: None,
        })
    } else if trimmed.starts_with("item") || trimmed.starts_with("index") {
        Some(HoverResult {
            contents: "## Loop Variable\n\nCurrent item/index from `for_each:` iteration."
                .to_string(),
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
    // Strip parenthesized args for parametric transforms: join(", ") → join
    let base = t.split('(').next().unwrap_or(t).trim();
    let doc = match base {
        // String transforms
        "upper" => "`upper` — Convert to UPPERCASE",
        "lower" => "`lower` — Convert to lowercase",
        "trim" => "`trim` — Strip leading/trailing whitespace",
        "trim_start" => "`trim_start` — Strip leading whitespace",
        "trim_end" => "`trim_end` — Strip trailing whitespace",
        "length" => "`length` — Count characters (string) or elements (array)",
        "to_string" => "`to_string` — Convert any value to string",
        // Array transforms
        "first" => "`first` — First element of array",
        "last" => "`last` — Last element of array",
        "flatten" => "`flatten` — Flatten nested arrays",
        "reverse" => "`reverse` — Reverse array or string",
        "sort" => "`sort` — Sort array",
        "unique" => "`unique` — Remove duplicates",
        "compact" => "`compact` — Remove null values from array",
        "keys" => "`keys` — Object keys as array",
        "values" => "`values` — Object values as array",
        // Numeric transforms
        "to_number" => "`to_number` — Parse string to number",
        "round" => "`round` — Round number (optional precision)",
        "abs" => "`abs` — Absolute value",
        "ceil" => "`ceil` — Round up to integer",
        "floor" => "`floor` — Round down to integer",
        // Type transforms
        "to_bool" => "`to_bool` — Convert to boolean",
        "to_json" => "`to_json` — Serialize to JSON string",
        "parse_json" => "`parse_json` — Parse JSON string to value",
        "type_of" => "`type_of` — Get type name (string, number, array, etc.)",
        // Parametric transforms
        "join" => "`join(sep)` — Join array with separator",
        "split" => "`split(sep)` — Split string into array",
        "default" => "`default(val)` — Fallback value if null/empty",
        // System
        "shell" => "`shell` — Shell-escape value for safe interpolation",
        _ => return None,
    };
    Some(HoverResult {
        contents: format!(
            "## Pipe Transform\n\n{}\n\n```yaml\n{{{{with.data | {}}}}}\n```",
            doc, t
        ),
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
    fn verb_subfields_have_hover() {
        // Common fields that should work for any verb
        for key in [
            "model",
            "temperature",
            "system",
            "extended_thinking",
            "thinking_budget",
        ] {
            let result = verb_subfield_hover("infer", key);
            assert!(
                result.is_some(),
                "Missing verb sub-field hover for infer.{}",
                key
            );
        }
        // Verb-specific fields
        assert!(verb_subfield_hover("infer", "prompt").is_some());
        assert!(verb_subfield_hover("agent", "prompt").is_some());
        assert!(verb_subfield_hover("exec", "command").is_some());
        assert!(verb_subfield_hover("exec", "shell").is_some());
        assert!(verb_subfield_hover("fetch", "url").is_some());
        assert!(verb_subfield_hover("fetch", "extract").is_some());
        assert!(verb_subfield_hover("invoke", "tool").is_some());
        assert!(verb_subfield_hover("invoke", "resource").is_some());
        assert!(verb_subfield_hover("agent", "max_turns").is_some());
        assert!(verb_subfield_hover("agent", "from").is_some());
        assert!(verb_subfield_hover("agent", "tools").is_some());
    }

    #[test]
    fn extract_hover_has_all_modes() {
        let r = verb_subfield_hover("fetch", "extract").unwrap();
        for mode in [
            "markdown", "article", "text", "selector", "metadata", "links", "jsonpath", "feed",
            "llm_txt",
        ] {
            assert!(
                r.contents.contains(mode),
                "Extract hover missing mode: {}",
                mode
            );
        }
    }

    #[test]
    fn all_root_keys_have_hover() {
        for k in [
            "schema", "workflow", "tasks", "mcp", "context", "include", "provider", "inputs",
            "edges", "pkg",
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
        for t in [
            "upper",
            "lower",
            "trim",
            "to_json",
            "length",
            "first",
            "last",
            "sort",
            "keys",
            "values",
            "compact",
            "unique",
            "flatten",
            "reverse",
            "trim_start",
            "trim_end",
            "to_string",
            "to_number",
            "to_bool",
            "parse_json",
            "round",
            "abs",
            "ceil",
            "floor",
            "type_of",
            "shell",
            "join",
            "split",
            "default",
        ] {
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
        let r = hover("", 0, &ctx, None);
        assert!(r.is_some());
        assert!(r.unwrap().contents.contains("LLM Generation"));
    }

    #[test]
    fn hover_with_context_unknown() {
        let ctx = CursorContext::Unknown {
            prefix: String::new(),
        };
        assert!(hover("", 0, &ctx, None).is_none());
    }

    #[test]
    fn hover_depends_on() {
        let ctx = CursorContext::DependsOn {
            task_id: None,
            existing_deps: vec![],
            prefix: String::new(),
        };
        let r = hover("", 0, &ctx, None).unwrap();
        assert!(r.contents.contains("Execution Dependencies"));
    }

    #[test]
    fn hover_retry() {
        let ctx = CursorContext::RetryBlock {
            task_id: None,
            prefix: String::new(),
        };
        let r = hover("", 0, &ctx, None).unwrap();
        assert!(r.contents.contains("Retry Policy"));
    }

    #[test]
    fn hover_guardrails() {
        let ctx = CursorContext::Guardrails {
            task_id: None,
            guardrail_type: None,
            prefix: String::new(),
        };
        let r = hover("", 0, &ctx, None).unwrap();
        assert!(r.contents.contains("Guardrails"));
    }

    #[test]
    fn hover_workflow_root_with_history() {
        let ctx = CursorContext::WorkflowRoot {
            prefix: "workflow:".into(),
        };
        let history = vec![
            WorkflowRunInfo {
                job_id: "j1".into(),
                state: "completed".into(),
                workflow: "test.nika.yaml".into(),
                created_at: "2026-03-27T12:00:00Z".into(),
                started_at: Some("2026-03-27T12:00:01Z".into()),
                completed_at: Some("2026-03-27T12:00:03Z".into()),
                exit_code: Some(0),
            },
            WorkflowRunInfo {
                job_id: "j2".into(),
                state: "failed".into(),
                workflow: "test.nika.yaml".into(),
                created_at: "2026-03-27T11:00:00Z".into(),
                started_at: Some("2026-03-27T11:00:01Z".into()),
                completed_at: Some("2026-03-27T11:00:05Z".into()),
                exit_code: Some(1),
            },
        ];
        let data = DaemonHoverData {
            workflow_history: &history,
        };
        let r = hover("", 0, &ctx, Some(&data)).unwrap();
        assert!(
            r.contents.contains("Recent runs"),
            "Should show run history: {}",
            r.contents
        );
        assert!(
            r.contents.contains("\u{2713}"),
            "Should show checkmark for success"
        );
        assert!(
            r.contents.contains("\u{2717}"),
            "Should show cross for failure"
        );
    }

    #[test]
    fn hover_workflow_root_without_history() {
        let ctx = CursorContext::WorkflowRoot {
            prefix: "workflow:".into(),
        };
        let r = hover("", 0, &ctx, None).unwrap();
        assert!(
            !r.contents.contains("Recent runs"),
            "No history = no runs section"
        );
    }
}
