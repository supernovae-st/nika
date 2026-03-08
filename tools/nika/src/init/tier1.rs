//! Tier 1: Zero Dependencies
//!
//! These workflows work immediately without any API keys or external services.

use super::WorkflowTemplate;

pub fn get_tier1_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "01-exec-basics.nika.yaml",
            tier_dir: "tier-1-no-deps",
            content: WORKFLOW_01_EXEC_BASICS,
        },
        WorkflowTemplate {
            filename: "02-fetch-http.nika.yaml",
            tier_dir: "tier-1-no-deps",
            content: WORKFLOW_02_FETCH_HTTP,
        },
        WorkflowTemplate {
            filename: "03-builtins-core.nika.yaml",
            tier_dir: "tier-1-no-deps",
            content: WORKFLOW_03_BUILTINS_CORE,
        },
    ]
}

/// 01: Shell command execution basics
pub const WORKFLOW_01_EXEC_BASICS: &str = r##"# ══════════════════════════════════════════════════════════════════════════════
# 🖥️  01 - EXEC BASICS
# ══════════════════════════════════════════════════════════════════════════════
#
# Learn shell command execution with Nika - no API keys needed!
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                            WORKFLOW FLOW                                    │
# ├─────────────────────────────────────────────────────────────────────────────┤
# │                                                                             │
# │   ┌──────────────┐     ┌──────────────┐     ┌──────────────┐               │
# │   │  get_date    │     │  get_user    │     │  list_files  │               │
# │   │   exec:      │     │   exec:      │     │   exec:      │               │
# │   │  "date"      │     │  "whoami"    │     │  "ls -la"    │               │
# │   └──────┬───────┘     └──────┬───────┘     └──────┬───────┘               │
# │          │                    │                    │                        │
# │          └────────────────────┼────────────────────┘                        │
# │                               │                                             │
# │                               ▼                                             │
# │                    ┌──────────────────────┐                                 │
# │                    │    show_system_info   │                                │
# │                    │        exec:          │                                │
# │                    │  "echo System Info"   │                                │
# │                    └──────────┬────────────┘                                │
# │                               │                                             │
# │                               ▼                                             │
# │                    ┌──────────────────────┐                                 │
# │                    │    with_env_vars     │                                 │
# │                    │       exec:          │                                 │
# │                    │   env: { VAR: val }  │                                 │
# │                    └──────────────────────┘                                 │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# FEATURES DEMONSTRATED:
# ├── exec: shorthand syntax (just a string)
# ├── exec: full form with options
# ├── shell: true vs false (security)
# ├── env: environment variables
# ├── timeout_ms: command timeout
# ├── use: bindings between tasks
# └── flows: task dependencies
#
# PREREQUISITES: None! Works out of the box.
#
# RUN: nika run workflows/tier-1-no-deps/01-exec-basics.nika.yaml
#
# ══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.10"
workflow: exec-basics
description: "Learn shell command execution - no API keys needed!"

tasks:
  # ─────────────────────────────────────────────────────────────────────────────
  # SHORTHAND SYNTAX: Just pass a command string
  # ─────────────────────────────────────────────────────────────────────────────
  - id: get_date
    # Shorthand: exec: "command" (simplest form)
    exec: "date '+%Y-%m-%d %H:%M:%S'"

  - id: get_user
    # Another shorthand example
    exec: "whoami"

  - id: list_files
    # List files in current directory
    exec: "ls -la | head -10"

  # ─────────────────────────────────────────────────────────────────────────────
  # FULL FORM: With all options
  # ─────────────────────────────────────────────────────────────────────────────
  - id: show_system_info
    # Bind outputs from previous tasks
    use:
      date_result: get_date      # Output of get_date task
      user_result: get_user      # Output of get_user task
      files_result: list_files   # Output of list_files task
    # Full form with explicit options
    exec:
      # The command to run
      command: |
        echo "═══════════════════════════════════════"
        echo "        SYSTEM INFORMATION"
        echo "═══════════════════════════════════════"
        echo ""
        echo "📅 Date: {{use.date_result}}"
        echo "👤 User: {{use.user_result}}"
        echo ""
        echo "📁 Files:"
        echo "{{use.files_result}}"
      # shell: true allows pipes, redirects, and variables
      # WARNING: Only use shell: true when you need shell features!
      # shell: false (default) is more secure
      shell: true
      # Timeout in milliseconds (30 seconds)
      timeout_ms: 30000

  # ─────────────────────────────────────────────────────────────────────────────
  # ENVIRONMENT VARIABLES: Pass custom env vars to commands
  # ─────────────────────────────────────────────────────────────────────────────
  - id: with_env_vars
    exec:
      command: 'echo "Greeting: $GREETING | Mode: $MODE"'
      shell: true
      env:
        GREETING: "Hello from Nika!"
        MODE: "demo"
        CUSTOM_VAR: "This is a custom environment variable"

  # ─────────────────────────────────────────────────────────────────────────────
  # SHELL-FREE MODE (Default & Secure)
  # ─────────────────────────────────────────────────────────────────────────────
  - id: secure_command
    exec:
      # shell: false (default) - command is parsed with shlex, no shell injection
      command: "echo 'This runs without shell - more secure!'"
      shell: false
      # Working directory (optional)
      # working_dir: /tmp

  # ─────────────────────────────────────────────────────────────────────────────
  # FINAL SUMMARY
  # ─────────────────────────────────────────────────────────────────────────────
  - id: summary
    use:
      sys_info: show_system_info
      env_demo: with_env_vars
      secure: secure_command
    exec:
      command: |
        echo ""
        echo "✅ All exec: examples completed successfully!"
        echo ""
        echo "💡 Key takeaways:"
        echo "   • Use shorthand 'exec: \"command\"' for simple commands"
        echo "   • Use full form for env vars, timeouts, shell mode"
        echo "   • shell: false (default) is more secure"
        echo "   • shell: true needed for pipes, redirects, variables"
      shell: true

# ─────────────────────────────────────────────────────────────────────────────
# FLOWS: Define task execution order
# ─────────────────────────────────────────────────────────────────────────────
flows:
  # Parallel: get_date, get_user, list_files run simultaneously
  - source: get_date
    target: show_system_info
  - source: get_user
    target: show_system_info
  - source: list_files
    target: show_system_info

  # Sequential chain
  - source: show_system_info
    target: with_env_vars
  - source: with_env_vars
    target: secure_command
  - source: secure_command
    target: summary
"##;

/// 02: HTTP fetch operations
pub const WORKFLOW_02_FETCH_HTTP: &str = r##"# ══════════════════════════════════════════════════════════════════════════════
# 🌐 02 - FETCH HTTP
# ══════════════════════════════════════════════════════════════════════════════
#
# Learn HTTP requests with Nika - GET, POST, headers, and JSON parsing!
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                            WORKFLOW FLOW                                    │
# ├─────────────────────────────────────────────────────────────────────────────┤
# │                                                                             │
# │   ┌───────────────┐    ┌───────────────┐    ┌───────────────┐              │
# │   │  simple_get   │    │   get_json    │    │  get_headers  │              │
# │   │   fetch:      │    │    fetch:     │    │    fetch:     │              │
# │   │   GET /ip     │    │   GET /json   │    │  GET /headers │              │
# │   └───────┬───────┘    └───────┬───────┘    └───────┬───────┘              │
# │           │                    │                    │                       │
# │           └────────────────────┼────────────────────┘                       │
# │                                │                                            │
# │                                ▼                                            │
# │                    ┌───────────────────────┐                                │
# │                    │      post_data        │                                │
# │                    │       fetch:          │                                │
# │                    │    POST /post         │                                │
# │                    │    json: { data }     │                                │
# │                    └───────────┬───────────┘                                │
# │                                │                                            │
# │                                ▼                                            │
# │                    ┌───────────────────────┐                                │
# │                    │    with_auth_header   │                                │
# │                    │       fetch:          │                                │
# │                    │   Authorization: ...  │                                │
# │                    └───────────┬───────────┘                                │
# │                                │                                            │
# │                                ▼                                            │
# │                    ┌───────────────────────┐                                │
# │                    │    display_results    │                                │
# │                    │        exec:          │                                │
# │                    │   "echo All done!"    │                                │
# │                    └───────────────────────┘                                │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# FEATURES DEMONSTRATED:
# ├── fetch: with GET method
# ├── fetch: with POST and JSON body
# ├── headers: custom HTTP headers
# ├── timeout_ms: request timeout
# ├── follow_redirects: redirect handling
# └── use: parsing JSON responses
#
# PREREQUISITES: None! Uses httpbin.org (free public API)
#
# RUN: nika run workflows/tier-1-no-deps/02-fetch-http.nika.yaml
#
# ══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.10"
workflow: fetch-http
description: "Learn HTTP requests - GET, POST, headers, JSON parsing"

tasks:
  # ─────────────────────────────────────────────────────────────────────────────
  # SIMPLE GET REQUEST
  # ─────────────────────────────────────────────────────────────────────────────
  - id: simple_get
    fetch:
      # URL to fetch
      url: "https://httpbin.org/ip"
      # HTTP method (GET is default, but explicit is clearer)
      method: GET
      # Timeout in milliseconds (10 seconds)
      timeout_ms: 10000

  # ─────────────────────────────────────────────────────────────────────────────
  # GET JSON DATA
  # ─────────────────────────────────────────────────────────────────────────────
  - id: get_json
    fetch:
      url: "https://httpbin.org/json"
      method: GET
      # Response is automatically parsed if Content-Type is application/json

  # ─────────────────────────────────────────────────────────────────────────────
  # GET WITH CUSTOM HEADERS
  # ─────────────────────────────────────────────────────────────────────────────
  - id: get_headers
    fetch:
      url: "https://httpbin.org/headers"
      method: GET
      # Custom headers to send
      headers:
        Accept: "application/json"
        X-Custom-Header: "Nika-Workflow"
        User-Agent: "Nika/0.21.0"

  # ─────────────────────────────────────────────────────────────────────────────
  # POST WITH JSON BODY
  # ─────────────────────────────────────────────────────────────────────────────
  - id: post_data
    use:
      # Reference results from previous tasks
      my_ip: simple_get
      json_data: get_json
    fetch:
      url: "https://httpbin.org/post"
      method: POST
      # Headers for JSON content
      headers:
        Content-Type: "application/json"
        Accept: "application/json"
      # JSON body (automatically serialized)
      json:
        message: "Hello from Nika!"
        timestamp: "{{use.my_ip}}"
        nested:
          key: "value"
          number: 42
          array: ["a", "b", "c"]

  # ─────────────────────────────────────────────────────────────────────────────
  # AUTHENTICATED REQUEST
  # ─────────────────────────────────────────────────────────────────────────────
  - id: with_auth_header
    fetch:
      url: "https://httpbin.org/bearer"
      method: GET
      headers:
        # Bearer token authentication (this is a test endpoint)
        Authorization: "Bearer test-token-12345"
        Accept: "application/json"
      # Follow redirects (default: true)
      follow_redirects: true

  # ─────────────────────────────────────────────────────────────────────────────
  # DISPLAY ALL RESULTS
  # ─────────────────────────────────────────────────────────────────────────────
  - id: display_results
    use:
      ip_result: simple_get
      json_result: get_json
      headers_result: get_headers
      post_result: post_data
      auth_result: with_auth_header
    exec:
      command: |
        echo "══════════════════════════════════════════════════════════════"
        echo "                    🌐 HTTP FETCH RESULTS"
        echo "══════════════════════════════════════════════════════════════"
        echo ""
        echo "📡 GET /ip:"
        echo "{{use.ip_result}}"
        echo ""
        echo "📦 GET /json:"
        echo "{{use.json_result}}" | head -5
        echo ""
        echo "📋 GET /headers (with custom headers):"
        echo "{{use.headers_result}}" | head -5
        echo ""
        echo "📤 POST /post (with JSON body):"
        echo "{{use.post_result}}" | head -10
        echo ""
        echo "🔐 GET /bearer (with auth):"
        echo "{{use.auth_result}}"
        echo ""
        echo "══════════════════════════════════════════════════════════════"
        echo "✅ All fetch examples completed successfully!"
        echo ""
        echo "💡 Key takeaways:"
        echo "   • Use method: GET, POST, PUT, DELETE, PATCH"
        echo "   • Add custom headers: for auth, content-type, etc."
        echo "   • Use json: for JSON body (auto-serialized)"
        echo "   • Use body: for raw text body"
        echo "   • timeout_ms: prevents hanging on slow endpoints"
      shell: true

# ─────────────────────────────────────────────────────────────────────────────
# FLOWS: Parallel fetch then sequential processing
# ─────────────────────────────────────────────────────────────────────────────
flows:
  # Parallel: first 3 requests run simultaneously
  - source: simple_get
    target: post_data
  - source: get_json
    target: post_data
  - source: get_headers
    target: post_data

  # Sequential: post_data -> auth -> display
  - source: post_data
    target: with_auth_header
  - source: with_auth_header
    target: display_results
"##;

/// 03: Core builtin tools
pub const WORKFLOW_03_BUILTINS_CORE: &str = r##"# ══════════════════════════════════════════════════════════════════════════════
# 🔧 03 - BUILTIN CORE TOOLS
# ══════════════════════════════════════════════════════════════════════════════
#
# Learn Nika's core builtin tools - logging, sleeping, events, and assertions!
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                            WORKFLOW FLOW                                    │
# ├─────────────────────────────────────────────────────────────────────────────┤
# │                                                                             │
# │   ┌───────────────┐                                                         │
# │   │    start      │                                                         │
# │   │  nika:log     │◄── "Workflow starting..."                               │
# │   └───────┬───────┘                                                         │
# │           │                                                                 │
# │           ▼                                                                 │
# │   ┌───────────────┐                                                         │
# │   │   validate    │                                                         │
# │   │ nika:assert   │◄── condition: true (passes!)                            │
# │   └───────┬───────┘                                                         │
# │           │                                                                 │
# │           ▼                                                                 │
# │   ┌───────────────┐                                                         │
# │   │  emit_event   │                                                         │
# │   │  nika:emit    │◄── custom event with payload                            │
# │   └───────┬───────┘                                                         │
# │           │                                                                 │
# │           ▼                                                                 │
# │   ┌───────────────┐                                                         │
# │   │   pause_1s    │                                                         │
# │   │  nika:sleep   │◄── duration: "1s"                                       │
# │   └───────┬───────┘                                                         │
# │           │                                                                 │
# │           ▼                                                                 │
# │   ┌───────────────┐                                                         │
# │   │   progress    │                                                         │
# │   │  nika:emit    │◄── progress: 50%                                        │
# │   └───────┬───────┘                                                         │
# │           │                                                                 │
# │           ▼                                                                 │
# │   ┌───────────────┐                                                         │
# │   │  pause_500ms  │                                                         │
# │   │  nika:sleep   │◄── duration: "500ms"                                    │
# │   └───────┬───────┘                                                         │
# │           │                                                                 │
# │           ▼                                                                 │
# │   ┌───────────────┐                                                         │
# │   │   complete    │                                                         │
# │   │  nika:log     │◄── level: info "Done!"                                  │
# │   └───────────────┘                                                         │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# BUILTIN TOOLS DEMONSTRATED:
# ├── nika:log     → Emit log events (trace/debug/info/warn/error)
# ├── nika:sleep   → Pause execution (humantime: "1s", "500ms", "2m30s")
# ├── nika:emit    → Emit custom events with JSON payload
# └── nika:assert  → Validate conditions (fails workflow if false)
#
# OTHER BUILTINS (not in this demo):
# ├── nika:prompt  → Ask user for input (HITL)
# ├── nika:run     → Execute sub-workflow
# └── nika:complete → Signal agent completion (agent-only)
#
# PREREQUISITES: None! Works out of the box.
#
# RUN: nika run workflows/tier-1-no-deps/03-builtins-core.nika.yaml
#
# ══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.10"
workflow: builtins-core
description: "Learn Nika's core builtin tools - log, sleep, emit, assert"

tasks:
  # ─────────────────────────────────────────────────────────────────────────────
  # nika:log - Emit log events at different levels
  # ─────────────────────────────────────────────────────────────────────────────
  - id: start_log
    invoke:
      # For builtin tools, we don't need an MCP server
      # Just specify the tool directly
      tool: nika:log
      params:
        # Log levels: trace, debug, info, warn, error
        level: info
        message: "🚀 Workflow starting! This is nika:log in action."

  # ─────────────────────────────────────────────────────────────────────────────
  # nika:assert - Validate conditions
  # ─────────────────────────────────────────────────────────────────────────────
  - id: validate_condition
    invoke:
      tool: nika:assert
      params:
        # Condition must be true, otherwise workflow fails with NIKA-213
        condition: true
        # Custom message shown if assertion fails
        message: "This assertion should pass!"

  - id: validate_expression
    invoke:
      tool: nika:assert
      params:
        # You can use more complex boolean conditions
        condition: true  # In real workflows, this could check bindings
        message: "Validating that setup completed correctly"

  # ─────────────────────────────────────────────────────────────────────────────
  # nika:emit - Emit custom events with payload
  # ─────────────────────────────────────────────────────────────────────────────
  - id: emit_start_event
    invoke:
      tool: nika:emit
      params:
        # Event name (appears in trace)
        name: "workflow_started"
        # JSON payload (any structure)
        payload:
          workflow: "builtins-core"
          version: "1.0.0"
          timestamp: "now"
          metadata:
            source: "nika-init-example"
            tier: 1

  # ─────────────────────────────────────────────────────────────────────────────
  # nika:sleep - Pause execution
  # ─────────────────────────────────────────────────────────────────────────────
  - id: pause_1_second
    invoke:
      tool: nika:sleep
      params:
        # Humantime format: "1s", "500ms", "2m30s", "1h", etc.
        duration: "1s"

  - id: log_after_sleep
    invoke:
      tool: nika:log
      params:
        level: debug
        message: "⏰ Slept for 1 second. Continuing..."

  # ─────────────────────────────────────────────────────────────────────────────
  # PROGRESS TRACKING with nika:emit
  # ─────────────────────────────────────────────────────────────────────────────
  - id: emit_progress_50
    invoke:
      tool: nika:emit
      params:
        name: "progress"
        payload:
          percent: 50
          stage: "midpoint"
          message: "Halfway there!"

  - id: pause_500ms
    invoke:
      tool: nika:sleep
      params:
        duration: "500ms"

  - id: emit_progress_100
    invoke:
      tool: nika:emit
      params:
        name: "progress"
        payload:
          percent: 100
          stage: "complete"
          message: "All done!"

  # ─────────────────────────────────────────────────────────────────────────────
  # FINAL LOG
  # ─────────────────────────────────────────────────────────────────────────────
  - id: final_log
    invoke:
      tool: nika:log
      params:
        level: info
        message: |
          ✅ Builtins demo complete!

          Tools demonstrated:
          • nika:log   - Log messages at different levels
          • nika:assert - Validate conditions
          • nika:emit  - Emit custom events
          • nika:sleep - Pause execution

          Check your trace file to see all events!

  # ─────────────────────────────────────────────────────────────────────────────
  # SUMMARY (using exec to show results)
  # ─────────────────────────────────────────────────────────────────────────────
  - id: summary
    exec:
      command: |
        echo ""
        echo "══════════════════════════════════════════════════════════════"
        echo "              🔧 BUILTIN TOOLS SUMMARY"
        echo "══════════════════════════════════════════════════════════════"
        echo ""
        echo "Core Builtins (6):"
        echo "  nika:log     Log messages (trace/debug/info/warn/error)"
        echo "  nika:sleep   Pause execution (humantime format)"
        echo "  nika:emit    Emit custom events with JSON payload"
        echo "  nika:assert  Validate conditions (fail if false)"
        echo "  nika:prompt  Ask user for input (HITL)"
        echo "  nika:run     Execute sub-workflow"
        echo ""
        echo "File Builtins (5) - Agent-only:"
        echo "  nika:read    Read file content"
        echo "  nika:write   Create/overwrite file"
        echo "  nika:edit    Find & replace in file"
        echo "  nika:glob    Find files by pattern"
        echo "  nika:grep    Search content with regex"
        echo ""
        echo "Agent Completion:"
        echo "  nika:complete  Signal agent task completion"
        echo ""
        echo "══════════════════════════════════════════════════════════════"
      shell: true

# ─────────────────────────────────────────────────────────────────────────────
# FLOWS: Sequential execution
# ─────────────────────────────────────────────────────────────────────────────
flows:
  - source: start_log
    target: validate_condition
  - source: validate_condition
    target: validate_expression
  - source: validate_expression
    target: emit_start_event
  - source: emit_start_event
    target: pause_1_second
  - source: pause_1_second
    target: log_after_sleep
  - source: log_after_sleep
    target: emit_progress_50
  - source: emit_progress_50
    target: pause_500ms
  - source: pause_500ms
    target: emit_progress_100
  - source: emit_progress_100
    target: final_log
  - source: final_log
    target: summary
"##;
