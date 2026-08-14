// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The closed key vocabularies of the workflow language, as ONE read
//! surface.
//!
//! Descended from `nika-schema`'s parser + its `keysets` door at the C2
//! flag-day (the 15k prod-LOC wall — key vocabularies are vocabulary, so
//! the vocabulary crate is their home). Every list here is the EXACT const
//! the parser validates against (and builds its did-you-mean suggestions
//! from) — a consumer that offers keys (the LSP · a form UI) reads THIS
//! door and can never drift from what the parser accepts (the born-stale
//! law, key edition).

/// Keys of the typed `inputs:` form (spec 01 §inputs · `type` required).
pub const INPUT_KEYS: &[&str] = &["type", "required", "default", "description"];

/// Keys of the typed `const:` form (spec 01 §const · `{type, value}` exactly).
pub const CONST_TYPED_KEYS: &[&str] = &["type", "value"];

/// Keys of a `secrets:` entry (spec 01 §secrets · `key` XOR `path` ·
/// discriminated by `source` · plus the optional `egress:` declass list).
pub const SECRET_KEYS: &[&str] = &["source", "key", "path", "egress"];

/// Keys of one `secrets.<name>.egress[]` entry (spec 01 §secrets).
pub const EGRESS_KEYS: &[&str] = &["to", "host", "host_from_self"];

/// Keys of the typed `outputs:` form (spec 01 §outputs).
pub const TYPED_OUTPUT_KEYS: &[&str] = &["value", "type", "description"];

/// Keys of the `permits:` block (spec 01 §permits · closed).
pub const PERMITS_KEYS: &[&str] = &["fs", "net", "exec", "tools", "env"];

/// Keys of `permits.fs` (closed).
pub const PERMITS_FS_KEYS: &[&str] = &["read", "write"];

/// Keys of `permits.net` (closed).
pub const PERMITS_NET_KEYS: &[&str] = &["http"];

/// Keys of a `retry:` block (spec 05 §retry).
pub const RETRY_KEYS: &[&str] = &[
    "max_attempts",
    "backoff_ms",
    "backoff_strategy",
    "backoff_max_ms",
    "jitter",
    "on_codes",
];

/// Keys of the `run:` block (F-P3 · closed — a typo'd declaration would
/// silently not bind, so the parser refuses unknown keys in BOTH modes,
/// the `permits:`/`policy:` precedent).
pub const RUN_KEYS: &[&str] = &["entropy", "clock"];

/// Keys of the parameterized `run.entropy` form (`{ seeded: <u64> }` ·
/// the single-key map idiom of the `assert:` vocabulary).
pub const RUN_ENTROPY_MAP_KEYS: &[&str] = &["seeded"];

/// Keys of an `on_error:` block (spec 05 §`on_error` · exactly one
/// ACTION + the optional `on_codes` filter).
pub const ON_ERROR_KEYS: &[&str] = &["recover", "skip", "on_codes"];

/// The canonical TASK-level keys (spec `03-dag.md` §the task shape ·
/// the 4 verb selectors live in the parser's `VERB_KEYS`). One list,
/// two consumers: the parser REFUSES anything else, and the LSP OFFERS
/// exactly this — pinned coherent by test so a teaching surface can
/// never drift ahead of, or behind, what the engine admits.
pub const TASK_KEYS: &[&str] = &[
    "after", "when", "for_each", "retry", "on_error", "timeout", "with", "extract", "returns",
    "lift", "group",
];

/// `infer:` fields (spec `02-verbs.md` §infer field table).
pub const INFER_KEYS: &[&str] = &[
    "prompt",
    "system",
    "model",
    "temperature",
    "max_tokens",
    "schema",
    "thinking",
    "vision",
];

/// `exec:` fields (spec `02-verbs.md` §exec field table).
pub const EXEC_KEYS: &[&str] = &[
    "command", "shell", "cwd", "env", "stdin", "capture", "decode",
];

/// `invoke:` fields (spec `02-verbs.md` §invoke field table + spec
/// `14-composition.md` §the form — `tool:` XOR `workflow:`).
pub const INVOKE_KEYS: &[&str] = &["tool", "workflow", "args"];

/// `agent:` fields (spec `02-verbs.md` §agent field table).
pub const AGENT_KEYS: &[&str] = &[
    "prompt",
    "system",
    "model",
    "tools",
    "skills",
    "max_turns",
    "max_tokens_total",
    "temperature",
    "schema",
];

/// `thinking:` sub-fields (spec `02-verbs.md` · `{ enabled, budget_tokens }`).
pub const THINKING_KEYS: &[&str] = &["enabled", "budget_tokens"];

/// The child keys legal inside the block introduced by `block_key` —
/// `None` when the key introduces no closed keyset this door knows
/// (free-form maps like `args:` / `with:` / `env:`, or an unknown
/// key). `parent` is the block's OWN enclosing key when known —
/// `fs:`/`net:` are permits vocabulary ONLY under `permits:` (an
/// `fs:` argument of some tool stays free-form).
#[must_use]
pub fn known_child_keys(block_key: &str, parent: Option<&str>) -> Option<&'static [&'static str]> {
    match block_key {
        "retry" => Some(RETRY_KEYS),
        "on_error" => Some(ON_ERROR_KEYS),
        "infer" => Some(INFER_KEYS),
        "exec" => Some(EXEC_KEYS),
        "invoke" => Some(INVOKE_KEYS),
        "agent" => Some(AGENT_KEYS),
        "thinking" => Some(THINKING_KEYS),
        "permits" => Some(PERMITS_KEYS),
        "fs" if parent == Some("permits") => Some(PERMITS_FS_KEYS),
        "net" if parent == Some("permits") => Some(PERMITS_NET_KEYS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The door serves the parser's OWN lists — spot-pin the members
    /// that define each register (full parity is the consts themselves:
    /// this door returns them by reference, no copy exists to drift).
    #[test]
    fn door_returns_the_parser_consts() {
        let retry = known_child_keys("retry", None).expect("retry keyset");
        assert!(retry.contains(&"max_attempts") && retry.contains(&"backoff_strategy"));
        let infer = known_child_keys("infer", None).expect("infer keyset");
        assert!(infer.contains(&"prompt") && infer.contains(&"thinking"));
        assert!(
            known_child_keys("args", None).is_none(),
            "free-form maps stay free"
        );
    }
}
