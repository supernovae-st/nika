# 10 -- Security Model

## Overview

Nika implements defense-in-depth security across five domains: command execution, network access, file system access, media processing, and credential management. Security is enforced at multiple layers: the analyzer (Phase 2), the policy enforcer (boot time), and the task executor (runtime).

---

## Command Execution Security (exec:)

### Command Blocklist

The exec verb validates every command against a static blocklist of dangerous patterns. The blocklist is checked after NFKC normalization to prevent Unicode confusable bypass.

**Always blocked (any shell mode):**

| Pattern | Category |
|---------|----------|
| `rm -rf /`, `rm -rf /*`, `rm -rf ~` | Destructive file operations |
| `\| bash`, `\|bash`, `\| sh`, `\|sh` | Remote code execution (pipe to shell) |
| `eval ` | Dynamic execution of untrusted input |
| `mkfifo` | Named pipes (reverse shells) |
| `nc -e`, `nc -c`, `ncat -e`, `ncat -c` | Netcat reverse shell |
| `; rm `, `&& rm `, `\| rm ` | Chained destructive commands |
| `:(){ :\|:& };:` | Fork bombs |
| `python -c "import socket`, `python3 -c "import socket` | Python reverse shell |
| `sudo `, `doas `, `pkexec `, `su ` | Privilege escalation |
| `chmod 777`, `chmod -r 777`, `chmod a+rwx` | Dangerous permission changes |
| `base64 -d \|`, `\| base64 -d` | Base64 encoded payload execution |
| `dd if=` | Disk destruction |
| `rm --recursive`, `rm --force` | Destructive rm long-flag variants |
| `perl -e`, `ruby -e`, `node -e` | Interpreter bypass |
| `env ` | Command wrapper bypass |

**Blocked only in shell mode (`shell: true`):**

| Pattern | Category |
|---------|----------|
| `$(` | Command substitution |
| `` ` `` | Backtick command substitution |

### Unicode Confusable Protection

All commands undergo NFKC normalization before blocklist checking:

```rust
use unicode_normalization::UnicodeNormalization;

fn normalize(input: &str) -> String {
    input.nfkc().collect()
}
```

This prevents bypass via:
- **Fullwidth characters:** `ｒｍ` (U+FF52, U+FF4D) normalizes to `rm`
- **Math bold/italic:** `𝘀𝘂𝗱𝗼` (U+1D600 range) normalizes to `sudo`
- **Combining characters:** Zero-width joiners stripped during normalization

### Control Character Detection

Commands are scanned for control characters (null bytes, escape sequences) which are unconditionally blocked. This prevents shell injection via terminal escape sequences.

### Shell Mode vs Direct Mode

| Aspect | `shell: false` (default) | `shell: true` |
|--------|------------------------|---------------|
| Execution | `shlex` tokenize + direct spawn | `sh -c "<command>"` |
| Blocklist | Standard blocklist | Standard + shell-mode blocklist |
| Escaping | No shell interpretation | Full shell interpretation |
| `$(...)` | Literal text | Command substitution (blocked) |
| Risk | Low | Higher (more attack surface) |

---

## Network Security (fetch:)

### SSRF Protection

The `PolicyEnforcer` blocks all requests to cloud metadata endpoints and loopback addresses. These are **hardcoded** and cannot be overridden by user configuration:

```rust
const SSRF_BLOCKED_HOSTS: &[&str] = &[
    "169.254.169.254",          // AWS metadata
    "metadata.google.internal",  // GCP metadata
    "100.100.100.200",          // Alibaba Cloud metadata
    "localhost",
    "127.0.0.1",
    "::1",
    "0.0.0.0",
];
```

### Host Restrictions

The `PolicyConfig` supports allow/block lists for network access:

```toml
# .nika/config.toml
[policy]
allowed_hosts = ["api.example.com", "*.github.com"]
blocked_hosts = ["evil.com"]
```

The `PolicyEnforcer` checks URLs before the HTTP client sends the request.

### TLS

Nika uses `reqwest` with `rustls-tls` (not native TLS). This ensures consistent TLS behavior across platforms without depending on system OpenSSL.

### Timeout and Redirect Control

- Default fetch timeout: 60 seconds (configurable per-task)
- Default connect timeout: 20 seconds
- Default redirect limit: 10 hops (configurable via `follow_redirects: false`)
- User-Agent: `nika/<version>`

---

## File System Security

### File Tool Permission Model

File tools (`nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`) enforce a permission model:

| Mode | Behavior |
|------|----------|
| `Deny` | All file operations denied |
| `Plan` | Ask before each operation (default for executor) |
| `AcceptEdits` | Auto-approve edits, ask for others |
| `YoloMode` | Auto-approve everything (used by agents) |

### Path Validation

All file tool paths are validated to be:

1. **Absolute paths only:** Relative paths are rejected
2. **Within working directory:** Paths outside the security boundary are rejected (NIKA-204)
3. **No traversal:** `../` patterns are detected and blocked (NIKA-204)

### Media Import Security

The `nika:import` tool validates import paths:

```rust
pub fn validate_import_path(path: &Path, workspace: &Path) -> Result<(), MediaError>;
```

Checks:
- No `../` traversal
- No symlinks pointing outside workspace
- File size pre-check (50 MB default limit)
- MIME type detection for content validation

---

## Media Processing Security

### Image Decode Safety

**Rule:** Never use `image::load_from_memory()` directly.

```rust
pub fn decode_image_safe(data: &[u8]) -> Result<DynamicImage, MediaError> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);  // 256 MB

    let reader = image::io::Reader::new(Cursor::new(data))
        .with_guessed_format()?;
    reader.with_limits(limits).decode()
}
```

The `Limits` struct prevents decompression bombs (tiny compressed files that expand to gigabytes of memory).

### SVG Sanitization

**Rule:** Always call `sanitize_svg()` BEFORE `usvg` parsing.

SVG files can contain:
- `<script>` elements
- External entity references (`<!ENTITY>`)
- `xlink:href` to external resources
- Embedded JavaScript event handlers

The sanitizer strips all potentially dangerous elements before the SVG tree is parsed.

### Operation Timeout

Every media operation is wrapped in `tokio::time::timeout()` with a 30-second default:

```rust
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
```

This prevents hung operations (e.g., corrupt files causing infinite loops in decoders).

---

## Credential Management

### Priority Order

1. **Environment variables** (highest): `ANTHROPIC_API_KEY`, etc.
2. **System keychain** (feature: `native-keychain`): macOS Keychain, Windows Credential Manager, Linux Secret Service
3. **Nika daemon** (via nika-daemon crate): Unified secret management via IPC
4. **Config file** (lowest): `~/.config/nika/config.toml`

### Keychain Safety

**Critical:** `cargo test` (without `--lib`) runs contract tests that trigger macOS Keychain popups. Always use `--lib` for safe testing. For CI environments, use environment variables exclusively.

### Credential Zeroization

The `secrecy` and `zeroize` crates are used to ensure API keys are wiped from memory when no longer needed:

```rust
use secrecy::SecretString;
use zeroize::Zeroize;
```

---

## Policy Enforcement

### PolicyConfig

```rust
pub struct PolicyConfig {
    pub allowed_commands: Vec<String>,
    pub blocked_commands: Vec<String>,
    pub allowed_hosts: Vec<String>,
    pub blocked_hosts: Vec<String>,
    pub token_budget: Option<u64>,
}
```

### PolicyDecision

```rust
pub enum PolicyDecision {
    Allow,
    Block(String),
    RequiresApproval(String),
}
```

The `PolicyEnforcer` is instantiated during the boot sequence and shared across all task executors. It checks:

1. **exec: commands** against allowed/blocked command patterns
2. **fetch: URLs** against allowed/blocked host patterns + SSRF blocklist
3. **Token usage** against the configured budget

### Token Budget

```rust
pub struct TokenBudget {
    pub limit: Option<u64>,
    pub used: u64,
}
```

The token budget tracks cumulative LLM token usage across all tasks in a workflow execution. When the budget is exceeded, new infer/agent tasks fail with NIKA-165.

---

## Boot Sequence Security

The 7-phase boot sequence includes security validation:

1. **Config Discovery**: Find `.nika/` directory
2. **Config Validation**: Parse and validate `config.toml`
3. **Memory Loading**: Load memory files (read-only)
4. **Secrets Loading**: Load from keychain/daemon (encrypted transit)
5. **MCP Startup**: Launch MCP servers (process isolation)
6. **Provider Validation**: Verify API key format
7. **Ready**: System operational

Phase 4 (Secrets Loading) uses the nika daemon's IPC protocol for encrypted credential transfer when available, falling back to OS keychain access.

Phase 5 (MCP Startup) launches MCP servers as child processes with controlled environments. Only explicitly configured environment variables are passed; the server cannot access the parent process's full environment.

---

## Structured Output Security

### JSON Schema Validation

When `output.format: json` with a schema is configured, all LLM output undergoes schema validation via the `jsonschema` crate. This prevents:

- **Data exfiltration**: Unexpected fields that might contain sensitive data
- **Type confusion**: Strings where numbers are expected, etc.
- **Schema bypass**: LLM output that does not conform to the expected structure

The 5-layer defense system ensures that invalid output is either repaired or rejected, never silently accepted.

### Agent Tool Access Control

Agent tool access is explicitly controlled:

```yaml
agent:
  prompt: "..."
  tools: ["nika:read", "nika:grep"]  # Only read and search
```

When `tools:` is specified, only listed tools are available. This prevents agents from accessing write operations unless explicitly granted. File tools run with `PermissionMode::YoloMode` when attached to agents, so the `tools:` list is the primary access control mechanism.

---

## Threat Model Summary

| Threat | Mitigation |
|--------|------------|
| Shell injection via exec: | Command blocklist + NFKC normalization + control char detection |
| SSRF via fetch: | Hardcoded blocklist of metadata/loopback endpoints |
| Path traversal via file tools | Absolute path requirement + boundary enforcement |
| Decompression bomb via media | Image limits + CAS size limits + decompression caps |
| SVG XSS/XXE | Mandatory sanitization before parsing |
| API key leakage | Env vars > keychain > config file priority; zeroization |
| Unicode confusable bypass | NFKC normalization before all blocklist checks |
| Agent tool abuse | Explicit tool list + depth limits + cost limits |
| Fork bomb via exec: | Blocked in static blocklist |
| Privilege escalation | sudo/doas/pkexec/su blocked in blocklist |

---

## Reporting Security Issues

Security issues should be reported via GitHub Security Advisories on the Nika repository. Do not file public issues for security vulnerabilities.
