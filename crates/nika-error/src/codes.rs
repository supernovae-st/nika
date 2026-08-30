// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIKA-XXX code registry — dual wire format + typed struct.
//!
//! Each error code is a [`NikaCode`] with a numeric id, [`Category`],
//! [`Severity`], and a kebab-case slug for documentation URLs.
//!
//! Wire format: `"NIKA-001"` (Display impl, stable across versions).

use std::fmt;

/// Structured error code combining numeric id, category, severity, and slug.
///
/// # Display
///
/// Formats as `"NIKA-{num:03}"` for wire stability:
/// ```
/// use nika_error::codes::NIKA_001;
/// assert_eq!(format!("{NIKA_001}"), "NIKA-001");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NikaCode {
    /// Numeric identifier (1..=9999). Unique across the entire registry.
    pub num: u16,
    /// Functional category for grouping and routing.
    pub category: Category,
    /// Severity level.
    pub severity: Severity,
    /// Kebab-case slug for documentation URLs (e.g. `"validation-failed"`).
    pub slug: &'static str,
}

impl fmt::Display for NikaCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NIKA-{:03}", self.num)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for NikaCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for NikaCode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(deserializer)?;
        lookup(s).ok_or_else(|| serde::de::Error::custom(format!("unknown code: {s}")))
    }
}

/// Functional category for error grouping and routing.
///
/// Numeric ranges are a convention (not enforced at type level). The
/// authoritative allocation lives in `nika-kernel::errors` (the hub):
/// Core 001-049, Shell 050-099, `FileIo` 100-139, Http 140-189,
/// Auth 190-229, Mcp 230-279, Schema 280-329, Provider 330-379,
/// Shield 380-429 (reserved · crate not yet admitted), Verb 430-479,
/// Runtime 480-529, Memory 600-649, `WasmPlugin` 700-749,
/// Sandbox 750-799, Observability 800-819, Screen 1000-1099,
/// Ocr 1100-1199, A11y 1200-1299 (M2 computer-use L1 ranges per ADR-081),
/// Access 1800-1849 (execution access · D-2026-08-04-N1).
/// `Binding` is a reserved category variant with no allocated range yet
/// (its original 330-379 slot was reassigned to Provider on 2026-05-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum Category {
    Core,
    Shell,
    FileIo,
    Http,
    Auth,
    Mcp,
    Schema,
    Binding,
    Provider,
    Verb,
    Runtime,
    /// Memory subsystem (600-649).
    Memory,
    /// WASM plugin host execution (700-749).
    WasmPlugin,
    /// Capability-based sandbox (750-799).
    Sandbox,
    /// Observability/telemetry sinks (800-819).
    Observability,
    /// Screen capture (1000-1099 · M2.1 nika-screen · ADR-081).
    Screen,
    /// OCR text extraction (1100-1199 · M2.2 nika-ocr · ADR-081).
    Ocr,
    /// Accessibility-tree query (1200-1299 · M2.3 nika-a11y · ADR-081).
    A11y,
    /// Synthetic input dispatch (1300-1399 · M2.4 nika-input · ADR-081).
    Input,
    /// Browser automation (1400-1499 · nika-browser · ADR-081).
    Browser,
    /// Vision inference (NIKA-1500..1599 · `VisionModel` · `nika-vision-local` M2.6).
    Vision,
    /// Audio inference (NIKA-1600..1699 · stt/tts/vad · `ai::audio` seam R6).
    Audio,
    /// Execution access resolution (NIKA-1800..1849 · D-2026-08-04-N1 ·
    /// the admission-time resolver, the `--access` pin, the P3+ harness
    /// probes — `model:` picks the intelligence, access picks the path).
    Access,
}

impl Category {
    /// The canonical kebab-case label (matches the `serde(rename_all =
    /// "kebab-case")` wire form). The match is EXHAUSTIVE on purpose:
    /// `#[non_exhaustive]` only binds downstream crates, so adding a
    /// variant here is a COMPILE error until this mapping learns it — the
    /// label can never silently drift to a fallback (the bug a consumer's
    /// hand-rolled copy invites).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Shell => "shell",
            Self::FileIo => "file-io",
            Self::Http => "http",
            Self::Auth => "auth",
            Self::Mcp => "mcp",
            Self::Schema => "schema",
            Self::Binding => "binding",
            Self::Provider => "provider",
            Self::Verb => "verb",
            Self::Runtime => "runtime",
            Self::Memory => "memory",
            Self::WasmPlugin => "wasm-plugin",
            Self::Sandbox => "sandbox",
            Self::Observability => "observability",
            Self::Screen => "screen",
            Self::Ocr => "ocr",
            Self::A11y => "a11y",
            Self::Input => "input",
            Self::Browser => "browser",
            Self::Vision => "vision",
            Self::Audio => "audio",
            Self::Access => "access",
        }
    }
}

/// Severity level for an error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// The canonical kebab-case label (exhaustive · compile-forced · same
    /// rationale as [`Category::as_str`]).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

mod registry;
pub use registry::*;

/// Returns an actionable help message for a given code.
///
/// Every registered code has a help string. This is used by miette's
/// Help text for the Verb range (430-479 · infer 430-439 · exec 440-449 ·
/// invoke 450-459 · agent 460-469). Split out of `code_help` to keep it under the 100-line
/// function cap as the verb crates land.
fn verb_help(num: u16) -> &'static str {
    match num {
        430 => {
            "The provider call failed during `infer`. Check the provider error chained below (credentials, rate limits, connectivity)."
        }
        431 => {
            "The model output never satisfied the task `schema:` within the retry budget. Simplify the schema, raise max_tokens, or pick a schema-capable model."
        }
        432 => {
            "An `infer` parameter is invalid. Prompt must be non-empty; temperature must be within 0-2."
        }
        433 => {
            "The `model:` string did not resolve. Use `provider/model` with a provider from the canonical catalog and ensure its API key is configured."
        }
        434 => {
            "The provider returned no token usage for a priced model, so the ledger cannot bill the call honestly — it refuses rather than record $0 for real spend (fail-closed). Use a backend that reports usage, or drop the priced model's budget reliance explicitly."
        }
        435 => {
            "The provider spent tokens yet the visible answer is empty — a thinking model ate the whole budget on its reasoning trace. Raise `max_tokens` (the reasoning share lives inside it), set an explicit `thinking.budget_tokens`, or pick a no-think variant."
        }
        440 => {
            "The command exited non-zero. Inspect stderr, or use capture: structured to branch on the exit code instead of failing."
        }
        441 => {
            "Shell execution failed before or during the run. Check the command exists, is not blocklisted, and completes within the timeout."
        }
        442 => "An `exec` parameter is invalid. Command must be a non-empty string.",
        450 => {
            "The `invoke` tool id did not resolve. Use `nika:<tool>` or `mcp:<server>/<tool>`; check the builtin name or MCP server registry."
        }
        451 => {
            "The tool ran but reported an error. Inspect the tool's output content for the failure detail."
        }
        452 => {
            "Tool dispatch failed. Under `nika test` this is BY DESIGN: the mock plane simulates the model, not effects, so the effect tools (`nika:write` · `edit` · `fetch` · `notify` · `image_generate` · `tts_generate` · `chart` · `image_fx`) and anything outside the `nika:` namespace refuse — reach for `nika run` when you want the effect, or let the task rehearse through `on_error: recover` (every shipped skeleton carries that armor). Elsewhere: a timeout, an execution error, or an unavailable tool system — check the MCP server or builtin availability."
        }
        460 => {
            "The agent hit max_turns without completing. Raise max_turns, tighten the prompt toward the nika:done sentinel, or reduce tool round-trips."
        }
        461 => {
            "The agent exhausted max_tokens_total. Raise the budget or reduce turn/context size; the last assistant message rides error.details.partial_output."
        }
        462 => {
            "The model requested a tool outside the agent whitelist. Security boundaries are not model-negotiable: add the tool to `tools:` if intended."
        }
        463 => {
            "A provider call failed mid-loop. Check the chained provider error (credentials, rate limits, connectivity)."
        }
        464 => {
            "The agent's final answer never satisfied the task `schema:` — the engine constrains the final turn to the schema and re-asks on a miss, so this means the model could not produce a conforming object within the retry budget. Simplify the schema, or check it is satisfiable for this task."
        }
        465 => {
            "An `agent` parameter is invalid. Prompt must be non-empty; temperature must be within 0-2."
        }
        466 => {
            "The tool-definition source failed (builtin catalog or MCP tools/list). Check the MCP server availability for mcp:* whitelist entries."
        }
        469 => {
            "The provider returned no token usage for a priced model, so the budget cannot meter the call — the loop refuses rather than continue invisibly (fail-closed). Point at a backend that reports usage (every first-party wire does), or accept the spend unbudgeted explicitly."
        }
        _ => "Verb execution failed. Check the task definition against the spec for this verb.",
    }
}

/// `help()` diagnostic and by the display layer for user-facing output.
#[must_use]
pub fn code_help(code: NikaCode) -> &'static str {
    match code.num {
        1 => "Check your workflow YAML syntax and field values.",
        2 => "Referenced item not found in catalogs or task outputs.",
        3 => "Feature not supported in current configuration.",
        10 => "Catalog TOML is malformed. Check syntax near the reported line and column.",
        11 => {
            "Catalog schema version does not match the expected version. Update the `schema` field."
        }
        12 => {
            "Two capability rules conflict for the same scope. Check rule ordering in model-capabilities.toml."
        }
        13 => {
            "A pricing axis value is out of valid range. Ensure rates are non-negative and finite."
        }
        14 => "max_output_tokens exceeds context_window_tokens. Fix the model capability rule.",
        15 => "Unrecognised json_mode value. Valid values: none, object, schema.",
        16 => {
            "The schedule declaration is invalid. Repair the named schedule field and apply it again."
        }
        17 => {
            "The schedule planner refused to guess. Remove unsupported hash jitter or repair the canonical cadence."
        }
        18 => {
            "Durable schedule state could not be read or changed safely. Check storage integrity, permissions, and configured bounds."
        }
        50..=99 => {
            "Shell/process execution failed. Check command path, permissions, and timeout settings."
        }
        100..=139 => {
            "File I/O or blob operation failed. Check file paths, permissions, and storage availability."
        }
        140..=189 => {
            "HTTP request failed. Check endpoint URL, network connectivity, and SSRF allowlist."
        }
        234 => {
            "The tool-definition source could not enumerate (builtin catalog unloadable or MCP tools/list unreachable). Check the backing source."
        }
        230..=279 => {
            "MCP tool call failed. Check tool name, parameters, and MCP server availability."
        }
        280..=329 => {
            "Schema/workflow validation failed. Check the `.nika.yaml` envelope, task ids, verbs, and field values against the spec."
        }
        330..=379 => {
            "AI provider error. Check the model name, API credentials, rate limits, and provider connectivity."
        }
        380..=429 => {
            "Shield security policy blocked the operation. Check trust levels, capability grants, and injection/canary guards."
        }
        430..=479 => verb_help(code.num),
        601 => {
            "Memory store unavailable. Verify the configured backend (Oxigraph / RocksDB / runtime) is initialised and reachable."
        }
        602 => {
            "Memory fact not found. The id does not exist in the store; verify the id is correct and not yet evicted."
        }
        603 => {
            "Embedding provider failed. Transient — retry-eligible. Check provider connectivity and credentials."
        }
        604 => {
            "Memory storage layer error. Transient — IO / cache / RocksDB-level failure. Retry-eligible after a brief backoff."
        }
        605 => {
            "Signed-memory store error (F-P8 · nika-store). The `.nika/memory/<store>/` envelope could not be written, read, or parsed — check the store dir permissions and that no non-entry files ride the layout. A rejected ENTRY is not this error: recall names it in the verdict (unsigned · bad signature · key mismatch)."
        }
        600 | 606..=649 => {
            "Memory subsystem reported an error. Check store availability, embedding provider, and tenant quotas."
        }
        1000..=1099 => {
            "Screen capture failed. Check display connectivity, capture consent (ConsentState · guard 7), and the OS screen-recording permission."
        }
        1100..=1199 => {
            "OCR failed. Check the model files (with_models path), frame format, and region bounds."
        }
        1200..=1299 => {
            "Accessibility-tree query failed. Check the OS accessibility permission and that a focused application exists."
        }
        1300..=1399 => {
            "Synthetic input failed. Check input/accessibility consent (ConsentProof TTL) and the OS input-monitoring permission."
        }
        1400..=1499 => {
            "Browser automation failed. Check the browser session, the target URL/selector, and the automation backend."
        }
        1500..=1599 => {
            "Vision inference failed. Check the model is available, the frame is valid RGBA8, and a vision backend is installed."
        }
        1600..=1699 => {
            "Audio inference failed. Check the model/voice is available, the clip format (PCM s16le), and an audio backend is installed."
        }
        1700..=1799 => run_code_help(code.num),
        1800..=1849 => access_code_help(code.num),
        700..=749 => {
            "WASM plugin host reported an error. Check plugin manifest and capability grants."
        }
        750..=799 => "Sandbox denied or failed. Verify capability allowlist and platform support.",
        800..=819 => "Observability sink rejected the event. Check exporter configuration.",
        999 => "Internal error. Please report at github.com/supernovae-st/nika/issues",
        _ => "Unknown error code. Check documentation for details.",
    }
}

/// The access-family helps (1800-1849 · D-2026-08-04-N1) — split out
/// of [`code_help`] to keep both under the 100-line cap (the
/// [`run_code_help`] precedent).
fn access_code_help(num: u16) -> &'static str {
    match num {
        1800 => {
            "No access path survives admission for this model — every candidate was rejected with a witness. Read the rejected rows (`nika explain <workflow>`), then configure the key it names, start the local server, or widen policy.allow.providers."
        }
        1801 => {
            "The `--access` pin names a path this task cannot use here. A pin is a pin: the engine refuses rather than substitute another path or model. Drop the pin, or pick one of the paths `nika doctor` lists for this provider."
        }
        1802 => {
            "The `--access` token is neither an access class (local · api · harness · oauth · mock) nor a known agentic CLI (claude-code · codex · gemini-cli · kimi-code · qwen-code). `nika doctor` lists every path."
        }
        1803 => {
            "That agentic CLI is not installed (or this nika was built without adapters). Install Claude Code, Codex, Gemini CLI, Kimi Code or Qwen Code, or pick Nika local / Nika Cloud."
        }
        1804 => {
            "The harness session died mid-run (process exit or wire breakdown) — transient: retry, and if it repeats read the adapter's own logs."
        }
        1805 => {
            "The harness refused the request in its own words (its auth may be absent on ITS side, or the capability unsupported). Sign in to the harness itself — nika never holds its credential."
        }
        1806 => {
            "The harness asked for an action the workflow's `permits:` grants do not cover, so the run PAUSED for a human answer (a gate question is never auto-answered). Resume with `nika run --resume <trace> --answer <task>=true` to grant it once, `=false` to deny — or widen the permits block."
        }
        _ => {
            "Execution access resolution failed. `model:` picks the intelligence; access picks the path — read the plan's witnesses via `nika explain`, then fix the path the witness names."
        }
    }
}

/// The RUN-namespace helps (`NIKA-RUN-*` · 1700-1799) — split out of
/// [`code_help`] to keep both under the 100-line cap.
fn run_code_help(num: u16) -> &'static str {
    match num {
        1700 => "Run `nika check` first — the runtime refuses a workflow whose audit is dirty.",
        1701 => {
            "The wave schedule references a task index outside the workflow. Re-run `nika check`; if it persists, report the checker/runtime mismatch."
        }
        1702 => {
            "A `${{ }}` reference did not resolve. Check the task id / var name spelling — only `tasks.<id>.output` and `vars.<key>` resolve in v0."
        }
        1703 => {
            "This `when:` form is not in the v0 subset. Supported: `${{ <ref> == '<lit>' }}`, `!=`, or a bare `${{ <ref> }}` truthy gate."
        }
        1704 => {
            "The run crossed its `--max-cost-usd` budget: in-flight work completed and was counted, unstarted tasks were cancelled. Raise the budget, trim the workflow, or check the envelope with `nika check` (the budget bounds METERED spend — local/mock work never trips it)."
        }
        1705 => {
            "The exec `decode:` pipeline failed: the captured bytes did not decode (strict-UTF-8 text · unparseable JSON/JSONL). Match the decode to what the command emits — an author who wants raw octets says `decode: bytes`."
        }
        1706 => {
            "The decoded value does not fit the task's `returns:` type (NIKA-TYPE-101). Align the contract with what the command really emits, or fix the command — the type is the truth the run enforces."
        }
        1707 => {
            "The CheckReport does not match the workflow bytes — the run-start boundary re-derivation (permits-fit · trifecta) found something a clean report was credited with not having. Re-run `nika check` on THIS file; a clean report over different bytes is not clean."
        }
        1708 => {
            "A `required: true` input has neither a declared `default:` nor a `--var` override — the run refuses at admission, before any task spends. Supply each named input with `nika run <file> --var <name>=<value>` (or declare a `default:`)."
        }
        1709 => {
            "The workflow's unavoidable cost floor already exceeds the budget it was launched under (cheapest static path · gates closed · first-try) — raise the budget, trim the workflow, or read the envelope with `nika check`. Under composition the budget is inherited (`min(parent remaining, child declared)` · spec 14 law 6): the parent's remaining at call time was already too small for the child's floor."
        }
        1710 => {
            "The sandbox policy requires OS confinement this host cannot provide: a `permits:`-declaring workflow with `exec:` children refuses to run unjailed. Install the backend (Debian/Ubuntu: `apt install bubblewrap`), or waive explicitly with `NIKA_SANDBOX=off` — the waiver is attested on the journal's opening frame."
        }
        1711 => {
            "`NIKA_SANDBOX` held an unknown word — expected exactly `auto`, `require`, or `off`. A typo'd security knob refuses to start rather than loudly default (fail-closed)."
        }
        _ => "Runtime error. Re-run `nika check`, then `nika explain` the exact code.",
    }
}

/// Look up a [`NikaCode`] by its wire string (e.g. `"NIKA-001"`).
///
/// Returns `None` if the string doesn't match any registered code.
#[must_use]
pub fn lookup(wire: &str) -> Option<NikaCode> {
    let num_str = wire.strip_prefix("NIKA-")?;
    let num: u16 = num_str.parse().ok()?;
    ALL.iter().copied().find(|c| c.num == num)
}

/// The namespace teaching for runtime codes with no per-code registry row —
/// per-builtin `NIKA-BUILTIN-<NAME>-<NNN>` and per-provider
/// `NIKA-PROVIDER-<NNN>` (001-099 allocated per owner · spec 05-errors.md).
/// Both are valid in `on_codes:`, so EVERY explain surface (CLI · MCP)
/// teaches what the namespace IS instead of 404-ing — one voice, one text.
/// `docs` is the caller's rendering of the error-docs reference (a themed
/// OSC-8 link on a TTY · plain text over MCP).
#[must_use]
pub fn namespace_help(code: &str, docs: &str) -> Option<String> {
    if let Some((name, num)) = builtin_code_name(code) {
        // The codes an author actually MEETS on a first run deserve the
        // full lesson: the run card says `fix: nika explain <code>` — an
        // explain that answered "see spec stdlib" taught nothing at the
        // exact moment it was obeyed (first-run gate · 2026-07-31).
        if let Some(contract) = builtin_contract_help(&name, &num) {
            return Some(format!(
                "{code} · builtin · the `nika:{name}` contract\n\n{contract}\n  \
                 Valid in `retry.on_codes:` and `on_error.on_codes:` — \
                 see spec stdlib (builtins) · {docs}.\n"
            ));
        }
        return Some(format!(
            "{code} · builtin · runtime error from the `nika:{name}` builtin\n\n  \
             A per-builtin runtime diagnostic (each builtin owns \
             NIKA-BUILTIN-<NAME>-001..099). Valid in `retry.on_codes:` and \
             `on_error.on_codes:`. The specific cause is the builtin's own \
             arg/runtime contract — see spec stdlib (builtins) · \
             {docs}.\n"
        ));
    }
    is_provider_code(code).then(|| {
        format!(
            "{code} · provider · a provider-adapter runtime error\n\n  \
             A per-provider diagnostic (each provider adapter owns \
             NIKA-PROVIDER-001..099). The specific cause is provider-defined \
             (transport · quota · auth · response shape from that provider). \
             Valid in `retry.on_codes:` and `on_error.on_codes:` — see \
             spec/05-errors.md §NIKA-PROVIDER · {docs}.\n"
        )
    })
}

/// Recognize `NIKA-BUILTIN-<NAME>-<NNN>` and return the builtin's lowercase
/// name + the number (`NIKA-BUILTIN-FETCH-001` → (`fetch`, `001`)).
fn builtin_code_name(code: &str) -> Option<(String, String)> {
    let rest = code.strip_prefix("NIKA-BUILTIN-")?;
    let (name, num) = rest.rsplit_once('-')?;
    (num.len() == 3 && num.bytes().all(|b| b.is_ascii_digit()) && !name.is_empty())
        .then(|| (name.to_ascii_lowercase(), num.to_owned()))
}

/// The per-code contract lessons (kept beside the namespace voice so
/// CLI and MCP explain teach identically). Only the codes whose cause
/// and exits are STABLE spec surface earn an entry — everything else
/// keeps the namespace teaching.
fn builtin_contract_help(name: &str, num: &str) -> Option<&'static str> {
    match (name, num) {
        ("read", "001") => Some(
            "  `nika:read` found no file at the path it was given. Paths resolve \
             from the RUN's working directory (never the workflow file's own \
             dir) — the usual causes are a missing ingredient or a run \
             launched from the wrong directory.\n\n  \
             exits (pick one):\n    \
             · see what IS there:          ls <the path's parent>\n    \
             · run beside the file:        cd <the workflow's dir> && nika run <file>\n    \
             · an example's ingredients:   `nika new <slug>` lands its \
             `examples/fixtures/` files beside the recipe.\n",
        ),
        ("prompt", "001") => Some(
            "  The gate asked a human (`confirm` · `input` · `choice`) where no \
             human can answer and no `default:` is declared. The engine never \
             hangs and never invents an answer (stdlib §prompt) — the reference \
             CLI parks the run instead: a durable pause (exit 4), the exact \
             resume line taught on the frame.\n\n  \
             exits (pick one):\n    \
             · answer it at launch:       nika run <file> --answer <task>=<value>\n    \
             · resume the paused trace:   nika run <file> --resume <trace> --answer <task>=<value>\n    \
             · declare the unattended answer:  args: { …, default: <value> }\n    \
             · at a terminal, just run it — the gate asks you directly.\n",
        ),
        ("write", "001") => Some(
            "  `nika:write` could not land the file. The causes, each with its \
             own voice: a missing parent directory while `create_dirs: false` \
             (the default — a typo'd path surfaces rather than silently \
             materializing a tree; one carve-out: a declared `permits.fs.write` \
             grant covering the path creates the tree itself, the permit IS \
             the intent) · a `null` `content:` (almost always a missing \
             upstream value — an unset var, a skipped task — refused loudly \
             because silently writing the bytes `null` is data corruption) · \
             a plain IO failure (disk · permission), named verbatim.\n\n  \
             exits (pick one):\n    \
             · create the parent:          args: { …, create_dirs: true }\n    \
             · a null content: check the producing task in the trace — \
             `nika trace peek <trace> <task>` shows what it actually \
             handed over (to write the literal word, quote it: \"null\")\n    \
             · the path exists + `overwrite: false` is its own code — \
             `nika explain NIKA-BUILTIN-WRITE-002`.\n",
        ),
        _ => None,
    }
}

/// The per-code contract lessons for the SPEC conformance codes — the
/// high-traffic refusals whose registry row states WHAT while the user
/// at the terminal needs the GRAMMAR (gauntlet 2026-07-31, twice: the
/// finding named the exact boundary, `explain` answered a category and
/// a URL). One voice: the CLI's canon row and the MCP explain tool both
/// append this text. Only codes whose teaching is STABLE spec surface
/// earn an entry.
#[must_use]
pub fn spec_contract_help(code: &str) -> Option<&'static str> {
    match code {
        "NIKA-SEC-004" => Some(
            "  The boundary is default-deny once `permits:` is present: an \
             effect the block does not cover is refused, and the FINDING \
             names the exact path, host, program or tool that fell outside \
             — grant that named thing, in its category, never more.\n\n  \
             the grant grammar (measured · CONVENTIONS §2):\n    \
             · a directory grant does NOT cover its children — `data` \
             covers `data` itself, not `data/a.csv`\n    \
             · `data/*` covers ONE level (`*` never crosses `/`) · \
             `data/**` covers the subtree\n    \
             · `net.http` entries are exact host names, never globs · \
             `exec:` lists program names\n\n  \
             exits (pick one):\n    \
             · grant the named path:       fs: { read: [\"./dir/*\"] } — \
             the file the finding printed, or its one-level glob\n    \
             · grant the named host:       net: { http: [\"api.example.com\"] }\n    \
             · grant the named tool:       add it to `permits.tools`\n    \
             · the effect was NOT intended — then the boundary just did \
             its job; the refusal is the feature, not the failure.\n\n  \
             Never widen to a root `**` to silence the message — the \
             tightest grant that covers the body is the whole point.\n\n  \
             And a green `check` was never this refusal's promise: check \
             judges the LITERAL shape — a computed path (a glob result · \
             an interpolated binding) is judged HERE, at run. Two tools \
             over one tree may need two grants (the walker wants the \
             directory · the reader wants the files: `[\"./dir\", \
             \"./dir/*\"]`).\n",
        ),
        _ => None,
    }
}

/// Recognize `NIKA-PROVIDER-<NNN>` (exactly three digits).
fn is_provider_code(code: &str) -> bool {
    code.strip_prefix("NIKA-PROVIDER-")
        .is_some_and(|num| num.len() == 3 && num.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one-voice namespace teaching both explain surfaces consume:
    /// builtin and provider codes teach, everything else stays `None`
    /// (the caller keeps its own unknown-code finding).
    #[test]
    fn namespace_help_teaches_builtin_and_provider_codes() {
        let b = namespace_help("NIKA-BUILTIN-FETCH-001", "docs").expect("builtin namespace");
        assert!(b.contains("`nika:fetch` builtin") && b.contains("on_codes"));
        let p = namespace_help("NIKA-PROVIDER-042", "docs").expect("provider namespace");
        assert!(p.contains("provider-adapter") && p.contains("docs"));
        assert!(namespace_help("NIKA-VAR-001", "docs").is_none());
        assert!(namespace_help("NIKA-BUILTIN-FETCH-1", "docs").is_none());
        assert!(namespace_help("NIKA-PROVIDER-1042", "docs").is_none());
    }

    /// The run card says `fix: nika explain NIKA-BUILTIN-PROMPT-001` —
    /// obeying it must teach the CONTRACT and every working exit, not
    /// the namespace boilerplate (the dead-end loop a real first run
    /// followed · 2026-07-31). One voice: CLI and MCP read this text.
    #[test]
    fn prompt_001_explain_teaches_the_contract_and_its_exits() {
        let read = namespace_help("NIKA-BUILTIN-READ-001", "docs").expect("teaches");
        assert!(
            read.contains("RUN's working directory") && read.contains("nika new"),
            "READ-001 carries the contract lesson (gauntlet 2026-07-31): {read}"
        );
        let help = namespace_help("NIKA-BUILTIN-PROMPT-001", "docs").expect("teaches");
        for lesson in [
            "no `default:` is declared",
            "durable pause (exit 4)",
            "--answer <task>=<value>",
            "--resume <trace>",
            "default: <value>",
            "the gate asks you directly",
        ] {
            assert!(help.contains(lesson), "missing `{lesson}` in:\n{help}");
        }
        // The config-error sibling keeps the namespace teaching.
        let sibling = namespace_help("NIKA-BUILTIN-PROMPT-002", "docs").expect("namespace");
        assert!(
            sibling.contains("per-builtin runtime diagnostic"),
            "{sibling}"
        );
    }

    /// C-9 (gauntlet 2026-07-31 ×2 · Marta + Lucie): WRITE-001 stops
    /// being namespace boilerplate — the lesson names every measured
    /// cause (missing parent + the #433 covering-grant carve-out · null
    /// content · IO) and hands working exits, sibling WRITE-002 pointed.
    #[test]
    fn write_001_explain_teaches_the_contract_and_its_exits() {
        let help = namespace_help("NIKA-BUILTIN-WRITE-001", "docs").expect("teaches");
        for lesson in [
            "create_dirs: false",
            "the permit IS the intent",
            "`null` `content:`",
            "trace peek",
            "NIKA-BUILTIN-WRITE-002",
        ] {
            assert!(help.contains(lesson), "missing `{lesson}` in:\n{help}");
        }
    }

    /// The 452 lesson (issue 814 · 2026-08-03): the way a user meets
    /// this code today is the golden lane — under `nika test` the mock
    /// plane refuses effects BY DESIGN, and the old text named only the
    /// MCP/builtin availability, so the reader was sent hunting a
    /// server that was never the problem. The text now names the test
    /// plane FIRST, enumerates the refused set, and hands both working
    /// exits (`nika run` for the effect · `on_error: recover` to
    /// rehearse) before falling back to the generic causes.
    #[test]
    fn invoke_452_explain_names_the_test_plane_and_its_exits() {
        let help = code_help(NIKA_452);
        for lesson in [
            "nika test",
            "simulates the model, not effects",
            "nika:write",
            "nika run",
            "on_error: recover",
            // The generic causes survive — they are still true elsewhere.
            "MCP server or builtin availability",
        ] {
            assert!(help.contains(lesson), "missing `{lesson}` in:\n{help}");
        }
    }

    /// C-9 · the SEC-004 contract lesson (the refusal every beginner
    /// meets first): the grant GRAMMAR is taught in-terminal — the
    /// wildcard facts, one exit per category, the honest not-intended
    /// arm — and the widening anti-pattern is named. Both explain
    /// surfaces (CLI canon row · MCP) append this same text.
    #[test]
    fn sec_004_contract_lesson_teaches_the_grant_grammar() {
        let help = spec_contract_help("NIKA-SEC-004").expect("teaches");
        for lesson in [
            "default-deny",
            "does NOT cover its children",
            "never crosses `/`",
            "exact host names",
            "permits.tools",
            "the refusal is the feature",
            "Never widen to a root `**`",
            // V7-2 (wave-3 · 4 personas): the check⊥run split taught at
            // the moment of refusal — a green check never promised the
            // computed paths, and two tools over one tree may need two
            // grants (the Marta read/glob whack-a-mole).
            "judged HERE, at run",
            "two grants",
        ] {
            assert!(help.contains(lesson), "missing `{lesson}` in:\n{help}");
        }
        // Only the earned codes teach — the rest keep the registry row.
        assert!(spec_contract_help("NIKA-SEC-001").is_none());
        assert!(spec_contract_help("NIKA-DAG-001").is_none());
    }

    #[test]
    fn category_and_severity_as_str_match_the_serde_wire_form() {
        // The two representations (the const `as_str` label + the serde
        // `rename_all = "kebab-case"` wire form) MUST agree — pinning it
        // here is the belt that lets both exist without drifting. Every
        // ALL-listed code's category round-trips through serde to the same
        // string `as_str` returns.
        for code in ALL {
            let wire = serde_json::to_string(&code.category).expect("serializes");
            assert_eq!(wire.trim_matches('"'), code.category.as_str(), "{code}");
            let sev_wire = serde_json::to_string(&code.severity).expect("serializes");
            assert_eq!(sev_wire.trim_matches('"'), code.severity.as_str(), "{code}");
        }
        // A spot-check that the labels are the expected kebab forms (the
        // Observability variant is the one explain's old hand-rolled copy
        // SILENTLY dropped to a fallback — `as_str` can't, it's exhaustive).
        assert_eq!(Category::Verb.as_str(), "verb");
        assert_eq!(Category::FileIo.as_str(), "file-io");
        assert_eq!(Category::WasmPlugin.as_str(), "wasm-plugin");
        assert_eq!(Category::Observability.as_str(), "observability");
        assert_eq!(Severity::Error.as_str(), "error");
    }

    #[test]
    fn verb_codes_have_their_category_and_range() {
        // Verb 430-479 · s9 infer 430-439 · s10 exec 440-449 · s11 invoke 450-459.
        for c in [
            NIKA_430, NIKA_431, NIKA_432, NIKA_433, NIKA_440, NIKA_441, NIKA_442, NIKA_450,
            NIKA_451, NIKA_452,
        ] {
            assert_eq!(c.category, Category::Verb, "{c}");
            assert!((430..=459).contains(&c.num), "{c}");
            assert_eq!(lookup(&c.to_string()), Some(c), "{c} resolvable");
            assert!(!code_help(c).is_empty(), "{c} has help");
        }
    }

    #[test]
    fn computer_use_codes_have_their_categories() {
        // M2 computer-use L1 ranges (ADR-081) · Screen 1000-1099 ·
        // Ocr 1100-1199 · A11y 1200-1299.
        for c in [
            NIKA_1000, NIKA_1001, NIKA_1002, NIKA_1003, NIKA_1004, NIKA_1005, NIKA_1006, NIKA_1007,
            NIKA_1008, NIKA_1009,
        ] {
            assert_eq!(c.category, Category::Screen, "{c}");
            assert!((1000..=1099).contains(&c.num), "{c}");
        }
        for c in [
            NIKA_1101, NIKA_1102, NIKA_1103, NIKA_1104, NIKA_1105, NIKA_1106, NIKA_1107, NIKA_1108,
            NIKA_1109,
        ] {
            assert_eq!(c.category, Category::Ocr, "{c}");
            assert!((1100..=1199).contains(&c.num), "{c}");
        }
        for c in [
            NIKA_1201, NIKA_1202, NIKA_1203, NIKA_1204, NIKA_1205, NIKA_1206,
        ] {
            assert_eq!(c.category, Category::A11y, "{c}");
            assert!((1200..=1299).contains(&c.num), "{c}");
        }
    }

    #[test]
    fn computer_use_codes_lookup_and_help() {
        let c = lookup("NIKA-1003").expect("screen capture-failed registered");
        assert_eq!(c.slug, "screen-capture-failed");
        assert!(!code_help(c).is_empty());
        assert!(lookup("NIKA-1101").is_some(), "ocr registered");
        assert!(lookup("NIKA-1206").is_some(), "a11y registered");
        assert!(
            lookup("NIKA-1200").is_none(),
            "1200 reserved (closed skeleton slot)"
        );
    }

    #[test]
    fn display_format_three_digit_padding() {
        assert_eq!(format!("{NIKA_001}"), "NIKA-001");
        assert_eq!(format!("{NIKA_999}"), "NIKA-999");
    }

    #[test]
    fn display_format_no_extra_padding_above_999() {
        let big = NikaCode {
            num: 1234,
            category: Category::Core,
            severity: Severity::Error,
            slug: "test",
        };
        assert_eq!(format!("{big}"), "NIKA-1234");
    }

    #[test]
    fn nika_001_is_core_validation() {
        assert_eq!(NIKA_001.category, Category::Core);
        assert_eq!(NIKA_001.severity, Severity::Error);
        assert_eq!(NIKA_001.slug, "validation-failed");
    }

    #[test]
    fn all_codes_unique_nums() {
        for (i, a) in ALL.iter().enumerate() {
            for b in ALL.iter().skip(i + 1) {
                assert_ne!(a.num, b.num, "duplicate num: {a} and {b}");
            }
        }
    }

    #[test]
    fn every_declared_code_is_registered_in_all() {
        // THE RATCHET (declared == registered) — `ALL` is hand-listed,
        // so the day someone declares `NIKA_468` and forgets the array,
        // `lookup()` 404s and `explain` goes silent for a live code.
        // Source-as-data: count the declaration lines in THIS file and
        // pin them to `ALL.len()`. Phantoms in `ALL` already fail to
        // compile (the array references the consts) — the forgotten
        // direction is the one only this test catches. Sister ratchet:
        // nika-schema's emitted⊆registered test for the SPEC codes.
        // The registry rows live in codes/registry.rs since the
        // 2026-07-10 split; the machinery file must stay row-free (the
        // second assert), so a stray const added HERE is caught too.
        let declared = include_str!("codes/registry.rs")
            .lines()
            .filter(|l| l.trim_start().starts_with("pub const NIKA_"))
            .count();
        let strays = include_str!("codes.rs")
            .lines()
            .filter(|l| l.trim_start().starts_with("pub const NIKA_"))
            .count();
        assert_eq!(
            strays, 0,
            "a NIKA_* const landed in codes.rs — registry.rs is the one home for rows"
        );
        assert_eq!(
            declared,
            ALL.len(),
            "a `pub const NIKA_*` is declared but not listed in `ALL` — register it"
        );
    }

    #[test]
    fn all_codes_unique_slugs() {
        for (i, a) in ALL.iter().enumerate() {
            for b in ALL.iter().skip(i + 1) {
                assert_ne!(a.slug, b.slug, "duplicate slug: {} and {}", a.slug, b.slug);
            }
        }
    }

    #[test]
    fn code_help_returns_non_empty_for_registered() {
        for code in ALL {
            let help = code_help(*code);
            assert!(!help.is_empty(), "empty help for {code}");
            assert!(
                !help.contains("Unknown"),
                "registered code {code} got fallback help — add specific entry to code_help()"
            );
        }
    }

    #[test]
    fn code_help_specific_values() {
        assert!(code_help(NIKA_001).contains("YAML"));
        assert!(code_help(NIKA_002).contains("not found"));
        assert!(code_help(NIKA_003).contains("not supported"));
        assert!(code_help(NIKA_999).contains("Internal"));
    }

    #[test]
    fn code_help_unknown_returns_fallback() {
        let unknown = NikaCode {
            num: 8888,
            category: Category::Core,
            severity: Severity::Error,
            slug: "unknown",
        };
        assert!(code_help(unknown).contains("Unknown"));
    }

    #[test]
    fn code_help_covers_cross_crate_ranges() {
        // Schema (280-329), Provider (330-379) and the reserved Shield range
        // (380-429) carry help via NUMERIC-RANGE arms — `code_help` switches on
        // `code.num` only, so the `category` field below is immaterial (Shield
        // is not even a Category variant yet, its crate is unadmitted). These
        // codes live in sibling crates so they are NOT in `ALL`; the range arms
        // are the only coverage. Regression: Provider moved 380-429 → 330-379,
        // which left Schema + Provider falling through to "Unknown error code".
        let probe = |num: u16| NikaCode {
            num,
            category: Category::Core,
            severity: Severity::Error,
            slug: "probe",
        };
        let schema = code_help(probe(299));
        assert!(
            schema.contains("Schema") && !schema.contains("Unknown"),
            "{schema}"
        );
        let provider = code_help(probe(330));
        assert!(
            provider.contains("provider") && !provider.contains("Unknown"),
            "{provider}"
        );
        let shield = code_help(probe(380));
        assert!(
            shield.contains("Shield") && !shield.contains("Unknown"),
            "{shield}"
        );
    }

    #[test]
    fn lookup_valid_codes() {
        assert_eq!(lookup("NIKA-001"), Some(NIKA_001));
        assert_eq!(lookup("NIKA-999"), Some(NIKA_999));
    }

    #[test]
    fn lookup_invalid_returns_none() {
        assert_eq!(lookup("NIKA-555"), None);
        assert_eq!(lookup("INVALID"), None);
        assert_eq!(lookup(""), None);
    }

    #[test]
    fn lookup_roundtrip_via_display() {
        for code in ALL {
            let wire = format!("{code}");
            let found = lookup(&wire);
            assert_eq!(found, Some(*code), "roundtrip failed for {code}");
        }
    }

    #[cfg(feature = "serde")]
    mod serde_tests {
        use super::*;

        #[test]
        fn nika_code_serializes_as_wire_string() {
            let json = serde_json::to_string(&NIKA_001).expect("serialize");
            assert_eq!(json, "\"NIKA-001\"");
        }

        #[test]
        fn nika_code_deserializes_from_wire_string() {
            let code: NikaCode = serde_json::from_str("\"NIKA-001\"").expect("deserialize");
            assert_eq!(code, NIKA_001);
        }

        #[test]
        fn nika_code_serde_roundtrip() {
            for code in ALL {
                let json = serde_json::to_string(code).expect("serialize");
                let back: NikaCode = serde_json::from_str(&json).expect("deserialize");
                assert_eq!(back, *code);
            }
        }

        #[test]
        fn category_serializes_kebab_case() {
            let json = serde_json::to_string(&Category::FileIo).expect("serialize");
            assert_eq!(json, "\"file-io\"");
        }

        #[test]
        fn unknown_code_deser_fails() {
            let result: Result<NikaCode, _> = serde_json::from_str("\"NIKA-555\"");
            assert!(result.is_err());
        }
    }

    mod proptest_codes {
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn display_always_starts_with_nika(num in 1u16..=9999u16) {
                let code = super::NikaCode {
                    num,
                    category: super::Category::Core,
                    severity: super::Severity::Error,
                    slug: "test",
                };
                let display = format!("{code}");
                prop_assert!(display.starts_with("NIKA-"));
                prop_assert!(display.len() >= 8); // "NIKA-" + at least 3 digits
            }
        }

        #[test]
        fn all_registered_codes_have_unique_nums() {
            let nums: Vec<u16> = super::ALL.iter().map(|c| c.num).collect();
            let mut sorted = nums.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(nums.len(), sorted.len(), "duplicate nums in ALL registry");
        }

        // ─── Registry uniqueness + memory cross-mapping (Diamond W2.3) ───

        #[test]
        fn all_registered_codes_have_unique_slugs() {
            let slugs: Vec<&'static str> = super::ALL.iter().map(|c| c.slug).collect();
            let mut sorted = slugs.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(slugs.len(), sorted.len(), "duplicate slugs in ALL registry");
        }

        proptest! {
            // Every registered slug MUST be lowercase kebab-case (^[a-z][a-z0-9-]*$).
            #[test]
            fn all_slugs_are_kebab_case(idx in 0usize..super::ALL.len()) {
                let slug = super::ALL[idx].slug;
                prop_assert!(!slug.is_empty(), "slug must not be empty");
                let first = slug.chars().next().expect("non-empty checked above");
                prop_assert!(
                    first.is_ascii_lowercase(),
                    "slug must start with a-z, got {first:?}"
                );
                for c in slug.chars() {
                    prop_assert!(
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-',
                        "slug contains invalid char {c:?}"
                    );
                }
            }
        }

        #[test]
        fn code_help_some_for_every_registered_code() {
            // Every code in ALL must return a non-empty, non-Unknown help string.
            for code in super::ALL {
                let help = super::code_help(*code);
                assert!(!help.is_empty(), "empty help for {code}");
                assert!(
                    !help.contains("Unknown"),
                    "registered code {code} got fallback Unknown help"
                );
            }
        }

        #[test]
        fn memory_codes_601_to_605_are_memory_category() {
            for code in [
                super::NIKA_601,
                super::NIKA_602,
                super::NIKA_603,
                super::NIKA_604,
                super::NIKA_605,
            ] {
                assert_eq!(
                    code.category,
                    super::Category::Memory,
                    "{code} must be Category::Memory"
                );
                assert!(
                    code.num >= 601 && code.num <= 605,
                    "{code} num must be 601..=605 sub-allocation"
                );
            }
        }

        #[test]
        fn memory_codes_lookup_roundtrip() {
            for code in [
                super::NIKA_601,
                super::NIKA_602,
                super::NIKA_603,
                super::NIKA_604,
                super::NIKA_605,
            ] {
                let wire = format!("{code}");
                let back = super::lookup(&wire);
                assert_eq!(back, Some(code), "memory code roundtrip failed for {code}");
            }
        }

        #[test]
        fn access_codes_ride_the_1800_block_and_roundtrip() {
            // D-2026-08-04-N1 · the access block is 1800..=1849 and every
            // registered row is Category::Access with a distinct fix line.
            for code in [
                super::NIKA_1800,
                super::NIKA_1801,
                super::NIKA_1802,
                super::NIKA_1803,
                super::NIKA_1804,
                super::NIKA_1805,
                super::NIKA_1806,
            ] {
                assert_eq!(
                    code.category,
                    super::Category::Access,
                    "{code} must be Category::Access"
                );
                assert!(
                    code.num >= 1800 && code.num <= 1849,
                    "{code} num must be inside the 1800..=1849 block"
                );
                let wire = format!("{code}");
                let back = super::lookup(&wire);
                assert_eq!(back, Some(code), "access code roundtrip failed for {code}");
            }
            // The fix lines teach DIFFERENT gestures — a shared string
            // would mean a copy-paste arm (mutation bait).
            let helps = [
                super::code_help(super::NIKA_1800),
                super::code_help(super::NIKA_1801),
                super::code_help(super::NIKA_1802),
                super::code_help(super::NIKA_1806),
            ];
            for (i, a) in helps.iter().enumerate() {
                for b in &helps[i + 1..] {
                    assert_ne!(a, b, "access help arms must stay distinct");
                }
            }
        }
    }
}
