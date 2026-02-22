# Claude Code File Tools Architecture

A deep dive into how Claude Code implements its filesystem tools (Read, Edit, Glob, Grep) and the broader architecture for replication in Rust CLIs like Nika.

**Status:** Research document for Nika v0.6 filesystem integration
**Based on:** Claude Code v2024.12 and Agent SDK documentation
**Target:** Nika Rust implementation patterns

---

## Table of Contents

1. [Overview](#overview)
2. [Tool Architecture](#tool-architecture)
3. [Built-in Tools](#built-in-tools)
4. [Tool Implementation Pattern](#tool-implementation-pattern)
5. [MCP Integration](#mcp-integration)
6. [Rust Implementation Strategy](#rust-implementation-strategy)

---

## Overview

Claude Code provides a comprehensive toolkit for filesystem operations, code editing, and codebase analysis. The tools are:

| Tool | Purpose | Key Features |
|------|---------|--------------|
| **Read** | Read any file in working directory | Line-based access, offset support, image/PDF support |
| **Write** | Create new files | Atomic writes, requires prior Read |
| **Edit** | Make precise edits to existing files | Must Read first, replace_all option, atomic updates |
| **Bash** | Run terminal commands, git operations | Sandboxed, permission modes, environment isolation |
| **Glob** | Find files by pattern | Fast pattern matching, `**/*.ext` support |
| **Grep** | Search file contents with regex | Full ripgrep support, output modes (content/files/count) |
| **WebSearch** | Search the web | Current information, sources tracking |
| **WebFetch** | Fetch and parse web page content | HTML to markdown conversion, screenshot capture |

**Reference:** Claude Code CLI reference shows all 8 built-in tools with full documentation.

---

## Tool Architecture

### System Design

Claude Code tools follow a consistent pattern:

```
┌─────────────────────────────────────────────────────────────────┐
│  Claude Code Agent Loop (TypeScript/Python SDK)                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ query() or ClaudeSDKClient                               │  │
│  │  - Creates agentic loop                                  │  │
│  │  - Streams messages as Claude works                      │  │
│  │  - Handles tool orchestration automatically              │  │
│  └──────────────────────────────────────────────────────────┘  │
│         │                                                       │
│         │ Calls with JSON parameters                           │
│         ▼                                                       │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ Built-in Tool Executor                                   │  │
│  │  - Validates parameters                                  │  │
│  │  - Checks permissions                                    │  │
│  │  - Executes tool                                         │  │
│  │  - Returns structured result                             │  │
│  └──────────────────────────────────────────────────────────┘  │
│         │                                                       │
│         │ Tool-specific logic                                  │
│         ▼                                                       │
│  ┌──────────────────────────────────────────────────────────┐  │
│  │ Filesystem Operations                                    │  │
│  │  - Path validation                                       │  │
│  │  - File I/O                                              │  │
│  │  - Pattern matching (ripgrep, glob)                      │  │
│  │  - Command execution (tokio/subprocess)                  │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Tool Definition Format

Tools are defined with a structured specification:

```json
{
  "name": "string (1-64 chars, alphanumeric + hyphen)",
  "description": "Human-readable description (3-4+ sentences)",
  "input_schema": {
    "type": "object",
    "properties": { /* tool-specific parameters */ },
    "required": [ /* required parameters */ ]
  },
  "input_examples": [ /* optional examples for complex inputs */ ]
}
```

**Key principle:** Detailed descriptions are the most important factor in tool performance.

---

## Built-in Tools

### 1. Read Tool

**Purpose:** Read any file in the working directory

**Parameters:**
```json
{
  "file_path": {
    "type": "string",
    "description": "Absolute path to the file to read"
  },
  "limit": {
    "type": "number",
    "description": "Number of lines to read (default: 2000)",
    "optional": true
  },
  "offset": {
    "type": "number",
    "description": "Line number to start reading from",
    "optional": true
  }
}
```

**Capabilities:**
- Reads up to 2000 lines by default
- Supports line offset and limit for large files
- Can read image files (PNG, JPG, etc.) and return visual content
- Supports PDF files (extracted page by page)
- Supports Jupyter notebooks (.ipynb) with code and outputs
- Returns content in line-number format (cat -n style)
- Truncates lines longer than 2000 characters

**Key Features:**
- **Must use absolute paths** (not relative paths)
- Works with all file types (text, binary, media)
- Returns structured output with line numbers

**Implementation consideration:** Read is the gateway tool - many operations require reading first before modification.

### 2. Edit Tool

**Purpose:** Make precise edits to existing files

**Parameters:**
```json
{
  "file_path": {
    "type": "string",
    "description": "Absolute path to file to modify"
  },
  "old_string": {
    "type": "string",
    "description": "Text to replace (must be unique or use replace_all)"
  },
  "new_string": {
    "type": "string",
    "description": "Replacement text (must differ from old_string)"
  },
  "replace_all": {
    "type": "boolean",
    "description": "Replace all occurrences (default: false)",
    "optional": true
  }
}
```

**Requirements:**
- **MUST read file first** - Edit fails if file hasn't been read
- `old_string` must be unique in file OR `replace_all: true`
- Preserves exact indentation (tabs/spaces)
- **Atomic operations** - updates are transactional

**Key Features:**
- Line-accurate replacement
- Indentation preservation (critical for Python/YAML)
- Replace all instances with flag
- Validation that old_string matches exactly

**Implementation detail:** The "must read first" requirement enables:
- Validation that file exists and is accessible
- Caching of file state for atomic updates
- Rollback capability on failure

### 3. Glob Tool

**Purpose:** Fast file pattern matching

**Parameters:**
```json
{
  "pattern": {
    "type": "string",
    "description": "Glob pattern (e.g., '**/*.js', 'src/**/*.ts')"
  },
  "path": {
    "type": "string",
    "description": "Directory to search in (default: current working directory)",
    "optional": true
  }
}
```

**Supported Patterns:**
- `*.js` - All .js files in current directory
- `**/*.ts` - All .ts files recursively
- `src/**/*.py` - All .py files under src/
- `test/**/*_test.js` - Specific naming patterns

**Returns:**
- Sorted list of matching file paths
- Sorted by modification time
- Absolute paths (recommended usage)
- Empty array if no matches

**Implementation pattern:**
- Uses fast glob matching (not shell globbing)
- Returns results sorted for deterministic behavior
- Works across all platforms (Windows, macOS, Linux)

### 4. Grep Tool

**Purpose:** Search file contents with full regex support

**Parameters:**
```json
{
  "pattern": {
    "type": "string",
    "description": "Regular expression pattern to search for"
  },
  "path": {
    "type": "string",
    "description": "File or directory to search",
    "optional": true
  },
  "glob": {
    "type": "string",
    "description": "Glob pattern to filter files (e.g., '*.js')",
    "optional": true
  },
  "type": {
    "type": "string",
    "description": "File type (js, py, rust, go, etc.)",
    "optional": true
  },
  "output_mode": {
    "type": "string",
    "enum": ["content", "files_with_matches", "count"],
    "description": "What to return (default: 'files_with_matches')",
    "optional": true
  },
  "-n": {
    "type": "boolean",
    "description": "Show line numbers (requires output_mode: 'content')",
    "optional": true
  },
  "-B": {
    "type": "number",
    "description": "Lines before match context",
    "optional": true
  },
  "-A": {
    "type": "number",
    "description": "Lines after match context",
    "optional": true
  },
  "-C": {
    "type": "number",
    "description": "Lines before and after match context",
    "optional": true
  },
  "-i": {
    "type": "boolean",
    "description": "Case-insensitive search",
    "optional": true
  },
  "multiline": {
    "type": "boolean",
    "description": "Enable multiline mode (patterns can span lines)",
    "optional": true
  },
  "head_limit": {
    "type": "number",
    "description": "Limit output to first N lines/entries",
    "optional": true
  }
}
```

**Output Modes:**
1. **files_with_matches** (default) - List of files containing matches
2. **content** - Full matching lines with context
3. **count** - Match count per file

**Implementation built on ripgrep:**
- Full regex support (not simple string matching)
- Fast parallel search across files
- Context line support (-B, -A, -C)
- Output filtering and limiting

**Key features:**
- Multiline pattern support (patterns can span lines with `multiline: true`)
- Line number inclusion with `-n`
- Context window surrounding matches
- Efficient streaming results for large codebases

---

## Tool Implementation Pattern

### Request/Response Cycle

```
1. User/Claude makes request with tool parameters (JSON)
       │
       ▼
2. Tool executor validates schema
       │
       ├─→ Check required parameters
       ├─→ Validate parameter types
       └─→ Check file/path permissions
       │
       ▼
3. Tool implementation executes
       │
       ├─→ Filesystem operations
       ├─→ Pattern matching
       └─→ Result formatting
       │
       ▼
4. Return structured result
       │
       ├─→ Success case: { content: [...], ... }
       └─→ Error case: { error: "...", code: "..." }
```

### Tool Result Format

```json
{
  "content": [
    {
      "type": "text",
      "text": "Tool output here"
    }
  ],
  "is_error": false
}
```

### Tool Composition

Tools work together in patterns:

```python
# Pattern 1: Search → Read → Edit
async for message in query(
    prompt="Find TODO comments and remove them",
    options=ClaudeAgentOptions(allowed_tools=["Grep", "Read", "Edit"])
):
    pass

# Pattern 2: Glob → Read
async for message in query(
    prompt="Find and analyze all Python files",
    options=ClaudeAgentOptions(allowed_tools=["Glob", "Read"])
):
    pass

# Pattern 3: Recursive operations
async for message in query(
    prompt="Add license header to all .ts files",
    options=ClaudeAgentOptions(allowed_tools=["Glob", "Read", "Edit"])
):
    pass
```

---

## MCP Integration

Claude Code tools can be exposed via MCP (Model Context Protocol) servers, which is relevant for Nika integration.

### MCP Tool Format

When exposing tools via MCP, they follow the same JSON Schema structure but are wrapped in MCP protocol:

```python
from claude_agent_sdk import tool, create_sdk_mcp_server

@tool(
    name="read_file",
    description="Read a file from the filesystem",
    input_schema={"file_path": str}
)
async def read_file(args: dict[str, Any]) -> dict[str, Any]:
    # Implementation
    return {
        "content": [{"type": "text", "text": file_contents}]
    }

server = create_sdk_mcp_server(
    name="filesystem-tools",
    version="1.0.0",
    tools=[read_file]
)
```

### MCP Naming Convention

When exposed via MCP, tools follow the pattern:
```
mcp__{server_name}__{tool_name}
```

Example: A tool named `read_file` in server `filesystem-tools` becomes:
```
mcp__filesystem-tools__read_file
```

---

## Rust Implementation Strategy

Based on Claude Code architecture, here's how to implement similar tools in Nika:

### Architecture Pattern

```rust
// src/tools/mod.rs - Central tool dispatcher
pub trait ToolExecutor {
    async fn execute(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> Result<ToolResult, ToolError>;
}

// src/tools/filesystem.rs
pub struct FilesystemTools {
    working_dir: PathBuf,
    permissions: PermissionMode,
}

impl FilesystemTools {
    pub async fn read(&self, file_path: &str, limit: Option<usize>, offset: Option<usize>)
        -> Result<ReadResult, ToolError> {
        // Validation
        self.validate_path(file_path)?;

        // Read file
        let content = self.read_file(file_path, limit, offset)?;

        // Format with line numbers
        Ok(ReadResult::with_line_numbers(content))
    }

    pub async fn edit(
        &self,
        file_path: &str,
        old_string: &str,
        new_string: &str,
        replace_all: bool
    ) -> Result<EditResult, ToolError> {
        // Require that file was read first
        self.validate_read_before_edit(file_path)?;

        // Find and replace
        let updated = if replace_all {
            content.replace_all(old_string, new_string)
        } else {
            self.replace_unique(content, old_string, new_string)?
        };

        // Atomic write
        self.atomic_write(file_path, updated)?;

        Ok(EditResult { ... })
    }

    pub async fn glob(&self, pattern: &str, path: Option<&str>)
        -> Result<Vec<PathBuf>, ToolError> {
        // Fast glob matching
        let matches = self.glob_search(pattern, path)?;
        Ok(matches) // Already sorted by modification time
    }

    pub async fn grep(
        &self,
        pattern: &str,
        path: Option<&str>,
        options: GrepOptions
    ) -> Result<GrepResult, ToolError> {
        // ripgrep-based search
        let results = self.regex_search(pattern, path, options)?;
        Ok(results)
    }
}
```

### Key Implementation Details

#### 1. Path Validation

```rust
fn validate_path(&self, file_path: &str) -> Result<PathBuf, ToolError> {
    let path = if file_path.starts_with('/') {
        PathBuf::from(file_path)
    } else {
        self.working_dir.join(file_path)
    };

    // Must be absolute
    if !path.is_absolute() {
        return Err(ToolError::RelativePath);
    }

    // Must be within working directory
    if !path.starts_with(&self.working_dir) {
        return Err(ToolError::OutOfBounds);
    }

    Ok(path)
}
```

#### 2. Read Before Edit Validation

```rust
struct EditContext {
    read_files: HashSet<PathBuf>,
    last_read_content: HashMap<PathBuf, String>,
}

fn validate_read_before_edit(&self, file_path: &str) -> Result<(), ToolError> {
    let path = self.validate_path(file_path)?;
    if !self.read_files.contains(&path) {
        return Err(ToolError::MustReadFirst);
    }
    Ok(())
}
```

#### 3. Atomic Operations

```rust
async fn atomic_write(&self, file_path: &str, content: String) -> Result<(), ToolError> {
    let path = self.validate_path(file_path)?;

    // Write to temporary file
    let temp_path = path.with_extension("tmp");
    tokio::fs::write(&temp_path, &content).await?;

    // Atomic rename
    tokio::fs::rename(&temp_path, &path).await?;

    Ok(())
}
```

#### 4. Fast Glob Implementation

```rust
use globset::GlobBuilder;

async fn glob_search(&self, pattern: &str, base_path: Option<&str>)
    -> Result<Vec<PathBuf>, ToolError> {
    let search_root = base_path
        .map(|p| self.validate_path(p)?)
        .unwrap_or_else(|| self.working_dir.clone());

    let glob = GlobBuilder::new(pattern)
        .build()?
        .compile_matcher();

    // Walk directory and collect matches
    let mut matches = Vec::new();
    let mut walker = walkdir::WalkDir::new(&search_root)
        .into_iter();

    for entry in walker {
        let entry = entry?;
        if glob.matches(entry.path()) {
            matches.push(entry.path().to_path_buf());
        }
    }

    // Sort by modification time (deterministic)
    matches.sort_by_key(|p| std::fs::metadata(p)
        .ok()
        .and_then(|m| m.modified().ok())
        .unwrap_or_else(SystemTime::now));

    Ok(matches)
}
```

#### 5. Regex Search (ripgrep backend)

```rust
use regex::Regex;

struct GrepOptions {
    output_mode: GrepOutputMode,
    context_before: Option<usize>,
    context_after: Option<usize>,
    case_insensitive: bool,
    multiline: bool,
}

async fn regex_search(&self, pattern: &str, path: Option<&str>, options: GrepOptions)
    -> Result<GrepResult, ToolError> {
    // Compile regex with options
    let mut regex_builder = RegexBuilder::new(pattern);
    if options.case_insensitive {
        regex_builder.case_insensitive(true);
    }
    if options.multiline {
        regex_builder.multi_line(true);
    }

    let regex = regex_builder.build()?;

    // Search files
    let search_root = path
        .map(|p| self.validate_path(p)?)
        .unwrap_or_else(|| self.working_dir.clone());

    // For each file, search and collect results
    // Return in appropriate output_mode
}
```

### Integration with Nika

```rust
// In nika runtime executor
pub struct TaskExecutor {
    filesystem_tools: FilesystemTools,
    event_log: EventLog,
}

impl TaskExecutor {
    pub async fn execute_invoke_task(&mut self, task: &Task, params: InvokeParams)
        -> Result<TaskResult, NikaError> {

        // Nika-specific built-in tools
        match params.tool.as_str() {
            "read" => {
                let file_path = params.params.get("file_path")?;
                let limit = params.params.get("limit");
                let offset = params.params.get("offset");

                let result = self.filesystem_tools
                    .read(file_path, limit, offset)
                    .await?;

                self.event_log.push(EventKind::ToolResult {
                    tool_name: "read".to_string(),
                    result: serde_json::to_value(result)?
                });

                Ok(TaskResult::success(result))
            }
            "edit" => { /* similar pattern */ }
            "glob" => { /* similar pattern */ }
            "grep" => { /* similar pattern */ }
            _ => {
                // Fall through to MCP tools
                self.invoke_mcp_tool(params).await
            }
        }
    }
}
```

### Permissions Model

```rust
pub enum PermissionMode {
    Deny,           // All operations denied
    Plan,           // Show plan, request approval
    AcceptEdits,    // Auto-approve edits, request others
    AcceptAll,      // Auto-approve all
}

fn check_permission(&self, tool: &str, mode: PermissionMode) -> Result<(), ToolError> {
    match mode {
        PermissionMode::Deny => Err(ToolError::PermissionDenied),
        PermissionMode::Plan => {
            // Send permission request event
            self.event_log.push(EventKind::PermissionRequest { tool });
            // Wait for response
            Err(ToolError::AwaitingPermission)
        }
        PermissionMode::AcceptEdits if tool == "edit" => Ok(()),
        PermissionMode::AcceptAll => Ok(()),
        _ => Err(ToolError::PermissionDenied),
    }
}
```

### Error Handling Pattern

```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("[NIKA-200] Failed to read file: {0}")]
    ReadError(String),

    #[error("[NIKA-201] Failed to edit file: {0}")]
    EditError(String),

    #[error("[NIKA-202] File must be read before editing")]
    MustReadFirst,

    #[error("[NIKA-203] Path out of bounds: {0}")]
    OutOfBounds(String),

    #[error("[NIKA-204] Permission denied: {0}")]
    PermissionDenied(String),

    #[error("[NIKA-205] Invalid glob pattern: {0}")]
    InvalidGlobPattern(String),

    #[error("[NIKA-206] Invalid regex pattern: {0}")]
    InvalidRegex(String),
}
```

---

## Related Documentation

- **Claude Code CLI Reference:** Full command-line interface documentation
- **Agent SDK Quickstart:** Building agents with Python/TypeScript SDK
- **Custom Tools Guide:** Creating MCP servers with custom tools
- **Nika YAML Specification:** Workflow definition format

---

## Implementation Roadmap for Nika

### Phase 1: Basic Tools (v0.6)
- [ ] Read tool with line offset/limit support
- [ ] Write tool for file creation
- [ ] Edit tool with replace_all logic
- [ ] Glob tool with pattern matching
- [ ] Grep tool with basic regex support

### Phase 2: Advanced Features (v0.7)
- [ ] Permission modes (Plan, AcceptEdits, AcceptAll)
- [ ] Read-before-edit validation
- [ ] Atomic operations with rollback
- [ ] Tool composition patterns
- [ ] Event logging for tool operations

### Phase 3: MCP Integration (v0.8)
- [ ] Expose tools via MCP server
- [ ] Tool naming convention (mcp__nika__read)
- [ ] Integration with NovaNet MCP client
- [ ] Tool result streaming

### Phase 4: Performance Optimization (v0.9)
- [ ] Parallel glob search with tokio
- [ ] ripgrep integration for fast regex
- [ ] File operation caching
- [ ] Result pagination for large datasets

---

## Key Takeaways for Nika Implementation

1. **Tool-first design:** Tools are the primary way Claude interacts with the system
2. **Structured JSON:** All tool parameters and results use JSON Schema
3. **Atomic operations:** File edits must be all-or-nothing (no partial writes)
4. **Path security:** All paths must be absolute and within working directory
5. **Composition over monolith:** Tools work together in patterns, not in isolation
6. **Deterministic results:** Sorting (by modification time, alphabetical) for reproducible behavior
7. **Permission model:** Runtime can enforce different permission levels per tool
8. **Error codes:** Each tool error gets a unique code for diagnostics
9. **Event sourcing:** Tool operations generate events for observability
10. **Streaming results:** Large result sets should be paginated/streamed

