# Episode 5: Security by Design -- How Nika Protects Against AI Workflow Attacks

## Metadata

| Field | Value |
|-------|-------|
| **Series** | Building Nika -- A Rust AI Engine from Scratch |
| **Episode** | 05 |
| **Duration** | ~25 minutes |
| **Topics** | Command injection, Unicode bypass, path traversal, SVG sanitization, env validation |
| **Guest Suggestions** | An application security engineer, an AI safety researcher, a Unicode expert |
| **Audience** | Developers building production AI systems, security-conscious engineers |
| **Prerequisites** | Episode 2 (understanding of exec: and fetch:) |

---

## Cold Open (30 seconds)

[MUSIC: Tense, precise, like a heist movie]

**Host:** An attacker submits a workflow with this task:

```yaml
- id: innocent_looking
  exec: "echo hello && ｒｍ -rf /"
```

Notice anything? That `rm` is not ASCII. Those are fullwidth Unicode characters -- U+FF52 and U+FF4D. They look identical in most terminals. Most blocklists would miss them completely.

[PAUSE]

Nika does not miss them. It normalizes Unicode before checking. And that is just one of six security layers protecting every workflow execution.

[MUSIC FADES]

---

## Intro (1 minute)

**Host:** Episode 5. Security.

This is the episode where we stop talking about features and start talking about what happens when someone tries to break them. Because here is the uncomfortable truth about AI workflow engines: they are, by design, remote code execution machines. You give them a YAML file, and they execute commands, call APIs, send data to LLMs. If the YAML is malicious -- or if a malicious input gets interpolated into a command -- the engine becomes a weapon.

Most workflow engines treat security as an afterthought. A blocklist here, a warning there. Nika treats it as a first-class architectural concern with 1,149 lines of dedicated security code, Unicode normalization, zero-width character stripping, and defense-in-depth across six attack surfaces.

Let us go through each one.

---

## Segment 1: Command Injection and the Blocklist (8 minutes)

**Host:** The `exec:` verb runs shell commands. This is inherently dangerous. The question is not whether to allow it -- workflows need to interact with the operating system -- but how to prevent abuse.

Nika's primary defense is a command blocklist. Let me read you the actual patterns from the source code, because the specificity matters:

[CODE EXAMPLE]
```rust
const BLOCKLIST: &[&str] = &[
    // Destructive file operations
    "rm -rf /", "rm -rf /*", "rm -rf ~",
    // Remote code execution (piping downloads to shell)
    "| bash", "|bash", "| sh", "|sh",
    // Dynamic execution
    "eval ",
    // Reverse shell infrastructure
    "mkfifo", "nc -e", "nc -c", "ncat -e", "ncat -c",
    // Chained destructive commands
    "; rm ", "&& rm ", "| rm ",
    // Fork bombs
    ":(){ :|:& };:",
    // Scripting runtime reverse shells
    "python -c \"import socket", "python3 -c \"import socket",
    // Privilege escalation
    "sudo ", "doas ", "pkexec ",
    // Dangerous permissions
    "chmod 777", "chmod -r 777", "chmod a+rwx",
    // Encoded payload execution
    "base64 -d |", "base64 --decode |",
    "| base64 -d", "| base64 --decode",
    // Disk destruction
    "dd if=",
    // Long-flag variants
    "rm --recursive", "rm --force",
    // Interpreter bypass
    "perl -e", "ruby -e", "node -e",
    // Command wrapper bypass
    "env ",
    // Privilege escalation (su)
    "su ",
];
```

[EMPHASIS] Notice the patterns are not just obvious ones like `rm -rf /`. They include:

**Chained destructive commands** -- `; rm `, `&& rm `, `| rm `. These catch attempts to hide `rm` after a semicolon or pipe.

**Interpreter bypass** -- `perl -e`, `ruby -e`, `node -e`. If you block `python -c`, an attacker just uses Perl or Ruby instead. Nika blocks all common scripting language one-liner patterns.

**Command wrapper bypass** -- `env `. The `env` command can prefix any other command, bypassing blocklists that check for commands at the start of the string. `env rm -rf /` executes `rm -rf /`.

**Encoded payload execution** -- `base64 -d |`. An attacker encodes a malicious command in Base64, pipes it to the decoder, and pipes the result to a shell. Nika blocks the base64-decode-pipe pattern in both directions.

And there is a separate `SHELL_MODE_BLOCKLIST` that only applies when `shell: true`:

[CODE EXAMPLE]
```rust
const SHELL_MODE_BLOCKLIST: &[&str] = &[
    "$(",  // Command substitution
    "`",   // Backtick command substitution (legacy)
];
```

These patterns are only dangerous in shell mode. In shell-free mode (shlex parsing), `$(` is a literal string, not a command substitution.

[PAUSE]

**Host:** But here is where it gets really interesting. The blocklist alone is not enough. An attacker can bypass it with Unicode.

### Unicode NFKC Normalization

Unicode has thousands of characters that look like ASCII characters but have different code points. Fullwidth Latin letters (`A` through `z`), mathematical bold/italic variants, superscript and subscript letters, ligatures -- all visually identical or near-identical to their ASCII counterparts.

An attacker could write `sudo` using fullwidth characters: `ｓｕｄｏ`. In most terminals, this looks exactly like `sudo`. But a naive string comparison -- `"sudo" == "ｓｕｄｏ"` -- returns false.

Nika applies NFKC normalization (Compatibility Decomposition followed by Canonical Composition) before checking the blocklist. NFKC maps fullwidth characters to their ASCII equivalents, mathematical variants to base characters, and ligatures to their component characters.

[CODE EXAMPLE]
```rust
fn normalize_for_blocklist(s: &str) -> String {
    s.nfkc()                                    // NFKC normalize
        .filter(|c| !ZERO_WIDTH_CHARS.contains(c)) // Strip invisible chars
        .collect::<String>()
        .split_whitespace()                      // Normalize whitespace
        .collect::<Vec<_>>()
        .join(" ")
}
```

### Zero-Width Character Stripping

Even after NFKC normalization, some invisible characters remain. Zero Width Space (U+200B), Zero Width Joiner (U+200D), Zero Width Non-Joiner (U+200C), Soft Hyphen (U+00AD), Word Joiner (U+2060), and the BOM (U+FEFF).

An attacker could insert a Zero Width Space inside `sudo` -- making it `su\u{200B}do`. Visually identical. The blocklist would not match `sudo` because the invisible character breaks the pattern. Nika strips all seven known zero-width characters before comparison.

### Control Character Validation

Before any blocklist check, Nika validates the entire command string for control characters. Characters with code points 0x00 through 0x1F (except newline 0x0A and tab 0x09) are rejected. This blocks null byte injection, escape sequence attacks, and other control character exploits.

[EMPHASIS] The three layers work in sequence: control character validation, then NFKC normalization with zero-width stripping, then blocklist matching. All three must pass for a command to execute.

---

## Segment 2: Path Traversal and SSRF (8 minutes)

**Host:** Command injection is not the only attack vector. Let us talk about path traversal and Server-Side Request Forgery.

### Path Traversal Protection

When Nika imports a file via `nika:import`, the user provides a file path. A malicious path like `../../etc/passwd` or `/etc/shadow` should never be accepted. Nika's `validate_import_path()` function resolves the path to its canonical form and verifies it stays within the intended directory.

But path traversal is not just about `../`. On some systems, path separators can be mixed (`\` and `/`). Null bytes can truncate path strings in some languages (though not in Rust, where strings are not null-terminated). And symlinks can point outside the intended directory.

Nika uses Rust's `std::fs::canonicalize()` to resolve all symlinks and normalize the path before validation. This is more robust than string-based path cleaning.

### SSRF Protection in Vision

The `infer:` verb supports `image_url` for sending remote images to vision-capable LLMs. Without protection, an attacker could use this to make Nika fetch internal network resources -- a classic Server-Side Request Forgery attack.

[CODE EXAMPLE]
```yaml
# BLOCKED: HTTP (not HTTPS)
- id: ssrf_attempt
  infer:
    content:
      - type: image
        image_url: "http://169.254.169.254/metadata"  # AWS metadata endpoint
```

Nika enforces HTTPS-only for `image_url` sources. No HTTP, no `file://`, no `ftp://`, no internal IP ranges. The URL scheme must be exactly `https://`.

### Environment Variable Validation

Nika reads API keys from environment variables (ANTHROPIC_API_KEY, etc.). But what if a workflow template interpolates an environment variable into a command?

[CODE EXAMPLE]
```yaml
- id: dangerous
  exec: "curl -H 'Authorization: {{env.SECRET_TOKEN}}' https://api.example.com"
```

If `SECRET_TOKEN` contains shell metacharacters, this could be exploited. Nika's template resolution happens in a controlled context where environment variable values are sanitized before interpolation into exec commands.

---

## Segment 3: Media Pipeline Security and Defense in Depth (6 minutes)

**Host:** We covered media security in Episode 4, but let me contextualize it within the broader security architecture.

### Image Decode Safety

A malicious image file can crash a process by specifying enormous dimensions in its header. The file might be 1 KB, but its header says it is 100,000 by 100,000 pixels -- requiring 40 GB of memory to decode. This is called a decompression bomb or pixel flood attack.

[CODE EXAMPLE]
```rust
// The WRONG way (vulnerable to pixel flood)
let img = image::load_from_memory(&bytes)?;

// The RIGHT way (Nika's approach)
let img = decode_image_safe(&bytes, &MediaLimits {
    max_width: 16384,
    max_height: 16384,
    max_alloc: 512 * 1024 * 1024,  // 512 MB max allocation
})?;
```

`decode_image_safe()` sets explicit limits on the image decoder before attempting to decode. If the image header exceeds these limits, decoding is rejected without allocating memory.

### SVG Sanitization

SVGs are a special case because they are XML documents. A malicious SVG can contain:

- `<script>` tags with JavaScript (XSS if rendered in a browser)
- External entity references (`<!ENTITY xxe SYSTEM "file:///etc/passwd">`)
- Embedded `<iframe>` or `<object>` elements
- External CSS imports (`@import url(...)`)
- Foreign namespace elements (`<foreignObject>` with embedded HTML)

Nika always calls `sanitize_svg()` before passing any SVG to the usvg parser. The sanitizer strips all non-SVG elements and attributes, removing any attack surface.

### The Full Security Stack

[EMPHASIS] Let me enumerate all six security layers in Nika:

1. **Command blocklist** with NFKC normalization and zero-width stripping (exec: verb)
2. **Control character validation** (exec: verb)
3. **Shell mode restrictions** -- command substitution blocked in shell mode (exec: verb)
4. **Path traversal protection** -- canonical path resolution for file operations
5. **SSRF protection** -- HTTPS-only for external image URLs (infer: vision)
6. **Media decode safety** -- size limits, SVG sanitization, decompression bomb protection

These layers operate independently. Even if an attacker bypasses one layer, the remaining layers provide defense in depth.

[PAUSE]

**Host:** Why does all of this matter more for AI workflow engines than for traditional automation tools?

Because AI workflows process untrusted inputs. An LLM might suggest a command. A web scrape might return unexpected content. A user might upload a malicious file. The workflow engine sits at the intersection of all these inputs, and if it does not validate each one, it becomes the attack vector.

Traditional CI/CD pipelines run in controlled environments with trusted inputs. AI workflows, by design, handle arbitrary inputs from the internet, from users, and from LLMs. The security model must account for this.

### The LLM-Suggested Command Problem

Here is a scenario that keeps me up at night. You have an agent with `exec:` access. The agent asks the LLM: "What command should I run to clean up temporary files?" The LLM, because it was trained on internet data that includes malicious scripts, might suggest something harmful. Not intentionally -- LLMs do not have intent -- but because the training data included that pattern.

In a workflow engine without blocklists, the agent would blindly execute whatever the LLM suggests. In Nika, the security module intercepts the command before execution. The blocklist catches destructive patterns regardless of whether they came from a human author, an LLM response, or a template interpolation.

This is a fundamental difference between securing AI workflows and securing traditional automation. In traditional automation, the commands are authored by humans and reviewed in code review. In AI workflows, commands can be generated at runtime by a probabilistic model. The security surface is fundamentally different, and Nika's security model accounts for this.

### Error Codes and Auditability

Every security violation produces a structured NIKA-XXX error code:

[CODE EXAMPLE]
```
NIKA-053: BlockedCommand
  Command: "sudo apt-get install malware"
  Reason: Blocklisted pattern: "sudo "
  Normalized form: "sudo apt-get install malware"
```

These error codes are logged, traced via the event system, and visible in the TUI. Security is not silent -- when Nika blocks a dangerous operation, it tells you exactly what it blocked, why it was blocked, and what the normalized form of the input was. This auditability is critical for debugging false positives and for compliance reporting.

The structured error codes also mean you can build monitoring dashboards that track security events across workflow runs. How many blocked commands per day? What patterns are triggering most frequently? Is someone testing the blocklist? These questions become answerable.

---

## Wrap-up & Preview (2 minutes)

**Host:** Security in Nika is not a feature -- it is an architectural principle applied at every layer.

Command injection: blocked by a comprehensive blocklist with Unicode normalization that handles fullwidth characters, mathematical variants, zero-width space injection, and control characters.

Path traversal: blocked by canonical path resolution that follows symlinks and normalizes separators.

SSRF: blocked by HTTPS-only enforcement on external URLs.

Media attacks: blocked by decode safety limits, SVG sanitization, size pre-checks, and decompression bomb protection.

Every one of these protections has tests. The security module alone has extensive test coverage verifying that each bypass technique is caught -- fullwidth `rm`, zero-width space injection, control character embedding, shell mode substitution.

[PAUSE]

Next episode: the learning system. Nika ships with a 12-level interactive course that teaches AI workflows from "Hello World" to full production orchestration. 44 exercises with templates, solutions, progressive hints, and a constellation progress map. Plus 200+ showcase workflows you can extract and run. Episode 6: "Learning AI Workflows -- The 12-Level Liberation Course."

[MUSIC: Outro theme]

---

## Show Notes

### Security Layers
| Layer | Protects Against | Implementation |
|-------|-----------------|----------------|
| Command Blocklist | Destructive commands, reverse shells, privilege escalation | 28+ patterns, case-insensitive |
| NFKC Normalization | Unicode confusable bypass (fullwidth, math variants) | `unicode_normalization` crate |
| Zero-Width Stripping | Invisible character injection (U+200B, U+200D, etc.) | 7 character types stripped |
| Control Character Check | Null bytes, escape sequences | 0x00-0x1F rejected (except \n, \t) |
| Shell Mode Blocklist | Command substitution ($(), backticks) | Only in shell: true mode |
| Path Traversal | Directory escape, symlink following | `std::fs::canonicalize()` |
| SSRF Protection | Internal network access via image URLs | HTTPS-only enforcement |
| Decode Safety | Pixel flood, decompression bombs | `decode_image_safe()` with Limits |
| SVG Sanitization | XSS, XXE, embedded objects | `sanitize_svg()` before parsing |
| Size Pre-Check | Memory exhaustion from large files | 50 MB default import limit |
| Decompression Limit | zstd decompression bombs | 200 MB max decompressed size |
| Media Budget | Disk exhaustion | 500 MB per run, atomic tracking |

### Error Codes for Security Violations
| Code | Error |
|------|-------|
| NIKA-053 | BlockedCommand (blocklist match) |
| NIKA-054 | InvalidCommandString (control characters) |
| NIKA-055 | PathTraversal (directory escape attempt) |
| NIKA-290 | MediaToolError (decode safety violation) |
| NIKA-297 | MediaSecurityError (SVG sanitization failure) |

### Attack Techniques Defended Against
- Fullwidth Unicode bypass (`sudo` vs `ｓｕｄｏ`)
- Mathematical variant bypass (`sudo` vs `\u{1D600}` range)
- Zero Width Space keyword splitting (`su\u{200B}do`)
- Null byte injection (`command\x00malicious`)
- Command substitution (`$(evil)`, `` `evil` ``)
- Base64 encoded payloads (`echo payload | base64 -d | sh`)
- Path traversal (`../../etc/passwd`)
- Symlink escape (`/tmp/innocent -> /etc/shadow`)
- SSRF via image_url (`http://169.254.169.254/metadata`)
- Pixel flood (tiny file, huge decoded dimensions)
- SVG XSS (`<svg><script>...</script></svg>`)
- Decompression bomb (small zstd, huge decompressed)
