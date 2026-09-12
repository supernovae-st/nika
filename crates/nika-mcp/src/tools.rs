// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MCP tool catalog — Nika's STATIC, read-only surface exposed as Model
//! Context Protocol tools. Every tool here is PURE over its arguments — static
//! analysis (parse · check · code lookup · the in-memory repair ladder behind
//! `nika_check(fix: true)`, which hands the repaired text BACK and writes
//! nothing) or embedded pack data (schema · examples · templates · canon) —
//! zero effects, zero network, no workflow ever RUNS through MCP (running
//! needs the effect-permits boundary · out of scope for the read-only server
//! surface). That purity is what makes a tool safe to expose to any
//! connecting client (Cursor · Claude Desktop · …) and lets the whole server
//! be unit-tested as a function.
//!
//! Two tool families:
//! - **validate** (`nika_check` · `nika_explain`) — the repair oracle.
//! - **learn** (`nika_schema` · `nika_examples` · `nika_template` ·
//!   `nika_canon` · `nika_catalog` · `nika_tools`) — the authoring surface,
//!   so a wired agent follows the deterministic template→fill→check→repair
//!   protocol instead of guessing structure (the spec's §Writing-a-workflow
//!   path, reachable over MCP) and picks REAL providers/models/tools from
//!   the versioned projections instead of remembered ids.

use std::fmt::Write as _;

use serde_json::{Value, json};

/// The tool catalog (`tools/list`): name · description · `inputSchema`
/// (JSON Schema the client validates arguments against).
#[must_use]
pub(crate) fn catalog() -> Value {
    let mut all = validate_tools().as_array().cloned().unwrap_or_default();
    all.extend(learn_tools().as_array().cloned().unwrap_or_default());
    for tool in &mut all {
        let Some(obj) = tool.as_object_mut() else {
            continue;
        };
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        obj.insert("title".into(), json!(display_title(&name)));
        obj.insert("annotations".into(), read_only_annotations());
    }
    Value::Array(all)
}

/// The behaviour hints every tool in this oracle carries, stamped in ONE
/// place because they are true of ALL of them: each answers from the
/// binary's embedded canon (schema · examples · templates · catalogs) or
/// from a static audit of workflow text it was handed. Nothing mutates,
/// nothing spawns, nothing opens a socket — running a workflow is a
/// separate, explicit human act on the CLI.
///
/// Clients read these to decide whether to interrupt the human on every
/// call (MCP 2025-06-18 · tool annotations). Leaving them unset is what
/// made a read-only oracle feel as dangerous as a shell.
fn read_only_annotations() -> Value {
    json!({
        "readOnlyHint": true,
        "destructiveHint": false,
        "idempotentHint": true,
        "openWorldHint": false
    })
}

/// The human-facing name a client shows beside the wire id. A tool with
/// no title falls back to its id, and the parity test below refuses it —
/// a tenth tool must name itself rather than inherit a blank.
fn display_title(name: &str) -> &'static str {
    match name {
        "nika_check" => "Audit a workflow",
        "nika_inspect" => "Project the workflow graph",
        "nika_explain" => "Explain an error code or hint",
        "nika_schema" => "The workflow JSON Schema",
        "nika_examples" => "Browse runnable examples",
        "nika_template" => "Fetch a template skeleton",
        "nika_canon" => "The spec canon",
        "nika_catalog" => "Providers and models",
        "nika_tools" => "The builtin catalog",
        _ => "",
    }
}

/// The VALIDATE half — audit and project a workflow before a token.
fn validate_tools() -> Value {
    json!([
        {
            "name": "nika_check",
            "description": "Statically audit a Nika workflow (schema · DAG · CEL · \
                            effects · permits · cost) BEFORE running it. Returns the \
                            findings, or a clean verdict — auditable before a token is spent. \
                            With `fix: true`: apply the machine-applicable repairs (the same \
                            ladder as `nika check --fix`) in memory, re-audit, and return the \
                            repaired source for you to write back.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "fix": {
                        "type": "boolean",
                        "default": false,
                        "description": "Apply the typed renames and dead-form migrations \
                                        `nika check --fix` applies — in memory, never to a \
                                        file — then re-audit. The answer is JSON: `repairs`, \
                                        `applied`, `changed`, the repaired `workflow` text \
                                        (write it back verbatim) and the plain `verdict` of \
                                        that text. Codes that prescribe `nika check --fix` \
                                        (NIKA-VAR-021 · NIKA-PARSE-024) are repaired here."
                    },
                    "native_strict": {
                        "type": "boolean",
                        "default": true,
                        "description": "Fail on native-first hints — an `exec` of a \
                                        helper script that a builtin already covers. \
                                        ON by default: this oracle is what an agent \
                                        consults before handing a file to a human, and \
                                        the gate in front of `nika run` uses the same \
                                        posture. Pass false to see the advisory verdict."
                    },
                    "verbose": {
                        "type": "boolean",
                        "default": false,
                        "description": "Return the full verdict object even when clean — the \
                                        same keys a dirty answer carries: `clean` · \
                                        `verdicts.{valid,access_ready,capacity_fit,run_ready}` · \
                                        `judged.{composition,skills,children}` (this oracle \
                                        reads no files: children and skills are never judged \
                                        here) · `model_findings[]` · `access_plan[]` · \
                                        `risk_grade` · `hints[]` · `next_actions[]`."
                    },
                    "workflow": {
                        "type": "string",
                        "description": "The *.nika.yaml workflow source."
                    }
                },
                "required": ["workflow"]
            }
        },
        {
            "name": "nika_inspect",
            "description": "Project a Nika workflow's DAG as the canonical \
                            graph document (graph_format: 3 — the same bytes \
                            `nika inspect --format json` prints and the LSP's \
                            nika/semanticDocument serves): wave-ordered nodes \
                            with verbs, models, permits, cost intervals; \
                            typed edges (value/observation from `with:` \
                            bindings · control from `after:` · recovery). \
                            Null graph + a one-word reason while the \
                            document has findings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workflow": {
                        "type": "string",
                        "description": "The *.nika.yaml workflow source."
                    }
                },
                "required": ["workflow"]
            }
        },
        {
            "name": "nika_explain",
            "description": "Teach one Nika error code or hint identity — cause, category, and the fix form.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "A code like `NIKA-VAR-001`, the bare `440`, or a hint kind `nika check` printed in [brackets] (`jq-as-map` · `native-first/006`)."
                    }
                },
                "required": ["code"]
            }
        },
    ])
}

/// The LEARN half — the embedded canon an agent reads.
fn learn_tools() -> Value {
    json!([
        {
            "name": "nika_schema",
            "description": "The embedded JSON Schema for *.nika.yaml — the structural \
                            contract (verbs · fields · shapes) an agent authors against.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "nika_examples",
            "description": "Browse the embedded runnable examples. Without `slug`: the \
                            JSONL metadata index (`slug` · `form` · `one_line` · `cost`). \
                            With `slug`: that example's full workflow source — read the \
                            canonical example instead of guessing a construct. With \
                            `builtin`: the examples that call one `nika:*` tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": {
                        "type": "string",
                        "description": "An example slug from the list (e.g. `pr-risk-review`)."
                    },
                    "builtin": {
                        "type": "string",
                        "description": "A builtin name (`nika:jq` or bare `jq`) — one JSONL \
                                        row per example that calls it (`slug` · `builtin` · \
                                        `one_line` · `sites`). Read a precedent for a tool's \
                                        args instead of guessing them."
                    }
                }
            }
        },
        {
            "name": "nika_template",
            "description": "The canonical workflow skeletons (chain · gate-and-act · \
                            fanout · …). Without `name`: the list. With `name`: that \
                            skeleton's source — copy it, fill the SLOT lines, never \
                            invent structure.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "A template name from the list (e.g. `chain`)."
                    }
                }
            }
        },
        {
            "name": "nika_canon",
            "description": "The spec canon SSOT (canon.yaml) — the locked counts and \
                            names: verbs, builtins, providers, extract modes. Cite it, \
                            never a remembered number.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "nika_catalog",
            "description": "The embedded provider/model catalog — providers, models, \
                            capabilities (vision · reasoning · json_mode), context \
                            windows, and API-key env-var NAMES (values never read). \
                            Versioned wire `catalog_version: 1`. Pick REAL model ids \
                            from here instead of guessing — and pick them ONLY from \
                            rows where `resolves` is true. A cataloged vendor is not a \
                            runnable one: `resolves: false` means this binary has no \
                            adapter for it and `nika check` refuses the model at the \
                            MODELS rung.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "nika_tools",
            "description": "The embedded builtin-tool catalog — every `nika:*` tool \
                            with its model-facing JSON-Schema (`parameters`) joined \
                            with the check-time contract (category · args · required). \
                            Versioned wire `tools_version: 1`.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

/// Execute a tool by name. `Ok(text)` is the success content; `Err(text)` is a
/// tool-level error (surfaced as `isError: true`, NOT a protocol error). PURE.
pub(crate) fn execute(name: &str, args: &Value) -> Result<String, String> {
    match name {
        "nika_check" => check(args),
        "nika_inspect" => inspect(args),
        "nika_explain" => explain(args),
        "nika_schema" => Ok(nika_pack::schema_json().to_owned()),
        "nika_examples" => crate::learn::examples(args),
        "nika_template" => crate::learn::template(args),
        "nika_canon" => Ok(nika_pack::canon().to_owned()),
        "nika_catalog" => catalog_payload(),
        "nika_tools" => tools_payload(),
        other => Err(format!(
            "unknown tool `{other}` — nika exposes nika_check · nika_inspect · \
             nika_explain · nika_schema · nika_examples · nika_template · \
             nika_canon · nika_catalog · nika_tools"
        )),
    }
}

/// `nika_check` — parse + the static check ladder over the supplied YAML.
/// The verdict for a workflow that passed all TEN finding surfaces but
/// may still leave the native path. Split out of `check` to stay under
/// the house function cap. The risk grade rides every arm — the CLI card
/// shows it on every audited card (P0-6: « clean » alone never names the
/// rope a declared grant hands over).
fn native_first_verdict(
    native: &[&str],
    strict: bool,
    grade: nika_check::RiskGrade,
) -> Result<String, String> {
    let word = grade.as_str();
    if native.is_empty() {
        return Ok(format!(
            "✔ clean — audited before a single token was spent · risk {word}"
        ));
    }
    let rows = native
        .iter()
        .map(|advice| format!("  · {advice}"))
        .collect::<Vec<_>>()
        .join("\n");
    let n = native.len();
    if strict {
        return Err(format!(
            "✖ native-first — schema, DAG, effects and permits are clean, but {n} \
             call(s) leave the native path. Replace each with the builtin its hint \
             names; the exec ledger documents intent without clearing this. The gate \
             in front of `nika run` uses the same posture, so this file cannot be run \
             as written (risk {word}):\n{rows}"
        ));
    }
    Ok(format!(
        "✔ clean (advisory) — audited before a single token was spent · risk {word}. {n} \
         native-first hint(s) are NOT enforced because native_strict=false; the same \
         file fails `nika check --native-strict` and the run gate:\n{rows}"
    ))
}

/// The `is_clean` mirror for the *expensive* paid-run pair.
/// `infer-as-law` and `digit-string-enum` burned the 2026-08-19 wave.
/// The rest of [`nika_check::PAID_RUN_KINDS`] still ride `.paid_ready`
/// (JSON) and the explain panel — they are not fail-set members
/// (`glob-readme` has no FS ·
/// `jq-as-map` is a style ratchet). This oracle is what an agent reads
/// before handing a file to a human.
fn paid_ready_verdict(
    paid: &[&nika_check::Hint],
    strict: bool,
    grade: nika_check::RiskGrade,
    prefix: &str,
) -> Result<String, String> {
    let hard: Vec<&&nika_check::Hint> = paid
        .iter()
        .filter(|h| h.kind == "infer-as-law" || h.kind == "digit-string-enum")
        .collect();
    let word = grade.as_str();
    if hard.is_empty() {
        if paid.is_empty() {
            return Ok(prefix.to_owned());
        }
        let rows = paid
            .iter()
            .map(|h| format!("  · [{}] {}", h.kind, h.advice))
            .collect::<Vec<_>>()
            .join("\n");
        return Ok(format!(
            "{prefix}\npaid_ready: false — {n} paid-run hint(s) remain \
             (risk {word}); they do not fail this oracle. Repair before \
             leaving `mock/`:\n{rows}",
            n = paid.len()
        ));
    }
    let rows = hard
        .iter()
        .map(|h| format!("  · [{}] {}", h.kind, h.advice))
        .collect::<Vec<_>>()
        .join("\n");
    let n = hard.len();
    if strict {
        return Err(format!(
            "✖ paid-ready — schema, DAG, effects and permits are clean, but {n} \
             paid-run hint(s) mean this file is not the one-way. Extract facts; \
             `nika:jq` / `nika:decide` is the law. Do not leave `mock/` \
             (risk {word}):\n{rows}"
        ));
    }
    Ok(format!(
        "{prefix}\n✔ clean (advisory) — paid_ready is false; {n} paid-run \
         hint(s) are NOT enforced because native_strict=false. The same file \
         fails this oracle by default (risk {word}):\n{rows}"
    ))
}

/// The AFFIRMATIVE contract, rendered on a GREEN check.
///
/// `permits` carries this in its own doc comment: consumers should
/// « render the positive contract on a green check instead of
/// reconstructing it from graph labels ». The CLI card does. This
/// oracle did not: it returned one line naming only the risk word,
/// while the report beside it held the wave plan, the cost ceiling, the
/// journey and the boundary in force. Measured 2026-08-20 — the JSON
/// key set is IDENTICAL clean and dirty, so nothing was ever gated;
/// the facts were computed and dropped at the render.
///
/// It matters here more than anywhere. This tool's own description says
/// it is what an agent consults BEFORE handing a file to a human, and a
/// green is exactly the moment that agent has something to report: what
/// the file touches, what it can spend, and whether it may leave mock.
/// « Valid » is not the question a reviewer is about to be asked.
///
/// A pure projection of the report — zero new scan, same source as the
/// JSON lane, so the two surfaces cannot drift.
fn affirmative_contract(report: &nika_check::CheckReport) -> String {
    let tasks: usize = report.waves.iter().map(Vec::len).sum();
    let waves = report.waves.len();
    // `PermitsSource` is `#[non_exhaustive]`, so the arm exists. It fails
    // CLOSED on purpose: a source this build does not know must never
    // render as a declared boundary, because that sentence is the one a
    // reviewer trusts. Naming the variant beats inventing a reading.
    let boundary = match report.permits.source {
        nika_check::PermitsSource::Declared => {
            format!("permits {}", report.permits.glance())
        }
        nika_check::PermitsSource::Absent => "no permits declared (zero authority)".to_owned(),
        ref other => {
            return format!(
                "  boundary source `{other:?}` is newer than this renderer — \
                 read the JSON lane, do not trust this line"
            );
        }
    };
    let spend = if report.cost.has_unbounded {
        "est out UNBOUNDED (a local or unpriced model is never $0)".to_owned()
    } else {
        format!("est out ≤${:.4}", report.cost.bounded_total_usd)
    };
    let journey = &report.data_journey;
    format!(
        "  {tasks} task(s) · {waves} wave(s) · {boundary} · {spend} ·          {} source(s) → {} destination(s) · {} model endpoint(s) · data {}",
        journey.sources.len(),
        journey.destinations.len(),
        journey.model_endpoints.len(),
        journey.classification.as_str(),
    )
}

/// The clean short-path — split out of `check` under the house function
/// cap (the `native_first_verdict` precedent). Still carries the catalog
/// cross-check: the ghost-model specimen IS clean (the provider
/// resolves), and a warning that only rode the dirty path would never
/// be seen.
fn clean_verdict(
    native: &[&str],
    paid: &[&nika_check::Hint],
    strict: bool,
    grade: nika_check::RiskGrade,
    catalog_warnings: &[Value],
) -> Result<String, String> {
    let verdict = native_first_verdict(native, strict, grade)?;
    let verdict = paid_ready_verdict(paid, strict, grade, &verdict)?;
    if catalog_warnings.is_empty() {
        return Ok(verdict);
    }
    let rows = catalog_warnings
        .iter()
        .filter_map(|w| w.get("why").and_then(Value::as_str))
        .map(|why| format!("  ⚠ {why}"))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!("{verdict}\n{rows}"))
}

/// Derive the repair hand-offs from the report's serialized finding surface.
/// A sorted set gives stable order and one action per code even when several
/// findings share a class (for example both missing envelope fields).
fn finding_next_actions(payload: &serde_json::Map<String, Value>) -> Value {
    let codes = payload
        .get("findings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|finding| finding.get("code").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    Value::Array(
        codes
            .into_iter()
            .map(|code| Value::String(format!("nika_explain {code}")))
            .collect(),
    )
}

/// `nika_check` — the argument door. `fix: true` walks the in-memory
/// repair ladder first ([`crate::repair`]) and audits what it produced;
/// otherwise the plain audit. Strict is the DEFAULT here (unlike the CLI,
/// where the bare verb is the human's advisory read): this tool is the
/// agent-facing oracle, and an oracle laxer than the gate it feeds is
/// worse than none.
fn check(args: &Value) -> Result<String, String> {
    let yaml = args
        .get("workflow")
        .and_then(Value::as_str)
        .ok_or("missing `workflow` (the *.nika.yaml source)")?;
    let native_strict = args
        .get("native_strict")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if args.get("fix").and_then(Value::as_bool).unwrap_or(false) {
        return crate::repair::check_fix(yaml, native_strict);
    }
    let verbose = args
        .get("verbose")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    audit_with(yaml, native_strict, verbose)
}

/// The plain audit — the ONE facade (`nika_cli_host::oracle` · ADR-124):
/// the SAME judgment `nika check` renders, over the supplied YAML,
/// whether the text came from the caller or from the repair ladder
/// (`fix: true` is check plus a pen, never a different audit). Source
/// only: composition and skills are unjudged here, and the verdict
/// object says so (`judged`).
pub(crate) fn audit(yaml: &str, native_strict: bool) -> Result<String, String> {
    audit_with(yaml, native_strict, false)
}

/// [`audit`] with `verbose`: a clean answer carries the verdict object too
/// (W3-F8 · the same keys a dirty answer carries).
fn audit_with(yaml: &str, native_strict: bool, verbose: bool) -> Result<String, String> {
    let audit = nika_cli_host::oracle::audit_source(
        yaml,
        "-",
        None,
        None,
        nika_cli_host::oracle::AuditOptions::default(),
    )
    .map_err(|e| format!("PARSE ✗ {}", e.diagnostic()))?;
    let lanes = nika_cli_host::oracle::Lanes::new(native_strict, false);
    // The is_clean mirror law, applied to the native-first lane. `hints`
    // are NOT part of `clean`: a workflow whose real work sits in
    // `exec python3 helper.py` reads "✔ clean" here while `nika check
    // --native-strict` refuses it — an oracle laxer than the gate it
    // feeds is worse than none, so strict is the DEFAULT on this lane
    // (`native_strict` arrives resolved from the `check` dispatcher).
    let native: Vec<&str> = audit
        .report
        .hints
        .iter()
        .filter(|h| h.kind == "native-first")
        .map(|h| h.advice.as_str())
        .collect();
    let paid = nika_check::paid_blockers(&audit.report.hints);
    if audit.verdict.clean {
        let warnings =
            nika_cli_host::oracle::model_finding_rows(&audit.verdict.models.catalog_warnings);
        let verdict = clean_verdict(
            &native,
            &paid,
            native_strict,
            audit.verdict.grade,
            &warnings,
        )?;
        let mut text = format!("{verdict}\n{}", affirmative_contract(&audit.report));
        // W3-F2 · this oracle reads no files: a composed workflow's clean
        // answer SAYS its children went unjudged, on the prose lane too.
        if !audit.verdict.judged.composition && !audit.verdict.children.is_empty() {
            let _ = write!(
                text,
                "\n⚠ composition unjudged · {} child reference(s): {} — this oracle reads no files: audit each child by its own source, and the parent on disk with `nika check`",
                audit.verdict.children.len(),
                audit.verdict.children.join(" · ")
            );
        }
        if verbose {
            let obj = nika_cli_host::oracle::audit_json(
                &audit.wf,
                &audit.report,
                &audit.skills,
                &audit.verdict,
                lanes,
            )?;
            let detail = serde_json::to_string_pretty(&obj)
                .map_err(|e| format!("check report serialization failed: {e}"))?;
            let _ = write!(text, "\n{detail}");
        }
        return Ok(text);
    }
    Err(dirty_payload(&audit, lanes))
}

/// The DIRTY render, out of `check` under the 100-line fn cap: the FULL
/// structured report (the prior code dropped 9 finding classes) as an
/// `Err` text, so the dispatcher flags `isError: true` and the harness
/// repairs — the CLI's exit-2-on-dirty, mirrored (the `is_clean` law).
fn dirty_payload(
    audit: &nika_cli_host::oracle::Audit,
    lanes: nika_cli_host::oracle::Lanes,
) -> String {
    let mut payload = match nika_cli_host::oracle::audit_json(
        &audit.wf,
        &audit.report,
        &audit.skills,
        &audit.verdict,
        lanes,
    ) {
        Ok(obj) => obj,
        Err(e) => return format!("check report serialization failed: {e}"),
    };
    let next_actions = finding_next_actions(&payload);
    payload.insert("next_actions".to_owned(), next_actions);
    match serde_json::to_string_pretty(&payload) {
        Ok(detail) => {
            format!("✖ findings — the workflow is not clean · the full check report:\n{detail}")
        }
        Err(e) => format!("check report serialization failed: {e}"),
    }
}

/// `nika_catalog` — the versioned provider/model projection: the SAME
/// payload `nika catalog --json` emits (built by `nika-catalog::export`,
/// the one owning builder — CLI and MCP never drift).
/// `nika_inspect` — the canonical graph projection (`graph_format: 3`),
/// the SAME contract the LSP's `nika/semanticDocument` serves: the
/// projection verbatim when the ladder is clean, `{"graph": null,
/// "reason": …}` otherwise (`"findings"` — parse failures error like
/// every other tool). One projector, three protocols.
fn inspect(args: &Value) -> Result<String, String> {
    let yaml = args
        .get("workflow")
        .and_then(Value::as_str)
        .ok_or("missing `workflow` (the *.nika.yaml source)")?;
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .map_err(|e| format!("PARSE ✗ {}", e.diagnostic()))?;
    let report = nika_check::check(&wf);
    if report.is_clean() {
        serde_json::to_string_pretty(&nika_graph::project(&wf, &report))
            .map_err(|e| format!("projection serialization failed: {e}"))
    } else {
        Ok(serde_json::json!({ "graph": null, "reason": "findings" }).to_string())
    }
}

/// `nika_catalog` — the versioned provider/model projection, with the
/// resolvability fact ON every row (#1184).
///
/// This tool's whole purpose is to stop an agent guessing model ids, so
/// it is the surface where an unmarked unreachable vendor costs the
/// most: the reader is a machine that acts, not a human who shrugs. The
/// chain over `CANONICAL_IDS` is what makes `resolves` true anywhere —
/// drop it and every row reads `false`.
fn catalog_payload() -> Result<String, String> {
    let export =
        nika_catalog::export::catalog_export().with_resolvable(&nika_providers::CANONICAL_IDS);
    serde_json::to_string_pretty(&export).map_err(|e| format!("catalog projection failed: {e}"))
}

/// `nika_tools` — the versioned builtin-tool projection: the SAME payload
/// `nika tools --json` emits (built by `nika-builtin::tools_json`).
fn tools_payload() -> Result<String, String> {
    serde_json::to_string_pretty(&nika_builtin::tools_json())
        .map_err(|e| format!("tools projection failed: {e}"))
}

/// `nika_explain` — teach one error code (numeric registry or spec
/// code) or a hint identity `nika check` printed in `[brackets]`.
fn explain(args: &Value) -> Result<String, String> {
    let code = args
        .get("code")
        .and_then(Value::as_str)
        .ok_or("missing `code` (e.g. `NIKA-VAR-001`)")?;
    // Same slot as the CLI (#1038): a HINT row prints `jq-as-map` /
    // `native-first/006` where a finding prints `NIKA-PARSE-019`.
    // Resolve before wrapping `NIKA-`.
    // ADR-124 · one ladder, two doors: the host's four-rung explain
    // (a hint kind · the registry · the spec rows · the namespaces),
    // worded for THIS door — the fix an oracle-only agent can reach is
    // `nika_check` with `fix: true`, never a shell (#1270).
    let out = nika_cli_host::explain::run_for(
        code,
        nika_cli_host::Theme::new(false, true, false),
        nika_cli_host::explain::Door::Oracle,
    );
    if out.code == 0 {
        Ok(out.text)
    } else {
        Err(out.text)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
