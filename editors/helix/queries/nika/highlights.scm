; Nika Workflow — Tree-sitter Highlights for Helix
;
; Overlays on top of tree-sitter-yaml grammar (grammar = "yaml").
; YAML key nodes: (flow_node (plain_scalar (string_scalar)))
;
; Helix resolves queries by language name, so these nika/ queries
; override the default yaml/ queries for *.nika.yaml files.

; ═══════════════════════════════════════════════════════════════════
; Inherit base YAML highlights
; ═══════════════════════════════════════════════════════════════════

; We re-include the essential YAML rules so Nika files are fully
; highlighted even without inheriting from yaml/highlights.scm.

(comment) @comment

(boolean_scalar) @constant.builtin.boolean
(null_scalar) @constant.builtin
(double_quote_scalar) @string
(single_quote_scalar) @string
(block_scalar) @string
(string_scalar) @string
(escape_sequence) @constant.character.escape
(integer_scalar) @constant.numeric.integer
(float_scalar) @constant.numeric.float

(anchor_name) @type
(alias_name) @type
(tag) @type
(yaml_directive) @keyword

["," "-" ":" ">" "?" "|"] @punctuation.delimiter
["[" "]" "{" "}"] @punctuation.bracket
["*" "&" "---" "..."] @punctuation.special

; ═══════════════════════════════════════════════════════════════════
; Schema declaration
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @keyword))
  (#match? @keyword "^schema$"))

; ═══════════════════════════════════════════════════════════════════
; Workflow name (top-level header)
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @keyword))
  (#match? @keyword "^workflow$"))

; ═══════════════════════════════════════════════════════════════════
; Top-level section keys
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @keyword))
  (#match? @keyword "^(agents|artifacts|context|description|goal|include|inputs|log|max_duration_secs|mcp|model|orchestrate|pkg|provider|routing|schedule|skills|tasks)$"))

; ═══════════════════════════════════════════════════════════════════
; 5 Semantic Verbs — the core of Nika
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @keyword.control))
  (#match? @keyword.control "^(infer|exec|fetch|invoke|agent)$"))

; ═══════════════════════════════════════════════════════════════════
; Task ID field
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @keyword.directive))
  (#match? @keyword.directive "^id$"))

; Task ID value — highlighted as function name
(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @_key))
  value: (flow_node (plain_scalar (string_scalar) @function))
  (#match? @_key "^id$"))

; ═══════════════════════════════════════════════════════════════════
; Task-level fields
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @type))
  (#match? @type "^(artifact|as|concurrency|context_budget|decompose|depends_on|description|fail_fast|for_each|guardrails|log|model|on_error|output|preset|provider|record|retry|routing|structured|timeout|when|with)$"))

; ═══════════════════════════════════════════════════════════════════
; Verb sub-fields (inside infer:, exec:, fetch:, invoke:, agent:)
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @variable.other.member))
  (#match? @variable.other.member "^(prompt|system|temperature|max_tokens|extended_thinking|thinking_budget|response_format|content|command|shell|cwd|working_dir|env|url|method|headers|body|json|extract|selector|response|follow_redirects|mcp|server|tool|resource|params|max_turns|max_iterations|depth_limit|tools|goal|tool_choice|scope|from|token_budget|stop_sequences|source|detail|signal|confidence|mode|on_failure|on_limit_reached|judge_prompt|judge_model|pass_pattern|pattern|negate|type|text|max_attempts|delay|delay_ms|backoff|backoff_multiplier|strategy|traverse|mcp_server|max_items|max_depth|max_cost_usd|max_duration_secs|save_progress|enable_repair|max_retries|repair_model|schema|path|format|dir|manifest|max_size|overwrite)$"))

; ═══════════════════════════════════════════════════════════════════
; Structured output / JSON Schema fields
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @type.builtin))
  (#match? @type.builtin "^(properties|required|items|minimum|maximum|minItems|maxItems|minLength|maxLength|enum|default|additionalProperties)$"))

; ═══════════════════════════════════════════════════════════════════
; Provider name values (known providers)
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @_key))
  value: (flow_node (plain_scalar (string_scalar) @constant))
  (#match? @_key "^provider$")
  (#match? @constant "^(anthropic|claude|openai|gpt|mistral|groq|deepseek|gemini|google|xai|grok|native|local|mock)$"))

; ═══════════════════════════════════════════════════════════════════
; Extract mode values
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @_key))
  value: (flow_node (plain_scalar (string_scalar) @constant))
  (#match? @_key "^extract$")
  (#match? @constant "^(markdown|article|text|selector|metadata|links|jsonpath|feed|llm_txt)$"))

; ═══════════════════════════════════════════════════════════════════
; HTTP method values
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @_key))
  value: (flow_node (plain_scalar (string_scalar) @constant))
  (#match? @_key "^method$")
  (#match? @constant "^(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)$"))

; ═══════════════════════════════════════════════════════════════════
; Dollar references: $task_id, $task.field, $inputs.locale
; ═══════════════════════════════════════════════════════════════════

; Plain scalar dollar references
((plain_scalar (string_scalar) @variable)
  (#match? @variable "^\\$"))

; Quoted dollar references
((double_quote_scalar) @variable
  (#match? @variable "^\"\\$"))

; ═══════════════════════════════════════════════════════════════════
; Environment variables: $env.API_KEY
; ═══════════════════════════════════════════════════════════════════

((plain_scalar (string_scalar) @variable.builtin)
  (#match? @variable.builtin "^\\$env\\."))

; ═══════════════════════════════════════════════════════════════════
; Tool names: nika:*, server::tool
; ═══════════════════════════════════════════════════════════════════

; Unquoted tool names
(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @_key))
  value: (flow_node (plain_scalar (string_scalar) @function.method))
  (#match? @_key "^tool$")
  (#match? @function.method "^(nika:|\\w+::)"))

; Quoted tool names
(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @_key))
  value: (flow_node (double_quote_scalar) @function.method)
  (#match? @_key "^tool$"))

; ═══════════════════════════════════════════════════════════════════
; Schema version string value
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @_key))
  value: (flow_node (double_quote_scalar) @string.special)
  (#match? @_key "^schema$"))

; ═══════════════════════════════════════════════════════════════════
; Fallback: all other mapping keys use default member highlight
; (lower priority than specific matches above)
; ═══════════════════════════════════════════════════════════════════

(block_mapping_pair
  key: (flow_node (plain_scalar (string_scalar) @variable.other.member)))
(block_mapping_pair
  key: (flow_node [(double_quote_scalar) (single_quote_scalar)] @variable.other.member))

(flow_mapping
  (_ key: (flow_node (plain_scalar (string_scalar) @variable.other.member))))
(flow_mapping
  (_ key: (flow_node [(double_quote_scalar) (single_quote_scalar)] @variable.other.member)))
