; Nika Workflow Language — Syntax Highlighting for Zed
; Uses tree-sitter-yaml grammar nodes with Nika-specific semantic queries
;
; Tree-sitter-yaml node types:
;   stream, document, block_mapping, block_mapping_pair, block_sequence,
;   block_sequence_item, flow_mapping, flow_sequence, flow_pair,
;   block_scalar, plain_scalar, double_quote_scalar, single_quote_scalar,
;   boolean_scalar, null_scalar, integer_scalar, float_scalar,
;   tag, anchor, alias, comment

; ============================================================
; Comments
; ============================================================

(comment) @comment

; ============================================================
; Schema declaration (top-level special treatment)
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "schema")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @string.special)))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "schema")))
  value: (flow_node
    (double_quote_scalar) @string.special))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "schema")))
  value: (flow_node
    (single_quote_scalar) @string.special))

; ============================================================
; Workflow name
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "workflow")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @function)))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "workflow")))
  value: (flow_node
    (double_quote_scalar) @function))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "workflow")))
  value: (flow_node
    (single_quote_scalar) @function))

; ============================================================
; Top-level keywords
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#match? @keyword "^(agents|artifacts|context|description|goal|include|inputs|log|max_duration_secs|mcp|model|orchestrate|pkg|provider|routing|schedule|skills|tasks)$"))))

; ============================================================
; Provider name values (known providers)
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "provider")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @constant
      (#match? @constant "^(anthropic|claude|openai|gpt|mistral|groq|deepseek|gemini|google|xai|grok|native|local|mock)$"))))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "provider")))
  value: (flow_node
    (double_quote_scalar) @constant
    (#match? @constant "anthropic|claude|openai|gpt|mistral|groq|deepseek|gemini|google|xai|grok|native|local|mock")))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "provider")))
  value: (flow_node
    (single_quote_scalar) @constant
    (#match? @constant "anthropic|claude|openai|gpt|mistral|groq|deepseek|gemini|google|xai|grok|native|local|mock")))

; ============================================================
; Task ID — the id: field value gets function highlighting
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "id")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @function)))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "id")))
  value: (flow_node
    (double_quote_scalar) @function))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword
      (#eq? @keyword "id")))
  value: (flow_node
    (single_quote_scalar) @function))

; ============================================================
; 5 Semantic Verbs (special keyword treatment)
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @keyword.function
      (#match? @keyword.function "^(infer|exec|fetch|invoke|agent)$"))))

; ============================================================
; Task-level fields
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @type
      (#match? @type "^(artifact|as|concurrency|context_budget|decompose|depends_on|description|fail_fast|for_each|guardrails|log|model|on_error|output|preset|provider|record|retry|routing|structured|timeout|when|with)$"))))

; ============================================================
; Verb sub-fields (properties inside verb blocks)
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @property
      (#match? @property "^(prompt|system|temperature|max_tokens|extended_thinking|thinking_budget|response_format|content|command|shell|cwd|working_dir|env|url|method|headers|body|json|extract|selector|response|follow_redirects|mcp|server|tool|resource|params|max_turns|max_iterations|depth_limit|tools|goal|tool_choice|scope|from|token_budget|stop_sequences|source|detail|signal|confidence|mode|on_failure|on_limit_reached|judge_prompt|judge_model|pass_pattern|pattern|negate|type|text|max_attempts|delay|delay_ms|backoff|backoff_multiplier|strategy|traverse|mcp_server|max_items|max_depth|max_cost_usd|max_duration_secs|save_progress|enable_repair|max_retries|repair_model|schema|path|format|dir|manifest|max_size|overwrite)$"))))

; ============================================================
; Structured output schema keywords
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @type
      (#match? @type "^(properties|required|items|minimum|maximum|minItems|maxItems|minLength|maxLength|enum|default|additionalProperties)$"))))

; ============================================================
; Extract mode values
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "extract")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @constant
      (#match? @constant "^(markdown|article|text|selector|metadata|links|jsonpath|feed|llm_txt)$"))))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "extract")))
  value: (flow_node
    (double_quote_scalar) @constant
    (#match? @constant "markdown|article|text|selector|metadata|links|jsonpath|feed|llm_txt")))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "extract")))
  value: (flow_node
    (single_quote_scalar) @constant
    (#match? @constant "markdown|article|text|selector|metadata|links|jsonpath|feed|llm_txt")))

; ============================================================
; HTTP method values
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "method")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @constant
      (#match? @constant "^(GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS)$"))))

; ============================================================
; Tool names: nika:*, server::tool
; ============================================================

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "tool")))
  value: (flow_node
    (plain_scalar
      (string_scalar) @function.method
      (#match? @function.method "^(nika:|\\w+::)"))))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "tool")))
  value: (flow_node
    (double_quote_scalar) @function.method
    (#match? @function.method "nika:|\\w+::")))

(block_mapping_pair
  key: (flow_node
    (plain_scalar
      (string_scalar) @_key
      (#eq? @_key "tool")))
  value: (flow_node
    (single_quote_scalar) @function.method
    (#match? @function.method "nika:|\\w+::")))

; ============================================================
; Dollar references: $task_id, $task.field, $inputs.key
; ============================================================

((plain_scalar
  (string_scalar) @variable
  (#match? @variable "^\\$")))

; ============================================================
; Environment variables: $env.API_KEY
; ============================================================

((plain_scalar
  (string_scalar) @variable.special
  (#match? @variable.special "^\\$env\\.")))

; ============================================================
; Template expressions {{...}}
; ============================================================

((double_quote_scalar) @string.special
  (#match? @string.special "\\{\\{.*\\}\\}"))

((single_quote_scalar) @string.special
  (#match? @string.special "\\{\\{.*\\}\\}"))

; ============================================================
; Error codes in comments (NIKA-XXX)
; ============================================================

((comment) @comment
  (#match? @comment "NIKA-\\d+"))

; ============================================================
; Strings
; ============================================================

(double_quote_scalar) @string
(single_quote_scalar) @string
(block_scalar) @string

; ============================================================
; Numbers
; ============================================================

(integer_scalar) @number
(float_scalar) @number

; ============================================================
; Booleans
; ============================================================

(boolean_scalar) @boolean

; ============================================================
; Null
; ============================================================

(null_scalar) @constant.builtin

; ============================================================
; YAML anchors and aliases
; ============================================================

(anchor) @label
(alias) @label

; ============================================================
; YAML tags
; ============================================================

(tag) @attribute

; ============================================================
; Flow mapping/sequence punctuation
; ============================================================

(flow_mapping
  "{" @punctuation.bracket
  "}" @punctuation.bracket)

(flow_sequence
  "[" @punctuation.bracket
  "]" @punctuation.bracket)

; ============================================================
; Sequence item dash
; ============================================================

(block_sequence_item
  "-" @punctuation.special)

; ============================================================
; Key-value separator
; ============================================================

(block_mapping_pair
  ":" @punctuation.delimiter)

(flow_pair
  ":" @punctuation.delimiter)
