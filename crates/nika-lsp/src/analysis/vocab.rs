// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static language vocabulary — ONE table behind completion + hover.
//!
//! Everything here is the LOCKED, statically-knowable surface of the v1
//! language: the 4 verbs (`infer · exec · invoke · agent`, LOCKED forever
//! per D-2026-05-22-N18), the canonical top-level envelope keys, the
//! closed task-field set, and the LLM provider names. Completion and hover
//! both read these tables — one source, no drift between « what we suggest »
//! and « what we document ».
//!
//! The verb set + the envelope/task keys mirror the strict parser's own
//! tables (`nika_schema::parser` `TOP_LEVEL_KEYS` + the closed task shape
//! of spec `03-dag.md`). The provider list is mirrored-from-canon (the
//! canonical 16-provider LLM catalog · spec `stdlib/providers-v0.1.md` +
//! `spec/canon.yaml` SSOT); it is presented local/open-weight-first per
//! the studio's vendor-agnostic ordering. A drift between this mirror and
//! the catalog is caught the day `nika-lsp` grows a `nika-catalog` dep
//! (deferred — v0.1 has no external-catalog dependency).

/// One vocabulary entry — a keyword/name with its one-line documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The literal token (`infer`, `tasks`, `depends_on`, `anthropic`).
    pub name: &'static str,
    /// A one-line human description (the hover body + completion detail).
    pub doc: &'static str,
}

/// The 4 verbs — a distinct native execution model each, LOCKED forever
/// (D-2026-05-22-N18 · `fetch` is the `nika:fetch` builtin under `invoke`,
/// not a verb).
pub const VERBS: &[Entry] = &[
    Entry {
        name: "infer",
        doc: "A single LLM call. Sends one prompt, returns one response \
              (optionally structured against a `schema:`).",
    },
    Entry {
        name: "exec",
        doc: "Run a shell command or argv program. Captures stdout/stderr \
              under the declared `permits:` boundary.",
    },
    Entry {
        name: "invoke",
        doc: "Call one builtin (`nika:<name>`) or MCP tool (`mcp:<server>/<tool>`) \
              with `args:`. The single-tool-call verb.",
    },
    Entry {
        name: "agent",
        doc: "A multi-turn agentic loop: the model calls whitelisted tools \
              over several turns until done (default-deny tools, capped turns).",
    },
];

/// The canonical top-level envelope keys (spec `01-envelope.md` · mirrors
/// the strict parser's `TOP_LEVEL_KEYS`).
pub const TOP_LEVEL_KEYS: &[Entry] = &[
    Entry {
        name: "nika",
        doc: "The language + contract version. Exactly `v1`. Required.",
    },
    Entry {
        name: "workflow",
        doc: "The workflow id (kebab-case). Required.",
    },
    Entry {
        name: "description",
        doc: "Free-form human description of the workflow.",
    },
    Entry {
        name: "model",
        doc: "The workflow-level default model, as `<provider>/<name>`.",
    },
    Entry {
        name: "vars",
        doc: "Workflow inputs — untyped values or typed input declarations \
              (`${{ vars.X }}`).",
    },
    Entry {
        name: "env",
        doc: "Non-sensitive runtime config (`${{ env.X }}`).",
    },
    Entry {
        name: "secrets",
        doc: "Masked, vault-backed references (`${{ secrets.X }}`). \
              Never printed; tracked by the IFC leak analysis.",
    },
    Entry {
        name: "permits",
        doc: "The declared capability boundary. Once present, every \
              category is default-deny (exec, tools, fs, net).",
    },
    Entry {
        name: "tasks",
        doc: "The task DAG. Required, non-empty. Each task runs exactly \
              one verb.",
    },
    Entry {
        name: "outputs",
        doc: "The workflow's return contract — named values the run yields.",
    },
];

/// The closed task-field set (spec `03-dag.md` §forward-compat · the v1
/// task shape · strict mode rejects anything else). Excludes the 4 verb
/// selectors, which live in [`VERBS`].
pub const TASK_FIELD_KEYS: &[Entry] = &[
    Entry {
        name: "after",
        doc: "The CONTROL boundary — `{producer: predicate}` map; each \
              entry is one control edge (succeeded · failed · skipped · \
              terminal).",
    },
    Entry {
        name: "when",
        doc: "A LOCAL boolean CEL condition (`${{ ... }}`) or `true` — \
              evaluated POST-gate over vars · env · with · item · index; \
              false settles skipped.",
    },
    Entry {
        name: "for_each",
        doc: "Map the task over a literal list or an upstream array output \
              (bounded fan-out).",
    },
    Entry {
        name: "max_parallel",
        doc: "Cap concurrent `for_each` iterations (≥ 1).",
    },
    Entry {
        name: "fail_fast",
        doc: "Abort the `for_each` on the first error (default true).",
    },
    Entry {
        name: "retry",
        doc: "Transient-error retry policy (attempts + backoff).",
    },
    Entry {
        name: "on_error",
        doc: "Terminal-error recovery (continue, fail, or a fallback).",
    },
    Entry {
        name: "timeout",
        doc: "A Go-duration hard kill for the task (`30s`, `5m`).",
    },
    Entry {
        name: "with",
        doc: "Task-scope variable injection (`${{ with.X }}`).",
    },
    Entry {
        name: "output",
        doc: "Named jq bindings over the verb's raw response.",
    },
    Entry {
        name: "on_finally",
        doc: "Cleanup mini-tasks that ALWAYS run after this task.",
    },
];

/// The JSON-Schema keyset inside a `schema:` block (02-verbs: the typed
/// response/final-message contract). Direct children of `schema:` and
/// `items:` — `properties:` children are the AUTHOR's field names.
pub const SCHEMA_KEYS: &[Entry] = &[
    Entry {
        name: "type",
        doc: "The JSON type — the portable top level is `object`.",
    },
    Entry {
        name: "required",
        doc: "The field names the response MUST carry (`[summary]`).",
    },
    Entry {
        name: "properties",
        doc: "Field name → its schema (`summary: { type: string }`).",
    },
    Entry {
        name: "items",
        doc: "The element schema of an array.",
    },
    Entry {
        name: "enum",
        doc: "The closed value set — the model cannot invent a fifth answer.",
    },
    Entry {
        name: "description",
        doc: "Teaches the model what the field means — part of the prompt.",
    },
];

/// The canonical LLM provider names for `model: <provider>/<name>`.
///
/// MIRRORED-FROM-CANON: the 16-provider catalog (spec `canon.yaml` SSOT ·
/// `stdlib/providers-v0.1.md`). Ordered local/open-weight-first per the
/// studio's vendor-agnostic presentation convention (the TEACHING surface
/// leads with sovereign options). Keep in sync with the catalog when the
/// `nika-catalog` dep lands.
pub const PROVIDERS: &[Entry] = &[
    Entry {
        name: "ollama",
        doc: "Local models via Ollama (sovereign, open-weight).",
    },
    Entry {
        name: "lmstudio",
        doc: "Local models via LM Studio (sovereign, open-weight).",
    },
    Entry {
        name: "llamacpp",
        doc: "Local models via llama.cpp (sovereign, open-weight).",
    },
    Entry {
        name: "localai",
        doc: "Local models via LocalAI (sovereign, OpenAI-compatible).",
    },
    Entry {
        name: "vllm",
        doc: "Local/self-hosted models via vLLM (high-throughput serving).",
    },
    Entry {
        name: "mistral",
        doc: "Mistral AI (EU, open-weight models available).",
    },
    Entry {
        name: "groq",
        doc: "Groq inference (cloud, low-latency).",
    },
    Entry {
        name: "deepseek",
        doc: "DeepSeek models (cloud).",
    },
    Entry {
        name: "openrouter",
        doc: "OpenRouter (cloud router across many providers/models).",
    },
    Entry {
        name: "huggingface",
        doc: "Hugging Face Inference Providers router (open-weight catalog · 18 backends · :provider or :fastest/:cheapest suffix).",
    },
    Entry {
        name: "nvidia",
        doc: "NVIDIA API (Nemotron 3 family · Open Model License · NIM self-hostable).",
    },
    Entry {
        name: "anthropic",
        doc: "Anthropic Claude models (cloud).",
    },
    Entry {
        name: "openai",
        doc: "OpenAI GPT models (cloud).",
    },
    Entry {
        name: "google",
        doc: "Google Gemini models (cloud).",
    },
    Entry {
        name: "xai",
        doc: "xAI Grok models (cloud).",
    },
    Entry {
        name: "mock",
        doc: "The in-engine deterministic mock provider (tests, offline).",
    },
];

/// Look an entry up by name in a table — the shared lookup behind hover.
#[must_use]
pub fn lookup(table: &[Entry], name: &str) -> Option<Entry> {
    table.iter().copied().find(|e| e.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_verbs_are_locked() {
        let names: Vec<&str> = VERBS.iter().map(|e| e.name).collect();
        assert_eq!(names, ["infer", "exec", "invoke", "agent"]);
    }

    #[test]
    fn top_level_keys_mirror_the_parser() {
        // Must match nika_schema::parser TOP_LEVEL_KEYS exactly.
        let names: Vec<&str> = TOP_LEVEL_KEYS.iter().map(|e| e.name).collect();
        assert_eq!(
            names,
            [
                "nika",
                "workflow",
                "description",
                "model",
                "vars",
                "env",
                "secrets",
                "permits",
                "tasks",
                "outputs",
            ]
        );
    }

    #[test]
    fn task_fields_exclude_the_verb_selectors() {
        for verb in VERBS {
            assert!(
                !TASK_FIELD_KEYS.iter().any(|f| f.name == verb.name),
                "verb {} must not be in TASK_FIELD_KEYS",
                verb.name
            );
        }
        assert!(TASK_FIELD_KEYS.iter().any(|f| f.name == "after"));
        assert!(TASK_FIELD_KEYS.iter().any(|f| f.name == "with"));
        assert!(
            !TASK_FIELD_KEYS.iter().any(|f| f.name == "depends_on"),
            "the dead form never re-enters the vocabulary"
        );
    }

    #[test]
    fn providers_are_local_first_and_complete() {
        let names: Vec<&str> = PROVIDERS.iter().map(|e| e.name).collect();
        assert_eq!(names.len(), 16, "the canonical 16-provider catalog");
        // local/open-weight first (vendor-agnostic teaching order)
        assert_eq!(names[0], "ollama");
        assert!(names.contains(&"anthropic"));
        assert!(names.contains(&"openai"));
        assert!(names.contains(&"mock"));
        // anthropic/openai never lead the list
        let anthropic = names.iter().position(|n| *n == "anthropic").unwrap_or(0);
        assert!(anthropic > 0, "anthropic is not the default example");
    }

    #[test]
    fn lookup_finds_and_misses() {
        assert_eq!(lookup(VERBS, "agent").map(|e| e.name), Some("agent"));
        assert!(lookup(VERBS, "fetch").is_none(), "fetch is not a verb");
        assert_eq!(
            lookup(TOP_LEVEL_KEYS, "tasks").map(|e| e.name),
            Some("tasks")
        );
    }

    #[test]
    fn every_entry_has_nonempty_docs() {
        for table in [VERBS, TOP_LEVEL_KEYS, TASK_FIELD_KEYS, PROVIDERS] {
            for entry in table {
                assert!(!entry.doc.is_empty(), "{} has empty doc", entry.name);
            }
        }
    }

    /// The MIRRORED-FROM-CANON providers order answers to the SPEC's
    /// token (design/tokens.yaml `presentation.providers_order`,
    /// vendored into the pack) — the last row of #557: a re-order lands
    /// spec-first, re-vendors, and THIS test stays red until the table
    /// follows. Same law as the CLI's verb-color parity pin.
    #[test]
    fn providers_order_matches_the_pack_design_tokens() {
        let tokens = nika_pack::design_tokens();
        let section = tokens
            .split("providers_order:")
            .nth(1)
            .expect("the pack tokens carry presentation.providers_order (sync-pack post spec#73)");
        let pinned: Vec<&str> = section
            .lines()
            .skip(1)
            .map_while(|l| l.trim().strip_prefix("- "))
            .collect();
        let ours: Vec<&str> = PROVIDERS.iter().map(|e| e.name).collect();
        assert_eq!(
            &ours[..pinned.len()],
            pinned.as_slice(),
            "vocab::PROVIDERS drifted from the spec's presentation order token"
        );
        // The token's own law for absentees: they rank AFTER. The one
        // engine extra today is `mock` (the test provider — real to the
        // engine, not a teaching row the spec orders).
        assert_eq!(
            &ours[pinned.len()..],
            ["mock"],
            "an engine-only provider must trail the spec-ordered set"
        );
    }
}
