; Nika-specific Tree-sitter highlights for YAML files.
; These queries extend (not replace) the default YAML highlights.
; They match Nika workflow keywords and template expressions.

; ── Schema declaration ──────────────────────────────────────────────────────
; schema: "nika/workflow@0.12"
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "schema"))))

; ── Workflow name ───────────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "workflow"))))

; ── Top-level keys ──────────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @type
      (#any-of? @type
        "agents" "artifacts" "context" "description" "goal" "include" "inputs"
        "log" "max_duration_secs" "mcp" "orchestrate" "pkg" "routing"
        "schedule" "skills" "tasks"))))

; ── Provider and model ──────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @attribute
      (#any-of? @attribute "provider" "model"))))

; ── Task ID ─────────────────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @property
      (#eq? @property "id"))))

; Task ID value — highlighted as function name
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "id")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @function)))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "id")))
  value: (flow_node
    (double_quote_scalar) @function))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "id")))
  value: (flow_node
    (single_quote_scalar) @function))

; ── 5 Semantic Verbs ────────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword.function
      (#any-of? @keyword.function "infer" "exec" "fetch" "invoke" "agent"))))

; ── Task-level fields ───────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @field
      (#any-of? @field
        "artifact" "as" "concurrency" "context_budget" "decompose"
        "depends_on" "description" "fail_fast" "for_each" "guardrails" "log"
        "model" "on_error" "output" "preset" "provider" "record" "retry"
        "routing" "structured" "timeout" "trust" "when" "with"))))

; ── Verb sub-fields ─────────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @variable.parameter
      (#any-of? @variable.parameter
        "prompt" "system" "temperature" "max_tokens"
        "extended_thinking" "thinking_budget" "response_format" "content"
        "command" "shell" "cwd" "working_dir" "env"
        "url" "method" "headers" "body" "json" "extract" "selector"
        "response" "follow_redirects"
        "mcp" "server" "tool" "resource" "params"
        "max_turns" "max_iterations" "depth_limit" "tools" "goal"
        "tool_choice" "scope" "from" "token_budget" "stop_sequences"
        "source" "detail" "signal" "confidence"
        "mode" "on_failure" "on_limit_reached" "judge_prompt" "judge_model"
        "pass_pattern" "pattern" "negate" "type" "text"
        "max_attempts" "delay" "delay_ms" "backoff" "backoff_multiplier"
        "strategy" "traverse" "mcp_server" "max_items" "max_depth"
        "max_cost_usd" "max_duration_secs" "save_progress"
        "enable_repair" "max_retries" "repair_model"
        "schema" "path" "format" "dir" "manifest" "max_size" "overwrite"))))

; ── Structured output / JSON Schema fields ──────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @type
      (#any-of? @type
        "properties" "required" "items" "minimum" "maximum"
        "minItems" "maxItems" "minLength" "maxLength"
        "enum" "default" "additionalProperties"))))

; ── Provider name values (known providers) ──────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "provider")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @constant
      (#any-of? @constant
        "anthropic" "claude" "openai" "gpt" "mistral" "groq" "deepseek"
        "gemini" "google" "xai" "grok" "native" "local" "mock"))))

; ── Extract mode values ─────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "extract")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @constant
      (#any-of? @constant
        "markdown" "article" "text" "selector" "metadata" "links"
        "jsonpath" "feed" "llm_txt"))))

; ── HTTP method values ──────────────────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "method")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @constant
      (#any-of? @constant "GET" "POST" "PUT" "DELETE" "PATCH" "HEAD" "OPTIONS"))))

; ── Tool names: nika:*, server::tool ────────────────────────────────────────
(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "tool")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @function.call
      (#match? @function.call "^(nika:|\\w+::)"))))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "tool")))
  value: (flow_node
    (double_quote_scalar) @function.call
    (#match? @function.call "nika:|\\w+::")))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "tool")))
  value: (flow_node
    (single_quote_scalar) @function.call
    (#match? @function.call "nika:|\\w+::")))

; ── Dollar references ($task_id, $env.VAR, $inputs.key) ────────────────────
((plain_scalar
  (string_scalar) @variable
  (#match? @variable "^\\$")))

; ── Environment variables: $env.API_KEY ─────────────────────────────────────
((plain_scalar
  (string_scalar) @variable.builtin
  (#match? @variable.builtin "^\\$env\\.")))

; ── Template expressions {{...}} ────────────────────────────────────────────
; Match the Nika template syntax inside strings
((string_scalar) @string.special
  (#match? @string.special "\\{\\{.*\\}\\}"))

; ── Error codes in comments ─────────────────────────────────────────────────
((comment) @comment.documentation
  (#match? @comment.documentation "NIKA-\\d+"))
