// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The MCP tool catalog — Nika's STATIC, read-only surface exposed as Model
//! Context Protocol tools. Every tool here is PURE over its arguments — static
//! analysis (parse · check · code lookup) or embedded pack data (schema ·
//! examples · templates · canon) — zero effects, zero network, no workflow
//! ever RUNS through MCP (running needs the effect-permits boundary · out of
//! scope for the read-only server surface). That purity is what makes a tool
//! safe to expose to any connecting client (Cursor · Claude Desktop · …) and
//! lets the whole server be unit-tested as a function.
//!
//! Two tool families:
//! - **validate** (`nika_check` · `nika_explain`) — the repair oracle.
//! - **learn** (`nika_schema` · `nika_examples` · `nika_template` ·
//!   `nika_canon` · `nika_catalog` · `nika_tools`) — the authoring surface,
//!   so a wired agent follows the deterministic template→fill→check→repair
//!   protocol instead of guessing structure (the spec's §Writing-a-workflow
//!   path, reachable over MCP) and picks REAL providers/models/tools from
//!   the versioned projections instead of remembered ids.

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
        "nika_explain" => "Explain an error code",
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
                            findings, or a clean verdict — auditable before a token is spent.",
            "inputSchema": {
                "type": "object",
                "properties": {
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
            "description": "Teach one Nika error code — cause, category, and the fix form.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "A code like `NIKA-VAR-001` or the bare `440`."
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
                            canonical example instead of guessing a construct.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "slug": {
                        "type": "string",
                        "description": "An example slug from the list (e.g. `pr-risk-review`)."
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
                            from here instead of guessing.",
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
        "nika_examples" => examples(args),
        "nika_template" => template(args),
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

/// The MODELS cross-check rows for both laws (resolver refusals ·
/// catalog warnings) — one `model`/`tasks`/`why` shape on every machine
/// lane (this oracle and the CLI `--json` twin must not disagree). The
/// catalog cross-check is the two-strike class (audit UX 2026-07-31): a
/// model that RESOLVES but matches nothing the snapshot prices for its
/// provider must be relayed BEFORE the human buys a key — advisory on
/// both lanes, `clean` untouched.
fn model_crosscheck(report: &nika_check::CheckReport) -> (Vec<Value>, Vec<Value>) {
    let findings = report
        .requirements
        .models
        .iter()
        .filter_map(|m| {
            nika_providers::resolve_refusal(&m.model)
                .map(|why| serde_json::json!({ "model": m.model, "tasks": m.tasks, "why": why }))
        })
        .collect();
    let warnings = report
        .requirements
        .models
        .iter()
        .filter_map(|m| {
            nika_providers::catalog_warning(&m.model)
                .map(|why| serde_json::json!({ "model": m.model, "tasks": m.tasks, "why": why }))
        })
        .collect();
    (findings, warnings)
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

fn check(args: &Value) -> Result<String, String> {
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
    // The grade rides EVERY verdict (the CLI card's law, P0-11): « clean »
    // alone never names how much rope the file has. A pure projection of
    // the report — zero new scan.
    let grade = nika_check::risk_grade(&report);
    // The MODELS rung, MCP lane (#320 repro 3): the schema ladder alone
    // audited a hallucinated model green — cross every requirement against
    // the RESOLVER law shared with the CLI rung (nika-providers), plus its
    // sister catalog law (advisory — `clean` is untouched).
    let (model_findings, catalog_warnings) = model_crosscheck(&report);
    // The is_clean mirror law, applied to the native-first lane. `hints`
    // are NOT part of `is_clean()`, so a workflow whose real work sits in
    // `exec python3 helper.py` used to come back here as a bare "✔ clean"
    // that named nothing at all — while the same file failed
    // `nika check --native-strict` in the shell AND was refused by the
    // hook in front of `nika run`. That is the false green the operator
    // met in Cursor: the agent consults THIS oracle, reads clean, and
    // hands over a file that cannot run.
    //
    // Strict is the DEFAULT here (unlike the CLI, where the bare verb is
    // the human's advisory read). This tool is the agent-facing oracle,
    // and an oracle laxer than the gate it feeds is worse than none.
    let native_strict = args
        .get("native_strict")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let native: Vec<&str> = report
        .hints
        .iter()
        .filter(|h| h.kind == "native-first")
        .map(|h| h.advice.as_str())
        .collect();
    let paid = nika_check::paid_blockers(&report.hints);
    if report.is_clean() && model_findings.is_empty() {
        return clean_verdict(&native, &paid, native_strict, grade, &catalog_warnings);
    }
    // `is_clean()` checks TEN finding surfaces (conformance · secret leaks +
    // egresses · capability escapes · schema findings + lints · unknown/missing
    // /unknown args · gate findings) — render the FULL structured report so the
    // MCP client sees EVERY finding (the prior code listed only `conformance`,
    // dropping 9 classes · a model can parse the JSON + repair from it).
    let mut payload = serde_json::to_value(&report)
        .map_err(|e| format!("check report serialization failed: {e}"))?;
    if let Some(obj) = payload.as_object_mut() {
        // The same keys the CLI --json lane carries — the two machine
        // lanes must not disagree (the is_clean mirror law).
        obj.insert(
            "models_resolve".to_owned(),
            Value::Bool(model_findings.is_empty()),
        );
        // The same key the CLI `--json` lane stamps (lowercase wire word)
        // — the two machine lanes must not disagree on the one verdict.
        obj.insert(
            "risk_grade".to_owned(),
            Value::String(grade.as_str().to_owned()),
        );
        if !model_findings.is_empty() {
            obj.insert("model_findings".to_owned(), Value::Array(model_findings));
        }
        // Presence-gated like the CLI twin (`models_catalog_warnings`) —
        // the same key on both machine surfaces, one voice.
        if !catalog_warnings.is_empty() {
            obj.insert(
                "models_catalog_warnings".to_owned(),
                Value::Array(catalog_warnings),
            );
        }
        nika_check::stamp_paid_ready(obj, &report.hints);
    }
    let detail = serde_json::to_string_pretty(&payload)
        .map_err(|e| format!("check report serialization failed: {e}"))?;
    // A DIRTY workflow is an `Err` so the dispatcher flags `isError: true`:
    // the model then SEES the findings AND the harness triggers its repair
    // loop (the authoring protocol is template→fill→check→REPAIR). This
    // mirrors the CLI's exit-2-on-dirty — the two machine lanes must not
    // disagree (a `nika check` that fails the shell must not read as
    // success over MCP). The full report rides the Err text unchanged, so
    // the model repairs from the same JSON either way (the user-sim
    // finding · the is_clean mirror law applied to the MCP lane).
    Err(format!(
        "✖ findings — the workflow is not clean · the full check report:\n{detail}"
    ))
}

/// `nika_examples` — return the JSONL metadata index (`slug` · `form` ·
/// `one_line` · `cost`), or one example's full workflow source (the LEARNING
/// surface: read the canonical example for a construct instead of guessing).
fn examples(args: &Value) -> Result<String, String> {
    match args.get("slug").and_then(Value::as_str) {
        None => example_index(),
        Some(slug) => match nika_pack::example(slug) {
            Some(body) => Ok(body.to_owned()),
            // RAMS-11: a PLAIN-WORDS miss routes through the SAME door
            // the CLI walks (`nika new <words>` · whole catalog) — one
            // router, one calibration. Only multi-word queries route:
            // a single token is a slug (typo'd or adversarial) and the
            // unknown-key contract holds — the router never sees the
            // traversal-shaped garbage the adversarial pin feeds.
            None if !slug.trim().contains(' ') => Err(format!(
                "unknown example `{slug}` — call nika_examples without arguments \
                 for the list"
            )),
            None => match nika_onboard::routing::route_query(slug) {
                nika_onboard::routing::RoutedEntry::Example(name) => {
                    let body = nika_pack::example(&name).unwrap_or_default();
                    Ok(format!("# routed: `{slug}` → example `{name}`\n{body}"))
                }
                nika_onboard::routing::RoutedEntry::Skeleton(name) => {
                    let body = nika_pack::template(&name).unwrap_or_default();
                    Ok(format!("# routed: `{slug}` → template `{name}`\n{body}"))
                }
                nika_onboard::routing::RoutedEntry::Clarify(candidates) => Err(format!(
                    "`{slug}` doesn't route confidently — closest: {} · call \
                     nika_examples without arguments for the list",
                    candidates.join(" · ")
                )),
            },
        },
    }
}

/// The retrieval-friendly example index: one JSON object per embedded file.
/// Every field derives from the same source the CLI showroom reads — no second
/// catalog to drift. Task count is the honest static cost available in-pack.
fn example_index() -> Result<String, String> {
    nika_pack::example_slugs()
        .into_iter()
        .map(|slug| {
            let body = nika_pack::example(&slug).ok_or_else(|| {
                format!("embedded example `{slug}` is listed but cannot be resolved")
            })?;
            let meta = nika_pack::meta(&slug, body);
            Ok(json!({
                "slug": slug,
                "form": meta.verbs,
                "one_line": meta.title,
                "cost": { "tasks": meta.tasks },
            })
            .to_string())
        })
        .collect::<Result<Vec<_>, String>>()
        .map(|rows| rows.join("\n"))
}

/// `nika_template` — list the canonical skeleton names, or return one
/// skeleton's source (copy · fill the `# SLOT:` lines · never invent shape).
fn template(args: &Value) -> Result<String, String> {
    match args.get("name").and_then(Value::as_str) {
        None => Ok(nika_pack::template_names().join("\n")),
        Some(name) => match nika_pack::template(name) {
            Some(body) => Ok(body.to_owned()),
            // RAMS-11: a PLAIN-WORDS miss routes within the SKELETON set
            // — the CLI wizard's door (SLOTs to fill). Single tokens keep
            // the unknown-key contract (see nika_examples).
            None if !name.trim().contains(' ') => Err(format!(
                "unknown template `{name}` — call nika_template without arguments \
                 for the list"
            )),
            None => match nika_onboard::routing::route_skeleton_query(name) {
                nika_onboard::routing::RoutedEntry::Skeleton(routed) => {
                    let body = nika_pack::template(&routed).unwrap_or_default();
                    Ok(format!("# routed: `{name}` → template `{routed}`\n{body}"))
                }
                nika_onboard::routing::RoutedEntry::Example(routed) => {
                    let body = nika_pack::example(&routed).unwrap_or_default();
                    Ok(format!("# routed: `{name}` → example `{routed}`\n{body}"))
                }
                nika_onboard::routing::RoutedEntry::Clarify(candidates) => Err(format!(
                    "`{name}` doesn't route confidently — closest: {} · call \
                     nika_template without arguments for the list",
                    candidates.join(" · ")
                )),
            },
        },
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

fn catalog_payload() -> Result<String, String> {
    serde_json::to_string_pretty(&nika_catalog::export::catalog_export())
        .map_err(|e| format!("catalog projection failed: {e}"))
}

/// `nika_tools` — the versioned builtin-tool projection: the SAME payload
/// `nika tools --json` emits (built by `nika-builtin::tools_json`).
fn tools_payload() -> Result<String, String> {
    serde_json::to_string_pretty(&nika_builtin::tools_json())
        .map_err(|e| format!("tools projection failed: {e}"))
}

/// `nika_explain` — teach one error code (numeric registry or spec code).
fn explain(args: &Value) -> Result<String, String> {
    let code = args
        .get("code")
        .and_then(Value::as_str)
        .ok_or("missing `code` (e.g. `NIKA-VAR-001`)")?;
    let normalized = if code.starts_with("NIKA-") {
        code.to_owned()
    } else {
        format!("NIKA-{code}")
    };
    // 1 · the numeric crate registry (`NIKA-440` · engine codes · code_help).
    if let Some(c) = nika_error::codes::lookup(&normalized) {
        return Ok(nika_error::codes::code_help(c).to_owned());
    }
    // 2 · the spec conformance codes (`NIKA-VAR-*` · `NIKA-DAG-*` · what `nika
    //     check` emits) from the embedded canon — the SSOT row, no network.
    if let Some(row) = nika_pack::error_codes()
        .into_iter()
        .find(|r| r.code == normalized)
    {
        // The contract lesson rides here too (one voice with the CLI's
        // canon row · gauntlet 2026-07-31: the CLI taught the SEC-004
        // grant grammar while this tool answered a category and a URL).
        let lesson = nika_error::codes::spec_contract_help(&normalized)
            .map(|l| format!("\n\n{l}"))
            .unwrap_or_default();
        return Ok(format!(
            "{normalized} · {} · transient: {}\n\n  {}{lesson}",
            row.category, row.transient, row.failure
        ));
    }
    // 3 · the runtime namespaces (per-builtin `NIKA-BUILTIN-<NAME>-<NNN>` ·
    //     per-provider `NIKA-PROVIDER-<NNN>`) — valid in `on_codes:` and
    //     emitted by failed runs, so the agent debugging a trace over MCP
    //     gets the SAME teaching the CLI gives (one voice · shared text in
    //     `nika-error::codes`). Gauntlet 2026-07-12: the CLI taught
    //     NIKA-BUILTIN-PROMPT-001 while this tool said "unknown code".
    if let Some(text) = nika_error::codes::namespace_help(&normalized, "docs.nika.sh/errors") {
        return Ok(text);
    }
    Err(format!(
        "unknown code `{code}` — the registry knows NIKA-001..9999, the spec \
         codes (NIKA-VAR-* · NIKA-DAG-* · …), per-builtin \
         NIKA-BUILTIN-<NAME>-NNN and per-provider NIKA-PROVIDER-NNN codes; \
         see docs.nika.sh/errors"
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Every tool the oracle serves declares its behaviour, and names
    /// itself. The hints are what a client reads to stop interrupting a
    /// human on a read-only call; a blank title is a tool that never
    /// introduced itself. Found 2026-07-28: all nine served NONE, so a
    /// pure-read oracle felt exactly as dangerous as a shell.
    #[test]
    fn every_served_tool_declares_read_only_behaviour_and_a_title() {
        let catalog = catalog();
        let tools = catalog.as_array().expect("the catalog is an array");
        assert!(!tools.is_empty(), "the oracle serves no tools at all");
        for tool in tools {
            let name = tool["name"].as_str().expect("every tool has a name");
            assert!(
                !tool["title"].as_str().unwrap_or_default().is_empty(),
                "{name} serves no display title — add it to display_title()"
            );
            let hints = &tool["annotations"];
            assert_eq!(hints["readOnlyHint"], json!(true), "{name} readOnlyHint");
            assert_eq!(hints["destructiveHint"], json!(false), "{name} destructive");
            assert_eq!(hints["idempotentHint"], json!(true), "{name} idempotent");
            assert_eq!(hints["openWorldHint"], json!(false), "{name} openWorld");
        }
    }

    /// `nika_inspect` serves the projection VERBATIM — byte-equal with
    /// `nika_graph::project` on the same source (one projector, three
    /// protocols: this pin is the MCP leg of the LSP's parity law).
    #[test]
    fn inspect_serves_the_canonical_projection_verbatim() {
        let yaml = "nika: w\npermits: { exec: [\"true\"] }\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n  b:\n    after:\n      a: success\n    exec: { command: [\"true\"] }\n";
        let out = inspect(&serde_json::json!({ "workflow": yaml })).expect("clean");
        let got: Value = serde_json::from_str(&out).expect("json");
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses");
        let report = nika_check::check(&wf);
        let expected = serde_json::to_value(nika_graph::project(&wf, &report)).expect("serializes");
        assert_eq!(got, expected);
        assert_eq!(got["graph_format"], 3, "in-payload version");
    }

    /// Findings → null graph + the one-word reason (the LSP contract,
    /// MCP leg) — never a projection of an unproven DAG.
    #[test]
    fn inspect_refuses_findings_with_a_reason() {
        let yaml = "nika: w\ntasks:\n  a:\n    after:\n      b: success\n    exec: { command: [\"true\"] }\n  b:\n    after:\n      a: success\n    exec: { command: [\"true\"] }\n";
        let out = inspect(&serde_json::json!({ "workflow": yaml })).expect("answers");
        let got: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(got["graph"], Value::Null);
        assert_eq!(got["reason"], "findings");
    }

    /// #320 repro 3, closed: a hallucinated model over the MCP lane was
    /// the LAST surface still auditing green — the rung now reds it with
    /// the same payload keys the CLI --json lane carries.
    #[test]
    fn check_reds_a_bare_model_id_with_the_shared_rung() {
        let yaml = "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"gpt-5-turbo\" }\n";
        let err = check(&serde_json::json!({ "workflow": yaml })).expect_err("dirty");
        assert!(err.contains("\"models_resolve\": false"), "{err}");
        assert!(
            err.contains("gpt-5-turbo") && err.contains("bare model id"),
            "{err}"
        );
    }

    #[test]
    fn check_reds_a_cataloged_but_unresolvable_provider() {
        let yaml = "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"azure/gpt-4o\" }\n";
        let err = check(&serde_json::json!({ "workflow": yaml })).expect_err("dirty");
        assert!(
            err.contains("`azure` does not resolve") || err.contains("provider `azure`"),
            "{err}"
        );
    }

    #[test]
    fn check_stays_clean_when_every_model_resolves() {
        let yaml = "nika: m\ntasks:\n  think:\n    infer: { prompt: hi, max_tokens: 10, model: \"mock/echo\" }\n";
        let ok = check(&serde_json::json!({ "workflow": yaml })).expect("clean");
        assert!(ok.contains("clean"), "{ok}");
    }

    #[test]
    fn catalog_lists_the_validate_and_learn_tools() {
        let c = catalog();
        let names: Vec<&str> = c
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(
            names,
            [
                "nika_check",
                "nika_inspect",
                "nika_explain",
                "nika_schema",
                "nika_examples",
                "nika_template",
                "nika_canon",
                "nika_catalog",
                "nika_tools"
            ]
        );
        // Each tool carries a JSON-Schema inputSchema (the client validates args).
        for t in c.as_array().expect("array") {
            assert_eq!(t["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn catalog_and_tools_payloads_are_the_versioned_wire_json() {
        let out = execute("nika_catalog", &json!({})).expect("nika_catalog runs");
        let value: Value = serde_json::from_str(&out).expect("nika_catalog emits JSON");
        assert_eq!(value["catalog_version"], 1, "the locked v1 wire marker");
        assert!(
            !value["providers"].as_array().expect("providers").is_empty(),
            "the embedded catalog is never empty",
        );

        let out = execute("nika_tools", &json!({})).expect("nika_tools runs");
        let value: Value = serde_json::from_str(&out).expect("nika_tools emits JSON");
        assert_eq!(value["tools_version"], 1, "the locked v1 wire marker");
        assert!(
            !value["tools"].as_array().expect("tools").is_empty(),
            "the embedded builtin set is never empty",
        );
    }

    #[test]
    fn check_a_clean_workflow_is_ok() {
        let wf = "nika: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let out = execute("nika_check", &json!({ "workflow": wf })).expect("ran");
        assert!(out.contains("clean"), "{out}");
    }

    /// The workflow that leaves the native path: every one of the TEN
    /// finding surfaces is clean, and it still must not read as green.
    ///
    /// This is the false green the operator met in Cursor. An agent
    /// consults THIS oracle before handing a file over; it used to get a
    /// bare "✔ clean" that named nothing, while the identical file
    /// failed `nika check --native-strict` in the shell and was refused
    /// by the hook in front of `nika run`. The `is_clean` mirror law — a
    /// check that fails the shell must not read as success over MCP —
    /// is what this pins. (The fixture declares its `net.http`: post-D1
    /// the exec URL is a net USE — undeclared it would be a PERMITS
    /// escape, and this test row is about the hint gate, not the escape.)
    const LEAVES_THE_NATIVE_PATH: &str = "nika: t\npermits: { exec: [\"curl\"], net: { http: [\"acme.test\"] } }\ntasks:\n  grab:\n    exec: { command: [\"curl\", \"-s\", \"https://acme.test\"] }\n";

    #[test]
    fn check_is_strict_about_paid_ready_by_default() {
        let wf = "nika: t\nmodel: mock/echo\ntasks:\n  judge:\n    infer:\n      prompt: |\n        Read the note and assign a belt.\n      max_tokens: 32\noutputs:\n  r: ${{ tasks.judge.output }}\n";
        let err = execute("nika_check", &json!({ "workflow": wf }))
            .expect_err("an infer that names the law is not a green by default");
        assert!(err.contains("paid-ready"), "{err}");
        assert!(err.contains("infer-as-law"), "{err}");
        assert!(
            err.contains("13-extract-then-law"),
            "the refusal must name the one-way: {err}"
        );
    }

    #[test]
    fn check_is_strict_about_the_native_path_by_default() {
        let err = execute("nika_check", &json!({ "workflow": LEAVES_THE_NATIVE_PATH }))
            .expect_err("an exec a builtin covers is not a green by default");
        assert!(err.contains("native-first"), "{err}");
        assert!(
            err.contains("nika:fetch"),
            "the refusal must name the builtin that replaces it: {err}"
        );
    }

    #[test]
    fn advisory_mode_still_names_what_it_did_not_enforce() {
        // Opting out must not hand back the SAME sentence a genuinely
        // clean workflow gets — that would just relocate the false
        // green behind a flag.
        let out = execute(
            "nika_check",
            &json!({ "workflow": LEAVES_THE_NATIVE_PATH, "native_strict": false }),
        )
        .expect("advisory mode returns a verdict");
        assert!(out.contains("advisory"), "{out}");
        assert!(out.contains("native-first"), "{out}");
        assert!(
            out.contains("--native-strict"),
            "advisory mode must say which posture WOULD refuse it: {out}"
        );
    }

    #[test]
    fn the_strict_flag_is_declared_on_the_tool_that_honours_it() {
        let listed = catalog();
        let tools = listed.as_array().expect("a tool array");
        let check_tool = tools
            .iter()
            .find(|t| t["name"] == "nika_check")
            .expect("nika_check is served");
        let strict = &check_tool["inputSchema"]["properties"]["native_strict"];
        assert_eq!(
            strict["default"],
            json!(true),
            "the agent-facing oracle defaults to the posture its run gate uses"
        );
    }

    #[test]
    fn check_a_broken_workflow_is_an_error_carrying_the_findings() {
        // A dangling `after:` edge — a DAG finding the ladder catches.
        // Dirty is an `Err` (→ isError:true) so a wired agent's repair
        // loop triggers, mirroring the CLI's exit-2-on-dirty; the full
        // report still rides the text so the model repairs from it.
        let wf = "nika: t\ntasks:\n  a:\n    after:\n      ghost: success\n    exec: { command: [\"x\"] }\n";
        let err = execute("nika_check", &json!({ "workflow": wf })).expect_err("dirty is an error");
        assert!(err.contains("findings") && err.contains("NIKA-"), "{err}");
    }

    #[test]
    fn check_missing_arg_is_a_tool_error() {
        assert!(execute("nika_check", &json!({})).is_err());
    }

    #[test]
    fn check_surfaces_non_conformance_findings_too() {
        // A schema whose `required` key is absent from `properties` COMPILES
        // (legal JSON Schema · no PARSE-019) but is a satisfiability smell →
        // it lands in `schema_lints`, NOT `conformance`. The old code rendered
        // only `conformance` → an empty "✖ findings" body for this whole class
        // (the P1). The full-report render must surface it.
        let wf = "nika: t\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: x\n      max_tokens: 10\n      schema: { type: object, properties: { a: { type: string } }, required: [b] }\n";
        let out = execute("nika_check", &json!({ "workflow": wf })).expect_err("dirty is an error");
        assert!(out.contains("findings"), "flags not-clean: {out}");
        assert!(
            out.contains("schema_lints") && out.contains('b'),
            "the non-conformance finding (required `b` not in properties) is rendered, \
             not dropped: {out}"
        );
    }

    /// P0-6's second answer rides the MCP verdict too: the CLI card shows
    /// the risk grade on EVERY audited card — a bare « ✔ clean » that
    /// never names the rope is the false-green shape this oracle exists to
    /// kill (a declared effect is Supervised, and the agent reading this
    /// verdict must see it).
    #[test]
    fn the_clean_verdict_names_the_risk_grade() {
        let wf = "nika: t\npermits: { exec: [\"echo\"] }\ntasks:\n  a:\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let out = execute("nika_check", &json!({ "workflow": wf })).expect("ran");
        assert!(out.contains("clean"), "{out}");
        assert!(
            out.contains("risk supervised"),
            "the grade rides the clean verdict: {out}"
        );
    }

    /// The dirty lane IS the machine contract (a model repairs from this
    /// JSON) — it must carry the same `risk_grade` key the CLI `--json`
    /// verdict stamps (lowercase, like `check --json`), or the two machine
    /// lanes disagree about the one verdict.
    #[test]
    fn the_findings_payload_carries_the_risk_grade_like_the_cli_json_lane() {
        // A dangling `after:` edge — dirty, no grants, nothing uncapped.
        let wf = "nika: t\ntasks:\n  a:\n    after:\n      ghost: success\n    exec: { command: [\"x\"] }\n";
        let err = execute("nika_check", &json!({ "workflow": wf })).expect_err("dirty is an error");
        let json_start = err.find('{').expect("the report rides the error as JSON");
        let payload: Value =
            serde_json::from_str(&err[json_start..]).expect("the report is valid JSON");
        assert_eq!(
            payload["risk_grade"],
            json!("low"),
            "lowercase, like the CLI lane: {payload:#}"
        );
    }

    /// The field tracks the REPORT, it is not a stamped constant: an
    /// agent loop without `max_tokens_total` (the audit's P0-6 fixture)
    /// grades Unbounded even behind a findings verdict.
    #[test]
    fn the_findings_payloads_grade_reflects_the_report() {
        let wf = "nika: t\nmodel: anthropic/claude-sonnet-4-6\npermits: { tools: [\"nika:read\"] }\ntasks:\n  a:\n    agent: { prompt: go, tools: [\"nika:read\"], max_turns: 100 }\n    after: { ghost: success }\n";
        let err = execute("nika_check", &json!({ "workflow": wf })).expect_err("dirty is an error");
        let json_start = err.find('{').expect("the report rides the error as JSON");
        let payload: Value =
            serde_json::from_str(&err[json_start..]).expect("the report is valid JSON");
        assert_eq!(
            payload["risk_grade"],
            json!("unbounded"),
            "max_turns bounds turns, never tokens: {payload:#}"
        );
    }

    #[test]
    fn explain_a_known_code_teaches_it() {
        let out = execute("nika_explain", &json!({ "code": "NIKA-VAR-001" })).expect("ran");
        assert!(!out.is_empty());
        let bare = execute("nika_explain", &json!({ "code": "VAR-001" })).expect("ran");
        assert_eq!(out, bare, "the bare form normalizes to the same code");
    }

    #[test]
    fn explain_an_unknown_code_is_a_tool_error() {
        assert!(execute("nika_explain", &json!({ "code": "NIKA-GHOST-999" })).is_err());
    }

    /// One voice with the CLI (gauntlet 2026-07-12): a failed run's
    /// per-builtin / per-provider code must TEACH over MCP too — the
    /// agent debugging a trace calls this tool, not the terminal.
    /// PROMPT-001 carries the full contract lesson (first-run gate ·
    /// 2026-07-31): the agent reads the same exits the CLI teaches.
    #[test]
    fn explain_teaches_the_runtime_namespaces_like_the_cli() {
        let b = execute(
            "nika_explain",
            &json!({ "code": "NIKA-BUILTIN-PROMPT-001" }),
        )
        .expect("builtin namespace teaches");
        assert!(b.contains("the `nika:prompt` contract") && b.contains("on_codes"));
        assert!(
            b.contains("--answer <task>=<value>") && b.contains("`default:`"),
            "the contract lesson rides MCP too: {b}"
        );
        // A builtin WITHOUT a contract entry keeps the namespace voice.
        let generic = execute("nika_explain", &json!({ "code": "NIKA-BUILTIN-JQ-001" }))
            .expect("namespace teaches");
        assert!(
            generic.contains("per-builtin runtime diagnostic"),
            "{generic}"
        );
        let p = execute("nika_explain", &json!({ "code": "NIKA-PROVIDER-007" }))
            .expect("provider namespace teaches");
        assert!(p.contains("provider-adapter"));
    }

    #[test]
    fn unknown_tool_is_an_error() {
        assert!(execute("nika_nonexistent", &json!({})).is_err());
    }

    // ── the LEARNING surface (agents can learn, not just validate) ──────

    #[test]
    fn catalog_lists_the_learning_tools_too() {
        let c = catalog();
        let names: Vec<&str> = c
            .as_array()
            .expect("array")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        for expected in [
            "nika_check",
            "nika_inspect",
            "nika_explain",
            "nika_schema",
            "nika_examples",
            "nika_template",
            "nika_canon",
        ] {
            assert!(names.contains(&expected), "{expected} missing: {names:?}");
        }
    }

    #[test]
    fn schema_returns_the_embedded_json_schema() {
        let out = execute("nika_schema", &json!({})).expect("ran");
        // The real schema: parses as JSON and declares the canonical $id.
        let v: Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(
            v["$id"], "https://nika.sh/spec/v1/workflow.schema.json",
            "the canonical $id travels with the schema"
        );
    }

    #[test]
    fn examples_without_slug_returns_a_metadata_derived_index() {
        let out = execute("nika_examples", &json!({})).expect("ran");
        let slugs = nika_pack::example_slugs();
        let rows: Vec<Value> = out
            .lines()
            .map(|line| serde_json::from_str(line).expect("each index row is JSON"))
            .collect();
        assert_eq!(rows.len(), slugs.len(), "one row per embedded example");
        assert!(!rows.is_empty(), "the pack ships examples");

        for (row, slug) in rows.iter().zip(&slugs) {
            let body = nika_pack::example(slug).expect("a listed slug resolves");
            let meta = nika_pack::meta(slug, body);
            assert_eq!(row["slug"], slug.as_str(), "slug derives from the pack");
            assert_eq!(row["form"], json!(meta.verbs), "form derives from verbs");
            assert_eq!(
                row["one_line"], meta.title,
                "one line derives from the header"
            );
            assert_eq!(
                row["cost"],
                json!({ "tasks": meta.tasks }),
                "cost derives from the task count"
            );
            assert_eq!(
                row.as_object().expect("row object").len(),
                4,
                "the index contract is exactly slug · form · one_line · cost"
            );
        }
    }

    #[test]
    fn examples_with_slug_returns_the_source() {
        let slug = nika_pack::example_slugs()
            .first()
            .cloned()
            .expect("pack has examples");
        let out = execute("nika_examples", &json!({ "slug": slug })).expect("ran");
        assert!(
            out.contains("nika: hello"),
            "a real workflow source (nine-key identity): {out}"
        );
    }

    #[test]
    fn examples_with_unknown_slug_is_a_tool_error_naming_the_list() {
        let err = execute("nika_examples", &json!({ "slug": "no-such-example" }))
            .expect_err("unknown slug");
        assert!(err.contains("unknown example"), "{err}");
    }

    /// RAMS-11: the oracle walks the CLI's routing door on plain words —
    /// the SAME query `nika new` routes lands the SAME entry here, and
    /// the interpretation is SAID in a leading YAML comment.
    #[test]
    fn examples_route_plain_words_through_the_one_door() {
        let out =
            execute("nika_examples", &json!({ "slug": "chase unpaid invoices" })).expect("routes");
        assert!(
            out.starts_with("# routed: `chase unpaid invoices` → example `invoice-chaser`"),
            "the routing is said: {out}"
        );
        assert!(out.contains("nika: "), "the body follows: {out}");
    }

    /// RAMS-11, the honest floor: plain words below the confidence bar
    /// clarify with the closest names — never a silent guess.
    #[test]
    fn examples_clarify_below_the_bar_instead_of_guessing() {
        let err = execute("nika_examples", &json!({ "slug": "do stuff with things" }))
            .expect_err("vague words clarify");
        assert!(err.contains("doesn't route confidently"), "{err}");
        assert!(err.contains("·"), "names candidates: {err}");
    }

    /// The slug/name is a KEY into the compile-time embedded pack
    /// (`nika_pack` · `include_dir!`), never a filesystem path — so path
    /// traversal, absolute paths, null bytes and injection are structurally
    /// impossible to turn into a read, not merely defended. This guards that
    /// invariant: any refactor that makes `example()`/`template()` touch the
    /// fs from the argument fails here. (Backed by the 2026-07-03 adversarial
    /// MCP e2e: 10/10 abusive slugs → clean errors, 0 leaks.)
    #[test]
    fn examples_and_templates_reject_adversarial_keys_as_plain_lookups() {
        let evil = [
            "../../../etc/passwd",
            "/etc/passwd",
            "%2e%2e%2f",
            "01-hello/../../secret",
            "inject\n\rion",
            "01-hello\0x",
        ];
        for key in evil {
            let e = execute("nika_examples", &json!({ "slug": key }))
                .expect_err("adversarial slug must be an unknown-key error");
            assert!(
                e.contains("unknown example") && !e.contains("root:"),
                "traversal leaked or crashed for {key:?}: {e}"
            );
            let t = execute("nika_template", &json!({ "name": key }))
                .expect_err("adversarial template name must be an unknown-key error");
            assert!(t.contains("unknown template"), "for {key:?}: {t}");
        }
    }

    #[test]
    fn template_without_name_lists_the_skeletons() {
        let out = execute("nika_template", &json!({})).expect("ran");
        assert!(out.contains("chain"), "the chain skeleton is listed: {out}");
    }

    #[test]
    fn template_with_name_returns_the_skeleton() {
        let out = execute("nika_template", &json!({ "name": "chain" })).expect("ran");
        assert!(
            out.contains("nika: ") && out.contains("SLOT"),
            "a fillable skeleton with SLOT markers: {out}"
        );
    }

    #[test]
    fn canon_returns_the_ssot() {
        let out = execute("nika_canon", &json!({})).expect("ran");
        assert!(
            out.contains("verbs") && out.contains("builtins"),
            "the canon SSOT covers verbs + builtins: {out}"
        );
    }
}
