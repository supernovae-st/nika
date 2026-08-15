// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! YAML → [`RawWorkflow`] parser — the canonical v1 language.
//!
//! Parses the spec envelope (`nika-spec/spec/01-envelope.md`) ·
//! `nika:` (the mark AND the file's name) · `model:` · `inputs:`
//! · `secrets:` · `tasks:` · `outputs:` — and the closed task shape of
//! `03-dag.md` with the 4 verbs of `02-verbs.md`.
//!
//! The parser is **shape-only** · it validates field forms (scalar vs
//! mapping · closed enums · the Go-duration grammar · exactly-one-verb)
//! and rejects unknown fields in [`ParseMode::Strict`]. Cross-reference
//! semantics (cycles · edge-target resolution · `${{ }}` namespace
//! resolution · the `when:` boolean-shape rule) are the analyzer's job.
//!
//! Presence of `nika:` / non-empty `tasks:` is ALSO the
//! analyzer's job (collected errors) — the parser only validates the
//! fields it sees.

pub(crate) mod envelope;
pub(crate) mod envelope_values;
pub(crate) mod for_each;
mod lift;
pub(crate) mod tasks;
mod value;
pub(crate) mod verbs;

use marked_yaml::types::MarkedMappingNode;
use marked_yaml::{LoadError, LoaderOptions, Span as YamlSpan, parse_yaml_with_options};

use crate::error::SchemaError;
use crate::raw::RawWorkflow;
use crate::source::{ByteOffset, FileId, Span, Spanned};

/// Forward-compat mode (spec `02-verbs.md` §forward-compat) ·
///
/// - [`Strict`](Self::Strict) — REJECT any unknown field anywhere
///   (top-level · task-level · verb-level). The conformance-test default.
/// - [`Lenient`](Self::Lenient) — ignore unknown fields (production
///   default per the spec · « Warn + ignore » · warnings TODO).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ParseMode {
    /// Reject unknown fields with a clear error (test default).
    #[default]
    Strict,
    /// Ignore unknown fields (production default).
    Lenient,
}

/// The canonical top-level envelope keys (spec `01-envelope.md` +
/// `10-authority.md` `policy:` + F-P3 `run:` ·
/// post-C2 the four-authority family — `vars`/`env` are NOT here: they get
/// their specific dead-form refusals (NIKA-VALUES-001/002) before this check).
/// The workflow envelope's accepted top-level keys — the authority.
///
/// `workflow` is NOT here since the envelope nuke (2026-08-12): the key
/// existed only to house `id:` and `description:`, the description died
/// (one consumer across five reading surfaces) and the id moved onto
/// `nika:`, so the object had nothing left to hold. `NIKA-PARSE-020` and
/// `NIKA-PARSE-021` are RETIRED with it — neither code is ever reused
/// (SSOT-2 B.22). The task-level `invoke: { workflow: … }` is ANOTHER key
/// and it LIVES (spec `14-composition.md`).
///
/// Public so consumers can DERIVE from it instead of retyping it. The
/// language server kept its own copy and a test that compared that copy
/// to a third hand-written list. A mirror validated against itself proves
/// nothing (2026-08-02).
pub const TOP_LEVEL_KEYS: &[&str] = &[
    "nika", "model", "inputs", "const", "secrets", "permits", "run", "tasks", "outputs",
];

/// Parse a YAML string into a [`RawWorkflow`].
///
/// `file_id` labels the source in downstream diagnostics; it is woven
/// into every [`Span`] attached to the returned workflow. `mode` picks
/// the unknown-field policy.
///
/// # Errors
///
/// Returns a [`SchemaError`] when the YAML is malformed, carries
/// duplicate mapping keys, violates a field's shape (closed enum ·
/// Go-duration · exactly-one-verb · id formats), or — in
/// [`ParseMode::Strict`] — carries an unknown field.
pub fn parse(yaml: &str, file_id: FileId, mode: ParseMode) -> Result<RawWorkflow, SchemaError> {
    // A leading UTF-8 BOM (U+FEFF · YAML 1.2 §5.2) parses — stripped at the ONE ingest seam.
    let yaml = yaml.strip_prefix('\u{FEFF}').unwrap_or(yaml);
    // Pre-parse resource guards (the untrusted-input bounds — see the
    // security note on `CharToByte::new`). marked-yaml allocates the
    // whole node tree up-front and its block parser recurses per
    // indentation level, so BOTH bounds must precede it.
    check_source_bounds(yaml)?;

    // YAML 1.2 forbids duplicate mapping keys from silently last-winning
    // — `error_on_duplicate_keys` turns them into loud errors (covers
    // vars/env/secrets/outputs/with/output duplicate-key detection).
    // `prevent_coercion` makes QUOTED scalars non-coercing (only plain
    // scalars type-coerce) — the YAML 1.2 contract `"42"` is a string.
    let options = LoaderOptions::default()
        .error_on_duplicate_keys(true)
        .prevent_coercion(true);
    let node =
        parse_yaml_with_options(file_id.0 as usize, yaml, options).map_err(|err| match err {
            LoadError::DuplicateKey(inner) => SchemaError::DuplicateKey {
                message: format!(
                    "\"{}\" appears twice in the same mapping",
                    inner.key.as_str()
                ),
                span: None,
            },
            // Pre-parse cause lint (the copy-fidelity class · #323): a weak
            // copier de-comments the editor modeline and YAML reads the bare
            // `$schema=…` line as a document scalar — the raw error then
            // points at the SYMPTOM (the first mapping line, e.g. `nika: v1`
            // at line 14) while the fault is line 1-2, and the repair loop
            // chases the wrong line forever (0/13 measured on a 14B grid).
            // Name the CAUSE, span on the offending line.
            other => match broken_modeline(yaml, file_id) {
                Some((span, line_no)) => SchemaError::YamlSyntax {
                    message: format!(
                        "a bare `$schema=` line (line {line_no}) is a broken editor \
                         modeline — restore the `# yaml-language-server: $schema=…` \
                         comment prefix (or delete the line; it is editor-only)"
                    ),
                    span: Some(span),
                },
                None => SchemaError::YamlSyntax {
                    message: other.to_string(),
                    span: None,
                },
            },
        })?;

    // Char-to-byte translation table: marked-yaml reports character
    // (code-point) indices, but `ByteOffset` — and every miette
    // `SourceSpan` downstream — wants byte offsets. Precompute once
    // per parse so each span lookup is O(1).
    let char_to_byte = CharToByte::new(yaml)?;

    let mapping = node.as_mapping().ok_or_else(|| SchemaError::Validation {
        message: "workflow root must be a YAML mapping".to_owned(),
        span: yaml_span_to_span(file_id, node.span(), &char_to_byte),
    })?;

    let cx = Cx {
        file_id,
        char_to_byte: &char_to_byte,
        mode,
    };

    // W1 « the map » + C2 « the E-split » dead forms get their SPECIFIC
    // teachings before the generic unknown-field check — structural
    // deaths, mode-independent.
    refuse_dead_envelope_forms(mapping, file_id, &char_to_byte)?;
    cx.check_unknown_keys(mapping, TOP_LEVEL_KEYS, "the workflow envelope")?;

    let mut workflow = RawWorkflow::new();

    // `nika:` carries BOTH the mark and the name (spec 01 §nika).
    if let Some(node) = mapping.get_node("nika") {
        workflow.workflow = Some(parse_nika_id(&cx, node)?);
    }
    if let Some(s) = extract_scalar(mapping, "model", file_id, &char_to_byte)? {
        workflow.model = Some(s);
    }

    workflow.inputs = envelope::parse_inputs(&cx, mapping)?;
    workflow.consts = envelope::parse_const(&cx, mapping)?;
    workflow.secrets = envelope::parse_secrets(&cx, mapping)?;
    workflow.permits = envelope::parse_permits(&cx, mapping)?;
    workflow.run = envelope::parse_run(&cx, mapping)?;
    workflow.outputs = envelope::parse_outputs(&cx, mapping)?;
    workflow.tasks = tasks::parse_tasks(&cx, mapping)?;

    Ok(workflow)
}

// ── Shared parse context ────────────────────────────────────────────

/// A spanned key → spanned value entry list, preserving YAML order
/// (`env:` · `secrets:` · `with:` · `output:` blocks).
pub(super) type SpannedEntries<T> = Vec<(Spanned<String>, Spanned<T>)>;

/// Maximum workflow source size — 4 MiB. marked-yaml allocates the
/// whole node tree up-front, so this bounds parse-time memory. A
/// hand-written workflow (even with inline JSON Schemas and long
/// prompts) is kilobytes; 4 MiB is a memory-safety bound, not a policy
/// limit — far above any real file, loud below the `u32::MAX` span
/// ceiling (which stays a separate span-correctness guard).
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// Maximum leading-space indentation accepted per line — ~512 block
/// nesting levels at 2 spaces/level, a 4–6× margin under the empirical
/// marked-yaml stack-overflow point (~3000 levels / 8 MB stack) and
/// generous for real files (block-scalar CONTENT with >1 KB of leading
/// spaces is not a workflow).
const MAX_INDENT_BYTES: usize = 1024;

/// Maximum COMPACT block-sequence nesting per line — a run of inline
/// `- ` markers (`- - - … x`) nests one YAML level EACH with ZERO
/// leading spaces, so the indent guard above never sees it and
/// marked-yaml recurses one frame per marker → an ~8 KB single line
/// (≈3000 markers) overflows the 8 MB stack and ABORTS the process (a
/// crash-DoS on every `nika check`/`run`/`lsp` over untrusted text).
/// 512 levels mirrors the indent budget with a 4×+ margin under the
/// empirical abort point; a real workflow never chains `- ` on one line.
const MAX_BLOCK_DASH_RUN: usize = 512;

/// The dead envelope forms — W1 « the map » (a stray top-level
/// `description:`) + C2 « the E-split » (the pre-C2 `vars:`/`env:`
/// fields) — each refused with its SPECIFIC classification teaching
/// (NIKA-VALUES-001/002 for the E-split · never the generic
/// unknown-field error · structural deaths, mode-independent).
fn refuse_dead_envelope_forms(
    mapping: &MarkedMappingNode,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<(), SchemaError> {
    // `description:` is NOT refused here any more — it died with the
    // `workflow:` object that housed it (envelope nuke 2026-08-12 · one
    // consumer across five reading surfaces · the semantic hash was
    // identical with it and without it). `NIKA-PARSE-021` is RETIRED and
    // never reused, so the key now falls to the generic unknown-key
    // refusal (`NIKA-PARSE-005`) like any other word that is not an
    // envelope key.
    for (key, form) in [
        ("vars", crate::error::DeadForm::Vars),
        ("env", crate::error::DeadForm::Env),
    ] {
        if let Some(node) = mapping.get_node(key) {
            return Err(SchemaError::DeadValueForm {
                form,
                message: form.field_teaching(),
                span: yaml_span_to_span(file_id, node.span(), char_to_byte),
            });
        }
    }
    Ok(())
}

/// The pre-parse resource guards — byte size + indentation depth, both
/// LOUD and both before marked-yaml allocates/recurses. One O(n) pass
/// over lines (leading spaces only — YAML forbids tabs in indentation).
/// Count the leading run of compact block-sequence markers — `- `
/// (dash + at least one space) repeated. Each is one nesting level in
/// YAML's compact form; a trailing `-` at EOL (no space) opens a level
/// too, so it counts as the final one.
fn compact_dash_run(rest: &str) -> usize {
    let mut n = 0;
    let mut b = rest.as_bytes();
    loop {
        if b.first() != Some(&b'-') {
            return n;
        }
        match b.get(1) {
            Some(&b' ') => {
                n += 1;
                // skip the dash + all following spaces to the next marker
                b = &b[1..];
                while b.first() == Some(&b' ') {
                    b = &b[1..];
                }
            }
            // `-` at end of line (or `-\t`, not a marker) — the last level.
            None => return n + 1,
            _ => return n,
        }
    }
}

/// The de-commented editor modeline (the copy-fidelity class · #323): an
/// early line reading `$schema=…` or `yaml-language-server: …` WITHOUT its
/// `#` prefix. Only consulted when the document already failed to parse —
/// this is a cause-namer, never a gate on valid YAML. Scans the head of
/// the file (the modeline contract is « at the top »; 8 lines covers
/// license headers) and returns the offending line's span + 1-based number.
fn broken_modeline(yaml: &str, file: FileId) -> Option<(Span, u32)> {
    let mut offset = 0u32;
    for (i, line) in yaml.lines().take(8).enumerate() {
        // Head-of-file offsets: bounded by 8 lines of a MAX_SOURCE_BYTES
        // (4 MiB) document — u32 holds by construction.
        let len = u32::try_from(line.len()).ok()?;
        let trimmed = line.trim_start();
        if trimmed.starts_with("$schema=") || trimmed.starts_with("yaml-language-server:") {
            let indent = u32::try_from(line.len() - trimmed.len()).ok()?;
            let span = Span::new(
                file,
                ByteOffset::new(offset + indent),
                ByteOffset::new(offset + len),
            );
            let line_no = u32::try_from(i).ok()? + 1;
            return Some((span, line_no));
        }
        offset += len + 1; // the LF the iterator swallowed
    }
    None
}

fn check_source_bounds(source: &str) -> Result<(), SchemaError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(SchemaError::YamlSyntax {
            message: format!(
                "workflow source is {} bytes (max {MAX_SOURCE_BYTES}) — \
                 rejected (memory-safety bound)",
                source.len()
            ),
            span: None,
        });
    }
    for (line_no, line) in source.lines().enumerate() {
        let indent = line.bytes().take_while(|&b| b == b' ').count();
        if indent > MAX_INDENT_BYTES {
            return Err(SchemaError::YamlSyntax {
                message: format!(
                    "line {} is indented {indent} spaces (max {MAX_INDENT_BYTES}) — \
                     nesting this deep is rejected (stack-safety bound)",
                    line_no + 1
                ),
                span: None,
            });
        }
        // Compact block-sequence depth: a leading run of `- ` markers
        // nests one level EACH with no indentation, so the indent guard
        // misses it (the `- - - … x` stack-overflow bomb). Count the run
        // after leading whitespace and cap it before marked-yaml recurses.
        let dashes = compact_dash_run(&line[indent..]);
        if dashes > MAX_BLOCK_DASH_RUN {
            return Err(SchemaError::YamlSyntax {
                message: format!(
                    "line {} opens {dashes} compact block levels (`- - …`, max \
                     {MAX_BLOCK_DASH_RUN}) — nesting this deep is rejected \
                     (stack-safety bound)",
                    line_no + 1
                ),
                span: None,
            });
        }
    }
    Ok(())
}

/// Per-parse context threaded through the submodules.
pub(super) struct Cx<'a> {
    pub(super) file_id: FileId,
    pub(super) char_to_byte: &'a CharToByte,
    pub(super) mode: ParseMode,
}

impl Cx<'_> {
    /// Translate a marked-yaml span.
    pub(super) fn span(&self, span: &YamlSpan) -> Option<Span> {
        yaml_span_to_span(self.file_id, span, self.char_to_byte)
    }

    /// Translate a marked-yaml span, falling back to a zero-point span.
    pub(super) fn span_or_zero(&self, span: &YamlSpan) -> Span {
        self.span(span)
            .unwrap_or_else(|| Span::point(self.file_id, ByteOffset::new(0)))
    }

    /// In [`ParseMode::Strict`] · reject any mapping key outside `known`
    /// (spec `02-verbs.md` §forward-compat · strict = test default).
    pub(super) fn check_unknown_keys(
        &self,
        mapping: &marked_yaml::types::MarkedMappingNode,
        known: &[&str],
        location: &str,
    ) -> Result<(), SchemaError> {
        if self.mode == ParseMode::Lenient {
            return Ok(());
        }
        self.check_unknown_keys_always(mapping, known, location)
    }

    /// Reject any mapping key outside `known` in BOTH parse modes.
    ///
    /// Reserved for security-bearing blocks (`permits:`) where a typo'd
    /// capability key silently changing the boundary would be a security
    /// bug — lenient mode does not apply there.
    pub(super) fn check_unknown_keys_always(
        &self,
        mapping: &marked_yaml::types::MarkedMappingNode,
        known: &[&str],
        location: &str,
    ) -> Result<(), SchemaError> {
        for (key, _) in mapping.iter() {
            if !known.contains(&key.as_str()) {
                // The modeline class, valid-YAML form (#323): with the `#`
                // stripped, `yaml-language-server: $schema=…` parses as a
                // top-level mapping key — the generic unknown-field message
                // sends the repairer hunting for a workflow field. Teach the
                // real fix instead of a Levenshtein guess.
                let suggestion = if key.as_str() == "yaml-language-server" {
                    Some(
                        "this is a de-commented editor modeline, not a workflow \
                         field — restore the `# ` comment prefix (or delete the \
                         line; it is editor-only)"
                            .to_owned(),
                    )
                } else {
                    nika_types::suggest::did_you_mean(key.as_str(), known.iter().copied())
                        .map(str::to_owned)
                        // No near-miss to assert: for a small closed set,
                        // teach the set itself — `env` in a secret is
                        // nobody's typo for `key`, the author needs the
                        // vocabulary (the chart-semantics precedent applied
                        // to parse · use-case battery 2026-07-11). Large
                        // sets (a task's keys) stay silent: a 20-item dump
                        // is noise, not teaching.
                        .or_else(|| {
                            // 9, not 8. Measured 2026-08-15: exactly two sets
                            // sit at nine — TOP_LEVEL_KEYS (the envelope) and
                            // AGENT_KEYS — and both were silent for the sake
                            // of one key over a round number. The envelope is
                            // the set an author meets first and the one whose
                            // vocabulary they are least likely to hold; it has
                            // never taught itself, at fourteen keys or at nine.
                            // A task's twenty keys still stay silent, which is
                            // the line this threshold exists to draw.
                            (known.len() <= 9)
                                .then(|| format!("the fields here: {}", known.join(" · ")))
                        })
                };
                return Err(SchemaError::UnknownField {
                    field: key.as_str().to_owned(),
                    location: location.to_owned(),
                    span: self.span(key.span()),
                    suggestion,
                });
            }
        }
        Ok(())
    }

    /// Extract an optional string scalar by key.
    pub(super) fn opt_scalar(
        &self,
        mapping: &marked_yaml::types::MarkedMappingNode,
        key: &str,
    ) -> Result<Option<Spanned<String>>, SchemaError> {
        extract_scalar(mapping, key, self.file_id, self.char_to_byte)
    }

    /// Extract a required string scalar by key.
    pub(super) fn require_scalar(
        &self,
        mapping: &marked_yaml::types::MarkedMappingNode,
        key: &str,
        location: &str,
    ) -> Result<Spanned<String>, SchemaError> {
        self.opt_scalar(mapping, key)?
            .ok_or_else(|| SchemaError::MissingField {
                field: format!("{location}.{key}"),
                span: self.span(mapping.span()),
            })
    }
}

// ── Helpers ─────────────────────────────────────────────────────

/// Precomputed char-index → byte-offset table for the source text.
///
/// `marked-yaml` 0.8 reports positions as character indices; our
/// [`ByteOffset`] and miette's `SourceSpan` want byte offsets. ASCII
/// inputs are the hot path, so the constructor short-circuits.
pub(super) struct CharToByte {
    /// `byte_at[char_idx]` is the UTF-8 byte offset. Contains
    /// `source.len()` as its final sentinel so end-of-file positions
    /// resolve without panicking.
    byte_at: Vec<u32>,
}

impl CharToByte {
    pub(super) fn new(source: &str) -> Result<Self, SchemaError> {
        // Guard against pathological inputs that would overflow u32.
        // 4 GB of YAML is not a workflow, it's a denial-of-service
        // attempt — fail loud rather than silently clamp.
        //
        // SECURITY NOTE (untrusted-input resource bounds · crate-spec §11):
        // this u32::MAX limit is a SPAN-CORRECTNESS bound, distinct from the
        // DoS bounds below. The untrusted-input guard SET is now complete —
        // all checked BEFORE marked-yaml allocates/recurses:
        //   • source byte cap        `MAX_SOURCE_BYTES` (4 MiB · memory)
        //   • indentation depth cap  `MAX_INDENT_BYTES` (block stack-safety)
        //   • YAML value nesting cap  `value::MAX_VALUE_DEPTH` (walker + Drop)
        //   • task-count cap         `tasks::MAX_TASKS` (analyzer DAG passes)
        // (empirical anchor: unbounded block nesting overflowed the stack at
        // ~3000 levels.) marked-yaml 0.8 does not expand anchors/aliases, so
        // the billion-laughs vector is closed and its flow parser self-limits
        // recursion (~150). What REMAINS pre-`nika serve` is policy, not
        // safety: per-tenant quotas + a wall-clock parse timeout.
        if source.len() > u32::MAX as usize {
            return Err(SchemaError::YamlSyntax {
                message: format!(
                    "workflow source exceeds {} bytes — spans would overflow",
                    u32::MAX
                ),
                span: None,
            });
        }
        // Fast path: if every byte is ASCII, char index = byte index.
        if source.is_ascii() {
            return Ok(Self {
                byte_at: Vec::new(),
            });
        }
        let mut byte_at: Vec<u32> = source
            .char_indices()
            .map(|(b, _)| u32::try_from(b).unwrap_or(u32::MAX))
            .collect();
        byte_at.push(u32::try_from(source.len()).unwrap_or(u32::MAX));
        Ok(Self { byte_at })
    }

    /// Translate a character index into a byte offset, clamping to
    /// end-of-file when the char index is out of range (which
    /// marked-yaml may report for synthetic end markers).
    pub(super) fn byte(&self, char_idx: usize) -> u32 {
        if self.byte_at.is_empty() {
            // ASCII fast path — char index IS the byte offset.
            return u32::try_from(char_idx).unwrap_or(u32::MAX);
        }
        let clamped = char_idx.min(self.byte_at.len().saturating_sub(1));
        self.byte_at[clamped]
    }
}

/// Extract an optional top-level string scalar by key.
///
/// Returns `Ok(None)` if the key is absent. Returns a
/// [`SchemaError::Validation`] if the key exists but the value is a
/// mapping or a sequence.
pub(super) fn extract_scalar(
    mapping: &marked_yaml::types::MarkedMappingNode,
    key: &str,
    file_id: FileId,
    char_to_byte: &CharToByte,
) -> Result<Option<Spanned<String>>, SchemaError> {
    let Some(node) = mapping.get_node(key) else {
        return Ok(None);
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: format!("`{key}` must be a scalar string"),
            span: yaml_span_to_span(file_id, node.span(), char_to_byte),
        });
    };
    let span = yaml_span_to_span(file_id, scalar.span(), char_to_byte)
        .unwrap_or_else(|| Span::point(file_id, ByteOffset::new(0)));
    Ok(Some(Spanned::new(scalar.as_str().to_owned(), span)))
}

/// `^[a-z][a-z0-9-]*$` — the kebab-case resource-name shape (spec
/// `01-envelope.md` · « it is a resource name, never referenced inside
/// an expression »). An empty string is not a name.
fn is_kebab_id(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Parse the `nika:` scalar — the file's NAME, kebab-case (spec
/// `01-envelope.md` §`nika` · « the key declares *this is a Nika file*;
/// the value is the file's name »).
///
/// The key held the literal `v1` until the envelope nuke (2026-08-12).
/// The version slot is gone FOREVER and that is LOSSLESS: `v1` was the
/// only legal value for the entire lifetime of the contract and there is
/// no `nika: v2` — ever. A field with one legal value is not a version,
/// so nothing was traded away and the slot now carries the file's most
/// necessary field instead. The magic-number function survives intact —
/// it was always done by the KEY, never by the value.
/// The shape is judged on the NODE, never via `get_scalar`: a mapping or
/// a sequence would otherwise read as ABSENT, and the id would go quietly
/// missing instead of loudly wrong.
fn parse_nika_id(
    cx: &Cx<'_>,
    node: &marked_yaml::types::Node,
) -> Result<Spanned<String>, SchemaError> {
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: "`nika:` must be a scalar string — the file's kebab-case name".to_owned(),
            span: yaml_span_to_span(cx.file_id, node.span(), cx.char_to_byte),
        });
    };
    let raw = scalar.as_str();
    let span = cx.span(scalar.span());
    if !is_kebab_id(raw) {
        return Err(SchemaError::BadNikaId {
            id: raw.to_owned(),
            span,
        });
    }
    Ok(Spanned::new(
        raw.to_owned(),
        span.unwrap_or_else(|| Span::point(cx.file_id, ByteOffset::new(0))),
    ))
}

/// Validate a task `id:` against `^[a-z][a-z0-9_]*$` (spec `03-dag.md` ·
/// `snake_case` · CEL-safe · a hyphen would parse as subtraction).
pub(super) fn validate_task_id(id: &Spanned<String>) -> Result<(), SchemaError> {
    let s = &id.value;
    let valid = s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if valid {
        Ok(())
    } else {
        Err(SchemaError::BadTaskId {
            id: s.clone(),
            span: Some(id.span),
        })
    }
}

/// Convert a `marked_yaml::Span` into our span (attached to `file_id`).
///
/// marked-yaml reports the start marker (and sometimes no end). When
/// both markers are present we produce a `[start, end)` range; when
/// only `start` is available the span is zero-length (point). An
/// entirely blank yaml span yields `None`. Character indices from
/// marked-yaml are translated to byte offsets via `char_to_byte`.
pub(super) fn yaml_span_to_span(
    file_id: FileId,
    span: &YamlSpan,
    char_to_byte: &CharToByte,
) -> Option<Span> {
    let start = span.start()?;
    let start_off = char_to_byte.byte(start.character());
    let end_off = span
        .end()
        .map_or(start_off, |m| char_to_byte.byte(m.character()));
    Some(Span::new(
        file_id,
        ByteOffset::new(start_off),
        ByteOffset::new(end_off),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fid() -> FileId {
        FileId::new(0)
    }

    fn parse_strict(yaml: &str) -> Result<RawWorkflow, SchemaError> {
        parse(yaml, fid(), ParseMode::Strict)
    }

    const MINIMAL: &str = "\
nika: hello
tasks:
  greet:
    infer:
      prompt: \"Say hi\"
";

    #[test]
    fn oversized_source_is_rejected_loud() {
        // a 4 MiB+ document rejects BEFORE marked-yaml allocates the tree
        let big = format!(
            "nika: w\ndescription: \"{}\"\ntasks: []\n",
            "x".repeat(MAX_SOURCE_BYTES)
        );
        let err = parse_strict(&big).expect_err("rejected");
        assert!(err.to_string().contains("memory-safety bound"), "{err}");
    }

    #[test]
    fn too_many_tasks_is_rejected_loud() {
        use std::fmt::Write as _;
        let mut yaml = String::from("nika: w\ntasks:\n");
        for i in 0..=tasks::MAX_TASKS {
            let _ = write!(yaml, "  t{i}:\n    exec: {{ command: [\"true\"] }}\n");
        }
        let err = parse_strict(&yaml).expect_err("rejected");
        assert!(err.to_string().contains("resource bound"), "{err}");
        assert!(err.to_string().contains("compose"), "actionable: {err}");
    }

    #[test]
    fn pathological_block_nesting_is_rejected_not_crashed() {
        // The empirical stack-overflow class (marked-yaml block parse
        // recursed past the 8 MB stack at ~3000 levels · CRASH before
        // this guard): both layers reject LOUD now.
        // Layer 1 · the indent guard (pre-parse · protects marked-yaml).
        let mut deep_block = String::new();
        for i in 0..600 {
            deep_block.push_str(&" ".repeat(2 * i));
            deep_block.push_str("k:\n");
        }
        let yaml = format!("nika: w\nx:\n{deep_block}");
        let err = parse_strict(&yaml).expect_err("rejected");
        assert!(
            err.to_string().contains("stack-safety bound"),
            "loud: {err}"
        );
        // Layer 2 · the value-depth cap (conversion · protects every
        // downstream Value walker) — flow style slips past layer 1.
        let depth = 140; // > MAX_VALUE_DEPTH(128) · < marked-yaml flow limit
        let inner = format!("{}1{}", "[".repeat(depth), "]".repeat(depth));
        let yaml = format!(
            "nika: w\ntasks:\n  t:\n    invoke: {{ tool: \"nika:log\", args: {{ v: {inner} }} }}\n"
        );
        let err = parse_strict(&yaml).expect_err("rejected");
        assert!(
            err.to_string().contains("nesting exceeds 128"),
            "loud: {err}"
        );
    }

    #[test]
    fn parse_minimal_canonical_envelope() {
        let wf = parse_strict(MINIMAL).expect("parse");
        assert_eq!(
            wf.workflow.as_ref().map(|s| s.value.as_str()),
            Some("hello")
        );
        assert_eq!(wf.tasks.len(), 1);
    }

    #[test]
    fn parse_all_top_level_scalars() {
        let yaml = "\
nika: my-workflow
model: anthropic/claude-sonnet-4-6
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.workflow.expect("nika").value, "my-workflow");
        assert_eq!(
            wf.model.expect("model").value,
            "anthropic/claude-sonnet-4-6"
        );
    }

    #[test]
    fn parse_bad_nika_id_errors() {
        // Spec 01 §nika · « **Anti-pattern** · do not write `nika: v1` ·
        // `nika: My_Workflow` · `nika: "1.0"`. The value is a kebab-case
        // id. » The dot, the underscore, the capital and the leading
        // digit are each outside `^[a-z][a-z0-9-]*$`.
        for bad in [
            "nika: v1.0\n",
            "nika: \"1\"\n",
            "nika: 1.0\n",
            "nika: My_Workflow\n",
            "nika: Bad_Id\n",
            "nika: 9lives\n",
            "nika: my_flow\n",
            "nika: \"\"\n",
        ] {
            let err = parse_strict(bad).expect_err("bad nika id");
            assert!(
                matches!(err, SchemaError::BadNikaId { .. }),
                "{bad:?} → {err:?}"
            );
        }
    }

    #[test]
    fn parse_bad_nika_id_carries_span() {
        let err = parse_strict("nika: Not_Kebab\n").expect_err("bad id");
        let SchemaError::BadNikaId { id, span } = err else {
            panic!("expected BadNikaId");
        };
        assert_eq!(id, "Not_Kebab");
        assert!(span.is_some(), "BadNikaId must carry its span");
    }

    #[test]
    fn parse_good_nika_ids() {
        for good in ["hello", "scrape-and-summarize", "a", "a1-b2"] {
            let yaml = format!("nika: {good}\n");
            let wf = parse_strict(&yaml).expect("parse");
            assert_eq!(wf.workflow.expect("nika").value, good);
        }
    }

    #[test]
    fn the_dead_version_literals_are_now_ordinary_names() {
        // DELIBERATE, and the inverse of what this file asserted before
        // the envelope nuke: `v1` and `v2` are legal kebab-case ids. The
        // old test refused `nika: v2` because the slot was a VERSION;
        // the slot is a NAME now, so a file may legitimately be called
        // `v2`. Nothing here blesses `nika: v1` as an envelope marker —
        // it simply names a workflow `v1`, and spec 01 calls that an
        // anti-pattern in prose, not a refusal.
        for name in ["v1", "v2", "v999"] {
            let yaml = format!("nika: {name}\n");
            let wf = parse_strict(&yaml).expect("a version-shaped name is just a name");
            assert_eq!(wf.workflow.expect("nika").value, name);
        }
    }

    #[test]
    fn the_dead_workflow_key_is_refused() {
        // Conformance fixture envelope/017-workflow-key-rejected. The
        // key existed only to house `id:` and `description:`; both are
        // gone, so it is not an envelope key at all any more and falls
        // to the generic unknown-key refusal.
        let err = parse_strict("nika: t\nworkflow: old-envelope-form\n")
            .expect_err("`workflow:` is not an envelope key");
        assert!(matches!(err, SchemaError::UnknownField { .. }), "{err:?}");
    }

    #[test]
    fn the_dead_top_level_description_is_refused() {
        // It died with the object that housed it (one consumer across
        // five reading surfaces). `NIKA-PARSE-021` is retired, so this
        // is the generic unknown-key refusal now, not a W1 teaching.
        let err = parse_strict("nika: t\ndescription: a demo\n")
            .expect_err("top-level `description:` is dead");
        assert!(matches!(err, SchemaError::UnknownField { .. }), "{err:?}");
    }

    #[test]
    fn strict_rejects_unknown_top_level_key() {
        // Conformance fixture envelope/005-unknown-top-level-key.
        let yaml = "\
nika: hello
foo: bar
tasks:
  greet:
    infer:
      prompt: \"hi\"
";
        let err = parse_strict(yaml).expect_err("unknown key");
        let SchemaError::UnknownField { field, .. } = err else {
            panic!("expected UnknownField, got {err:?}");
        };
        assert_eq!(field, "foo");
    }

    #[test]
    fn unknown_field_suggests_near_typos_and_stays_silent_far() {
        // The #1 beginner friction (user-sim punch list 2026-07-06): a
        // typo'd verb key (`infr:`) died as a bare « unknown field » while
        // the closed vocabulary sat RIGHT THERE in the rejection call.
        // And the suggest module's own law: a wrong suggestion is worse
        // than none — `zzzqx` is nowhere near the vocabulary, so silence.
        let near = "nika: h\ntasks:\n  g:\n    infr:\n      prompt: \"hi\"\n";
        let err = parse_strict(near).expect_err("unknown key");
        assert!(err.to_string().contains("did you mean `infer`?"), "{err}");
        let SchemaError::UnknownField { suggestion, .. } = err else {
            panic!("expected UnknownField, got {err:?}");
        };
        assert_eq!(suggestion.as_deref(), Some("infer"));

        let far = "nika: h\ntasks:\n  g:\n    zzzqx:\n      prompt: \"hi\"\n";
        let msg = parse_strict(far).expect_err("unknown key").to_string();
        assert!(!msg.contains("did you mean"), "{msg}");
    }

    #[test]
    fn the_envelope_teaches_its_own_vocabulary() {
        // Measured 2026-08-15. The set-listing fallback fired at `<= 8`, and
        // the envelope has NINE keys — so the one set an author meets first,
        // and the one whose vocabulary they are least likely to hold, said
        // « unknown field » and nothing else. At fourteen keys it was silent
        // too; the nuke did not cause this, it only brought the envelope
        // close enough to a round number for the silence to look deliberate.
        //
        // `config:` is the case that exposed it: a value authority that
        // SHIPPED in v0.108.0, so authors hold files carrying it, and its
        // replacement (an `inputs:` entry) is not guessable from a bare
        // refusal. It is too far from any envelope key for did-you-mean.
        let err = parse_strict(
            "nika: h\nconfig:\n  a: 1\ntasks:\n  g:\n    exec:\n      run: \"true\"\n",
        )
        .expect_err("config is not an envelope key");
        let SchemaError::UnknownField { suggestion, .. } = err else {
            panic!("expected UnknownField, got {err:?}");
        };
        let taught = suggestion.expect("the envelope must teach its set");
        assert!(taught.starts_with("the fields here:"), "{taught}");
        for key in TOP_LEVEL_KEYS {
            assert!(taught.contains(key), "`{key}` missing from: {taught}");
        }

        // The other side of the threshold, and the reason it exists: a task
        // carries far more keys than a reader can use as a hint, so that set
        // stays silent. A dump is not teaching.
        let far = "nika: h\ntasks:\n  g:\n    zzzqx: 1\n    exec:\n      run: \"true\"\n";
        let msg = parse_strict(far).expect_err("unknown task key").to_string();
        assert!(!msg.contains("the fields here"), "{msg}");
    }

    #[test]
    fn lenient_ignores_unknown_top_level_key() {
        let yaml = "\
nika: hello
foo: bar
tasks:
  greet:
    infer:
      prompt: \"hi\"
";
        let wf = parse(yaml, fid(), ParseMode::Lenient).expect("lenient parse");
        assert_eq!(wf.tasks.len(), 1);
    }

    #[test]
    fn duplicate_top_level_keys_error() {
        // YAML 1.2 · duplicate keys never silently last-win.
        let err = parse_strict("nika: first\nnika: second\n").expect_err("dup");
        assert!(matches!(err, SchemaError::DuplicateKey { .. }), "{err:?}");
    }

    #[test]
    fn parse_empty_yaml_yields_empty_workflow() {
        // The PARSER accepts an empty mapping — a missing `nika:` and
        // missing tasks are the ANALYZER's collected errors. This seam
        // survives the envelope nuke unchanged.
        let wf = parse_strict("").expect("empty yaml is shape-legal");
        assert!(wf.workflow.is_none());
        assert!(wf.tasks.is_empty());
    }

    #[test]
    fn parse_sequence_top_level_errors() {
        let err = parse_strict("- item\n").expect_err("sequence root must fail");
        assert!(matches!(err, SchemaError::YamlSyntax { .. }));
    }

    #[test]
    fn parse_nika_as_sequence_errors() {
        let err = parse_strict("nika:\n  - foo\n").expect_err("sequence value");
        assert!(matches!(err, SchemaError::Validation { .. }));
    }

    #[test]
    fn parse_yaml_syntax_error_maps_to_schema_error() {
        let err = parse_strict("nika: [unclosed\n").expect_err("bad yaml");
        assert!(matches!(err, SchemaError::YamlSyntax { .. }));
    }

    /// The copy-fidelity class (#323): a weak copier de-comments the
    /// modeline; the raw YAML error points at the first mapping line (the
    /// SYMPTOM, e.g. `nika: v1`) — the diagnostic must name the CAUSE with
    /// the span on the modeline itself, or the repair loop never converges
    /// (0/13 measured on a 14B grid).
    #[test]
    fn bare_modeline_parse_failure_names_the_cause() {
        let yaml = "$schema=https://nika.sh/schema/v1/workflow.schema.json\n\
                    nika: hello\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n";
        let err = parse_strict(yaml).expect_err("bare modeline breaks the doc");
        let SchemaError::YamlSyntax { message, span } = err else {
            panic!("expected YamlSyntax, got {err:?}");
        };
        assert!(
            message.contains("broken editor modeline")
                && message.contains("(line 1)")
                && message.contains("# yaml-language-server:"),
            "the diagnostic teaches the fix verbatim: {message}"
        );
        let span = span.expect("span lands on the modeline line, not the symptom");
        assert_eq!(span.start.0, 0, "starts at the offending line");
    }

    /// The BOM class (the 2026-07-31 edge-case sweep): a Windows-authored
    /// file opens with U+FEFF, and the strict reader glued it to the first
    // key — « unknown field `nika` — did you mean `nika`? », two
    // visually identical words. YAML 1.2 §5.2 allows the mark; the ingest
    // seam strips ONE so the file parses like its twin without one.
    #[test]
    fn a_utf8_bom_parses_like_the_bare_file() {
        let bare = "nika: hello\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n";
        let bommed = format!("\u{FEFF}{bare}");
        let wf = parse_strict(&bommed).expect("a leading BOM parses");
        assert_eq!(
            wf.workflow
                .as_ref()
                .expect("the workflow id")
                .value
                .as_str(),
            "hello",
            "the workflow survives"
        );
        // One BOM only: a second U+FEFF is content, not metadata — the
        // strict reader still speaks for it.
        let doubled = format!("\u{FEFF}{bommed}");
        assert!(
            parse_strict(&doubled).is_err(),
            "a second BOM is not metadata"
        );
    }

    /// The valid-YAML form of the same class: without its `#`, the
    /// `yaml-language-server:` line PARSES (a top-level key) — it lands as
    /// an unknown field, and the suggestion must teach the modeline fix,
    /// never a did-you-mean workflow field.
    #[test]
    fn bare_language_server_line_is_the_same_class() {
        let yaml = "# SPDX-License-Identifier: Apache-2.0\n\
                    yaml-language-server: $schema=https://nika.sh/x.json\n\
                    nika: hello\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n";
        let err = parse_strict(yaml).expect_err("unknown top-level field in strict");
        let SchemaError::UnknownField {
            field, suggestion, ..
        } = err
        else {
            panic!("expected UnknownField, got {err:?}");
        };
        assert_eq!(field, "yaml-language-server");
        let s = suggestion.expect("the modeline teaching replaces did-you-mean");
        assert!(
            s.contains("editor modeline") && s.contains("comment prefix"),
            "teaches the real fix: {s}"
        );
    }

    #[test]
    fn commented_modeline_never_fires_the_lint() {
        // The HEALTHY form — parse succeeds, the lint is never consulted.
        let yaml = "# yaml-language-server: $schema=https://nika.sh/x.json\n\
                    nika: hello\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n";
        parse_strict(yaml).expect("commented modeline is valid YAML");
    }

    #[test]
    fn unrelated_syntax_error_keeps_the_raw_message() {
        let err = parse_strict("nika: [unclosed\n").expect_err("bad yaml");
        let SchemaError::YamlSyntax { message, .. } = err else {
            panic!("expected YamlSyntax");
        };
        assert!(
            !message.contains("modeline"),
            "no modeline in the file — the raw error stands: {message}"
        );
    }

    #[test]
    fn parse_file_id_propagates_into_span() {
        let wf = parse("nika: x\n", FileId::new(42), ParseMode::Strict).expect("parse");
        assert_eq!(wf.workflow.expect("workflow").span.file, FileId::new(42));
    }

    #[test]
    fn parse_error_span_carries_original_file_id() {
        let err =
            parse("nika:\n  - foo\n", FileId::new(42), ParseMode::Strict).expect_err("seq value");
        assert!(
            matches!(
                &err,
                SchemaError::Validation {
                    span: Some(span),
                    ..
                } if span.file == FileId::new(42)
            ),
            "expected Validation with span carrying FileId(42), got {err:?}",
        );
    }

    #[test]
    fn parse_non_ascii_value_span_starts_at_correct_byte() {
        // Regression lock for the char-index → byte-offset fix.
        let yaml = "desc_\u{00e9}: skip\nnika: hit\n";
        let wf = parse(yaml, fid(), ParseMode::Lenient).expect("parse");
        let spanned = wf.workflow.expect("workflow present");
        let line1_bytes = "desc_\u{00e9}: skip\n".len();
        // "nika: " (6 bytes) precedes the value on line 2
        let expected_start = u32::try_from(line1_bytes + 6).expect("fits");
        assert_eq!(
            spanned.span.start,
            ByteOffset::new(expected_start),
            "span start must be byte offset after char→byte translation",
        );
    }

    // ── check_source_bounds · the untrusted-input resource guards ───────
    //
    // These exercise the pre-parse guard DIRECTLY (the test module sees it
    // via `use super::*`), so the byte/indent boundary math is exact —
    // no YAML-envelope overhead muddies the count. They pin every `>`/`>=`
    // boundary and the `MAX_SOURCE_BYTES = 4 * 1024 * 1024` arithmetic so a
    // flipped operator or a `*`→`+` arithmetic mutation flips an
    // accept/reject and FAILS here.

    #[test]
    fn max_source_bytes_constant_is_exactly_four_mib() {
        // Pins the `4 * 1024 * 1024` arithmetic (parser:163). The two `*`
        // operators each have a `*`→`+` mutant:
        //   • first  `*`→`+` ⇒ 4 + 1024*1024 = 1_048_580
        //   • second `*`→`+` ⇒ 4*1024 + 1024 = 5_120
        // Both differ from 4_194_304, so this literal-pin kills both.
        assert_eq!(MAX_SOURCE_BYTES, 4_194_304);
        assert_eq!(MAX_SOURCE_BYTES, 4 * 1024 * 1024);
    }

    #[test]
    fn source_at_exactly_max_bytes_is_accepted() {
        // The boundary is `len > MAX_SOURCE_BYTES` (strict). A source of
        // EXACTLY 4 MiB must pass — this is the accept side of the `>`
        // boundary (parser:176) AND it kills the arithmetic mutants:
        // if MAX_SOURCE_BYTES were 5_120 or 1_048_580 (the `*`→`+`
        // products) a 4 MiB source would be > the cap and REJECTED here.
        let source = "x".repeat(MAX_SOURCE_BYTES);
        assert_eq!(source.len(), 4_194_304);
        check_source_bounds(&source).expect("a source of exactly MAX_SOURCE_BYTES is accepted");
    }

    #[test]
    fn source_one_over_max_bytes_is_rejected() {
        // `>` boundary, reject side (parser:176). One byte over the cap
        // rejects loud. With `>` mutated to `>=`, the exact-cap accept
        // test above already fails; with the arithmetic mutated the cap
        // shrinks and the accept test fails — this pins the over side.
        let source = "x".repeat(MAX_SOURCE_BYTES + 1);
        let err = check_source_bounds(&source).expect_err("one byte over the cap rejects");
        assert!(
            matches!(err, SchemaError::YamlSyntax { .. }),
            "byte-cap breach is a YamlSyntax error: {err:?}",
        );
        assert!(
            err.to_string().contains("memory-safety bound"),
            "loud + actionable: {err}",
        );
    }

    #[test]
    fn compact_block_dash_run_at_exactly_max_is_accepted() {
        // A run of EXACTLY MAX_BLOCK_DASH_RUN `- ` markers passes (the
        // `>`→`>=` mutant would reject it). Real workflows never chain
        // this many on one line, but the boundary must be exact.
        let line = format!("{}x", "- ".repeat(MAX_BLOCK_DASH_RUN));
        check_source_bounds(&line).expect("exactly-max compact dash run is accepted");
    }

    #[test]
    fn compact_block_dash_run_bomb_is_rejected_not_a_stack_overflow() {
        // THE bomb: `- - - … x` nests one YAML level per marker with no
        // leading spaces, so the indent guard misses it and marked-yaml
        // recursed to a process ABORT (stack overflow) on ~3000 markers —
        // a crash-DoS on every check/run/lsp over untrusted text. The
        // dash-run cap rejects it LOUD before marked-yaml recurses.
        let line = format!("{}x", "- ".repeat(MAX_BLOCK_DASH_RUN + 1));
        let err = check_source_bounds(&line).expect_err("over-cap compact dash run rejects");
        assert!(
            err.to_string().contains("compact block levels")
                && err.to_string().contains("stack-safety bound"),
            "loud + names the vector: {err}",
        );
    }

    #[test]
    fn compact_dash_run_counts_only_leading_markers() {
        // A `-` inside a value (`key: a-b`) or a lone list item (`- one`)
        // is not a nesting run — the cap must not false-fire on real YAML.
        assert_eq!(super::compact_dash_run("- one"), 1);
        assert_eq!(super::compact_dash_run("key: a-b-c"), 0);
        assert_eq!(super::compact_dash_run("- - - x"), 3);
        assert_eq!(super::compact_dash_run("-"), 1); // dash at EOL is a level
        assert_eq!(super::compact_dash_run(""), 0);
        // A normal short workflow list never trips it.
        check_source_bounds("tasks:\n  a:\n  b:\n").expect("a real 2-item list is fine");
    }

    #[test]
    fn indent_at_exactly_max_is_accepted() {
        // `indent > MAX_INDENT_BYTES` boundary (parser:188), accept side.
        // A line indented EXACTLY MAX_INDENT_BYTES spaces passes. This is
        // the arm a `>`→`>=` mutation would break (it would reject the
        // exact-cap line) — so this test kills the `>`→`>=` mutant.
        let line = format!("{}k: v", " ".repeat(MAX_INDENT_BYTES));
        check_source_bounds(&line).expect("indentation of exactly MAX_INDENT_BYTES is accepted");
    }

    #[test]
    fn indent_one_over_max_is_rejected() {
        // `indent > MAX_INDENT_BYTES` boundary (parser:188), reject side.
        // One space over the cap rejects loud — pins the over side and,
        // paired with the exact-cap accept above, locks the `>` operator
        // (a `>`→`==` mutation would accept this over-cap line because
        // 1025 == 1024 is false, so the accept test above stays green but
        // 1025 > 1024 must reject HERE; a `>`→`>=` mutation rejects the
        // exact-cap line in the accept test).
        let line = format!("{}k: v", " ".repeat(MAX_INDENT_BYTES + 1));
        let err = check_source_bounds(&line).expect_err("one space over the indent cap rejects");
        assert!(
            matches!(err, SchemaError::YamlSyntax { .. }),
            "indent-cap breach is a YamlSyntax error: {err:?}",
        );
        assert!(
            err.to_string().contains("stack-safety bound"),
            "loud + actionable: {err}",
        );
    }

    #[test]
    fn indent_well_over_max_is_rejected_with_line_number() {
        // The reject message names the 1-based line — pins the `line_no + 1`
        // rendering (parser:193). The over-indented line is the 2nd line
        // (0-based line_no == 1 → "line 2"); a `+`→`-` mutant renders
        // "line 0", a `+`→`*` mutant "line 1". Also locks the `>`→`==`
        // mutant on the indent compare: an indent of MAX+1024 is NOT
        // EXACTLY MAX, so `==` would WRONGLY accept it and this expect_err
        // would fail.
        let source = format!("ok: v\n{}deep: v", " ".repeat(MAX_INDENT_BYTES + 1024));
        let err = check_source_bounds(&source).expect_err("deep indent rejects");
        let msg = err.to_string();
        assert!(msg.contains("stack-safety bound"), "loud: {msg}");
        assert!(msg.contains("line 2"), "names the offending line: {msg}");
    }

    #[test]
    fn empty_and_shallow_sources_pass_the_guard() {
        // The accept floor — nothing about an ordinary file trips either
        // cap. Guards against a guard that rejects everything (a constant
        // that collapsed to 0 via arithmetic mutation would fail here).
        check_source_bounds("").expect("empty source passes");
        check_source_bounds("nika: w\n").expect("a normal file passes");
        check_source_bounds(&format!("{}k: v", " ".repeat(MAX_INDENT_BYTES - 1)))
            .expect("just under the indent cap passes");
    }
}
