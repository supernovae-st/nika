// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Dataflow schema typing — `${{ tasks.A.output.field }}` type-checked
//! against A's declared shape (ADR-092 #4).
//!
//! A task that declares a `schema:` (infer/agent structured output) or
//! `output:` bindings (jq) has a KNOWN output address space. Every deep
//! reference into `tasks.<id>.output.<path…>` anywhere in the workflow
//! (prompts · commands · args · `when:` · `with:` · `for_each` ·
//! envelope `outputs:` · `on_finally` cleanups) is resolved against that
//! space — a typo'd field name is caught BEFORE a single token is spent,
//! transitively across the DAG. No Turing-complete engine can do this;
//! ours can because outputs carry declared shapes and references are a
//! closed CEL subset.
//!
//! Sound-by-honesty: a shape that goes statically opaque (`$ref` ·
//! explicit `additionalProperties: true` · an un-schema'd task · a jq
//! binding's inner structure) resolves to "unknown — no finding", never
//! a guess. Findings are only emitted where the declared shape PROVES
//! the path cannot exist.

use serde_json::Value;
use std::collections::BTreeMap;

use nika_types::types::{NikaType, assignable, parse_type};

use nika_schema::expression::{Expr, scan_templates, task_output_paths, with_alias_paths};
use nika_schema::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use nika_schema::types::{VarDecl, type_expr_display};

use nika_types::suggest::{did_you_mean, suggestion_clause};

/// A deep output reference the declared shape proves invalid.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct SchemaTypeFinding {
    /// Where the reference appears (task id · `<id> (on_finally)` ·
    /// `outputs`).
    pub site: String,
    /// The reference, rendered (`tasks.a.output.summray`).
    pub reference: String,
    /// The task whose declared shape rejects it.
    pub target: String,
    /// Why — the failing segment + the keys that DO exist there.
    pub detail: String,
}

/// A deep `tasks.<id>.output.<path…>` reference the lane CANNOT judge
/// (F3 · 2026-07-30): the target task exists but declares NO output
/// shape — a builtin invoke without `returns:`, an exec without
/// `output:` bindings. Not a finding (the soundness law stands: no proof
/// either way, no finding) — but an ✔ that stays silent about them reads
/// as universal while the run dies on a missing key, so the verdict line
/// counts them and names its own blind spot (the F7 narrowing pattern:
/// the line claims exactly what it covers).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct UnverifiableOutputRef {
    /// Where the reference appears (task id · `<id> (on_finally)` ·
    /// `outputs`).
    pub site: String,
    /// The reference, rendered (`tasks.inspect.output.total_usd`).
    pub reference: String,
}

/// One task's declared output address space.
enum Shape<'a> {
    /// `schema:` on an infer/agent task — a JSON Schema to descend.
    Schema(&'a Value),
    /// `output:` jq bindings — the address space is the binding names
    /// (their inner structure is jq-shaped, statically opaque).
    Bindings(Vec<&'a str>),
}

/// Scan every expression island in the workflow and type-check deep
/// `tasks.<id>.output.<path…>` references against declared shapes.
/// Returns the findings AND the unverifiable refs (F3): a deep ref whose
/// target task exists but declares no output shape is no finding — the
/// verdict line counts it instead of reading as universal.
#[must_use]
pub(super) fn scan_types(wf: &RawWorkflow) -> (Vec<SchemaTypeFinding>, Vec<UnverifiableOutputRef>) {
    let mut findings = Vec::new();
    let mut unverifiable = Vec::new();
    // for_each source typing reads VAR declarations, not output shapes —
    // it must run even when no task carries a `schema:`/`output:`.
    scan_for_each_sources(wf, &mut findings);
    // `returns:` contracts walk through the SAME door as `schema:` —
    // lowered once (spec 09 §lowering · one direction), owned here so
    // the shapes map can borrow either surface.
    let lowered = crate::analyzer::lowered_returns(wf);
    let shapes = declared_shapes(wf, &lowered);
    // NO early return when `shapes` is empty: with zero declared shapes
    // every deep ref into an existing task is unverifiable — the F3 case
    // itself — and counting them is now this lane's duty.
    let all_tasks: std::collections::BTreeSet<&str> =
        wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        for text in task_texts(&task.value) {
            check_text(
                id,
                text,
                &shapes,
                &all_tasks,
                &mut findings,
                &mut unverifiable,
            );
        }
        // A `with:` alias bound to a SHAPELESS task's whole output
        // carries the same blind spot one hop further (F3's own repro
        // reads `with.bill.total_usd`, not `tasks.bill.output.total_usd`).
        let aliases = shapeless_with_aliases(&task.value, &shapes, &all_tasks);
        if !aliases.is_empty() {
            for text in task_texts(&task.value) {
                check_alias_text(id, text, &aliases, &mut unverifiable);
            }
        }
    }
    for (_, decl) in &wf.outputs {
        check_text(
            "outputs",
            &decl.value().value,
            &shapes,
            &all_tasks,
            &mut findings,
            &mut unverifiable,
        );
    }
    (findings, unverifiable)
}

/// A `for_each:` source that is a BARE `${{ inputs.X }}`/`${{ inputs.X }}`/
/// `${{ const.X }}` whose declaration is a non-array type can never be an
/// array — the runtime refuses it (NIKA-VAR-006 « `for_each` collection
/// must be an array ») and the check must catch that BEFORE the run
/// (audit-before-run), or a `for_each: ${{ inputs.locales }}` with
/// `locales: { type: string }` audits clean then dies at dispatch. Scoped
/// for zero false positives:
/// ONLY a bare `<authority>.X` reference (inputs · config · const) to a
/// `Typed` non-array declaration — an untyped entry (a `--var` override
/// could pass an array), a `tasks.*` source, or any transformed
/// expression (`split()` etc.) is left alone. Post-R3b the judgment
/// rides the one type core (`assignable` answers « admits an array » ·
/// a broken expression skips, its refusal is the analyzer's).
fn scan_for_each_sources(wf: &RawWorkflow, findings: &mut Vec<SchemaTypeFinding>) {
    let named = std::collections::BTreeMap::new();
    let type_names = std::collections::BTreeSet::new();
    for task in &wf.tasks {
        let Some(fe) = &task.value.for_each else {
            continue;
        };
        let ForEachValue::Expression(src) = &fe.value else {
            continue; // an inline list literal is already an array
        };
        let Some((authority, name)) = bare_authority_reference(src) else {
            continue; // not a plain authority ref — could resolve to an array
        };
        let block = match authority.as_str() {
            "inputs" => &wf.inputs,
            _ => &wf.consts,
        };
        let Some((_, decl)) = block.iter().find(|(n, _)| n.value == name) else {
            continue;
        };
        // An UNTYPED entry is legal in `const:` alone (the parser refuses
        // one for `inputs:`), and spec 01 §const is normative: a constant
        // is « immutable across the run and never caller-supplied ». The
        // `--var` override that spares an untyped input therefore cannot
        // reach it — the literal IS the run value, so a non-array can
        // never become an array. Without this arm the run refuses what
        // the check just cleared (NIKA-VAR-006 at dispatch), which is the
        // linter-not-verifier gap ADR-092 exists to close.
        if let VarDecl::Untyped(literal) = decl
            && authority == "const"
            && !literal.is_array()
        {
            findings.push(SchemaTypeFinding {
                site: task.value.id.value.clone(),
                reference: format!("for_each: {{ items: \"${{{{ {authority}.{name} }}}}\" }}"),
                target: format!("{authority}.{name}"),
                detail: format!(
                    "`const.{name}` is {} — `for_each` needs an array (a constant is \
                     never caller-supplied · the run rejects it · NIKA-VAR-006)",
                    crate::schema_lint::kind(literal)
                ),
            });
            continue;
        }
        let VarDecl::Typed { r#type, .. } = decl else {
            continue;
        };
        let Ok(declared_type) = parse_type(&r#type.value, &type_names, &name) else {
            continue; // the analyzer's grammar arm owns this refusal
        };
        let any_array = NikaType::Array(Box::new(NikaType::Unknown));
        if !assignable(&any_array, &declared_type, &named) {
            findings.push(SchemaTypeFinding {
                site: task.value.id.value.clone(),
                reference: format!("for_each: {{ items: \"${{{{ {authority}.{name} }}}}\" }}"),
                target: format!("{authority}.{name}"),
                detail: format!(
                    "`{authority}.{name}` is declared `type: {}` — `for_each` needs an array \
                     (the run rejects it · NIKA-VAR-006)",
                    type_expr_display(&r#type.value)
                ),
            });
        }
    }
}

/// The `(authority, name)` of a source that is EXACTLY `${{ inputs.X }}` /
/// `${{ inputs.X }}` / `${{ const.X }}` (one template island covering the
/// whole value, a bare `Member { Ident(authority), X }` with no further
/// path), else `None`. `for_each` sources carry the raw `${{ … }}`
/// wrapper, so the island's pre-parsed `expr` is the entry.
fn bare_authority_reference(src: &str) -> Option<(String, String)> {
    let islands = scan_templates(src).ok()?;
    let [island] = islands.as_slice() else {
        return None; // zero, or more than one → not a bare reference
    };
    // The island must BE the whole value (no `prefix ${{ … }} suffix`).
    if src[..island.start].trim().is_empty()
        && src[island.end..].trim().is_empty()
        && let Expr::Member { base, field } = &island.expr
        && let Expr::Ident(r) = base.as_ref()
        && matches!(r.as_str(), "inputs" | "config" | "const")
    {
        return Some((r.clone(), field.clone()));
    }
    None
}

/// The `with:` aliases of one task that bind a SHAPELESS task's whole
/// output (`bill: "${{ tasks.bill.output }}"` where `bill` declares no
/// shape) — a deep read THROUGH the alias is exactly as unverifiable as
/// the direct deep ref (F3's own repro reads `with.bill.total_usd`).
/// Maps alias → producer id.
fn shapeless_with_aliases(
    task: &RawTask,
    shapes: &BTreeMap<&str, Shape<'_>>,
    all_tasks: &std::collections::BTreeSet<&str>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (name, v) in &task.with {
        let Some(s) = v.value.as_str() else {
            continue;
        };
        let Some(producer) = bare_whole_output_ref(s) else {
            continue;
        };
        if all_tasks.contains(producer.as_str()) && !shapes.contains_key(producer.as_str()) {
            out.insert(name.value.clone(), producer);
        }
    }
    out
}

/// The producer id of a value that is EXACTLY `${{ tasks.<id>.output }}`
/// (one island covering the whole string · no deeper path).
fn bare_whole_output_ref(s: &str) -> Option<String> {
    let islands = scan_templates(s).ok()?;
    let [island] = islands.as_slice() else {
        return None;
    };
    if !s[..island.start].trim().is_empty() || !s[island.end..].trim().is_empty() {
        return None;
    }
    match task_output_paths(&island.expr).as_slice() {
        [(id, path)] if path.is_empty() => Some(id.clone()),
        _ => None,
    }
}

/// Scan one text for deep reads THROUGH a `with:` alias bound to a
/// shapeless task output — the same blind spot as the direct deep ref,
/// one hop later (the alias is the edge since W2 « the flow »).
fn check_alias_text(
    site: &str,
    text: &str,
    aliases: &BTreeMap<String, String>,
    unverifiable: &mut Vec<UnverifiableOutputRef>,
) {
    let Ok(islands) = scan_templates(text) else {
        return;
    };
    for island in islands {
        for (alias, path) in with_alias_paths(&island.expr) {
            let Some(producer) = aliases.get(&alias) else {
                continue; // an alias bound to a shaped/local value is the shape lanes'
            };
            unverifiable.push(UnverifiableOutputRef {
                site: site.to_owned(),
                reference: format!(
                    "tasks.{producer}.output.{} (via with.{alias})",
                    path.join(".")
                ),
            });
        }
    }
}

/// Collect each task's declared output address space. `output:` bindings
/// REBIND the output namespace, so they take precedence over `schema:`.
/// A `returns:` contract walks through the SAME door — its
/// `lower(returns)` projection (`lowered` · owned by the caller) is the
/// task's schema when no verb-level `schema:` exists (`NIKA-TYPE-003`
/// forbids both, so at most one is ever present).
fn declared_shapes<'a>(
    wf: &'a RawWorkflow,
    lowered: &'a BTreeMap<String, Value>,
) -> BTreeMap<&'a str, Shape<'a>> {
    let mut shapes = BTreeMap::new();
    for task in &wf.tasks {
        let t = &task.value;
        let id = t.id.value.as_str();
        if !t.extract.is_empty() {
            shapes.insert(
                id,
                Shape::Bindings(t.extract.iter().map(|(n, _)| n.value.as_str()).collect()),
            );
            continue;
        }
        let schema = match &t.action {
            RawAction::Infer(a) => a.schema.as_ref(),
            RawAction::Agent(a) => a.schema.as_ref(),
            RawAction::Exec(_) | RawAction::Invoke(_) => None,
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown action: {other:?}"),
        };
        if let Some(s) = schema {
            shapes.insert(id, Shape::Schema(&s.value));
        } else if let Some(low) = lowered.get(id) {
            shapes.insert(id, Shape::Schema(low));
        }
    }
    shapes
}

/// Every expression-bearing text of a task (main verb + task-level
/// fields). `output:` binding values are jq programs, not CEL — skipped.
fn task_texts(task: &RawTask) -> Vec<&str> {
    let mut texts = action_texts(&task.action);
    if let Some(when) = &task.when
        && let Some(expr) = when.value.as_expr()
    {
        texts.push(expr);
    }
    if let Some(f) = &task.for_each
        && let nika_schema::raw::ForEachValue::Expression(src) = &f.value
    {
        texts.push(src);
    }
    for (_, v) in &task.with {
        collect_value_strings(&v.value, &mut texts);
    }
    texts
}

/// Every expression-bearing text of one action (works for main verbs
/// AND `on_finally` cleanup verbs).
fn action_texts(action: &RawAction) -> Vec<&str> {
    match action {
        RawAction::Exec(a) => {
            let mut texts = a.command.text_fragments();
            if let Some(stdin) = &a.stdin {
                texts.push(stdin.value.as_str());
            }
            for (_, v) in &a.env {
                texts.push(v.value.as_str());
            }
            texts
        }
        RawAction::Invoke(a) => {
            let mut texts = Vec::new();
            if let Some(args) = &a.args {
                collect_value_strings(&args.value, &mut texts);
            }
            texts
        }
        RawAction::Infer(a) => {
            let mut texts = vec![a.prompt.value.as_str()];
            if let Some(system) = &a.system {
                texts.push(&system.value);
            }
            texts
        }
        RawAction::Agent(a) => {
            let mut texts = vec![a.prompt.value.as_str()];
            if let Some(system) = &a.system {
                texts.push(&system.value);
            }
            texts
        }
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
}

/// Every string scalar inside a JSON value (invoke args · with values).
fn collect_value_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s),
        Value::Array(items) => {
            for item in items {
                collect_value_strings(item, out);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_value_strings(item, out);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// Type-check every island of one text against the declared shapes.
/// A deep ref whose target IS a workflow task but carries no declared
/// shape is no finding (the soundness law) — it is counted as
/// unverifiable instead, so the verdict line can name its blind spot.
#[allow(clippy::too_many_arguments)] // the two out-lanes + the two task sets ride together
fn check_text(
    site: &str,
    text: &str,
    shapes: &BTreeMap<&str, Shape<'_>>,
    all_tasks: &std::collections::BTreeSet<&str>,
    findings: &mut Vec<SchemaTypeFinding>,
    unverifiable: &mut Vec<UnverifiableOutputRef>,
) {
    // a malformed island is the parser/analyzer's finding, not ours
    let Ok(islands) = scan_templates(text) else {
        return;
    };
    for island in islands {
        for (target, path) in task_output_paths(&island.expr) {
            if path.is_empty() {
                continue; // whole-output reference · always valid
            }
            let Some(shape) = shapes.get(target.as_str()) else {
                // A real task with NO declared output shape: the lane
                // cannot prove anything (opaque), but the ✔ must not
                // read as checked-and-fine (F3). A target that is no
                // task at all is the DAG lane's finding, never this
                // one's.
                if all_tasks.contains(target.as_str()) {
                    unverifiable.push(UnverifiableOutputRef {
                        site: site.to_owned(),
                        reference: format!("tasks.{target}.output.{}", path.join(".")),
                    });
                }
                continue;
            };
            let detail = match shape {
                Shape::Schema(schema) => resolve(schema, &path),
                Shape::Bindings(names) => check_binding(names, &path),
            };
            if let Some(detail) = detail {
                findings.push(SchemaTypeFinding {
                    site: site.to_owned(),
                    reference: format!("tasks.{target}.output.{}", path.join(".")),
                    target: target.clone(),
                    detail,
                });
            }
        }
    }
}

/// Check a deep path against `output:` binding names — the first segment
/// must be a binding; the inner structure is jq-shaped (opaque).
fn check_binding(names: &[&str], path: &[String]) -> Option<String> {
    let first = path.first()?;
    if names.contains(&first.as_str()) {
        return None;
    }
    let clause = suggestion_clause(did_you_mean(first, names.iter().copied()));
    Some(format!(
        "`{first}` is not one of the declared output bindings [{}]{clause}",
        names.join(", ")
    ))
}

/// Resolve a path against a JSON Schema. `Some(detail)` when the schema
/// PROVES the path invalid; `None` when it resolves or goes opaque.
fn resolve(schema: &Value, path: &[String]) -> Option<String> {
    let mut node = schema;
    for (depth, segment) in path.iter().enumerate() {
        // `$ref` indirection is statically opaque (no resolver here)
        if node.get("$ref").is_some() {
            return None;
        }
        // combinators: OK if ANY branch admits the rest of the path
        for key in ["anyOf", "oneOf", "allOf"] {
            if let Some(branches) = node.get(key).and_then(Value::as_array) {
                return branches
                    .iter()
                    .all(|b| resolve(b, &path[depth..]).is_some())
                    .then(|| format!("no `{key}` branch declares `{segment}`"));
            }
        }
        // arrays are transparent: `output[0].title` drops the index hop,
        // so descend `items` until the element schema surfaces
        while node.get("type").and_then(Value::as_str) == Some("array") {
            match node.get("items") {
                Some(items) => node = items,
                None => return None, // un-itemized array · opaque
            }
        }
        let Some(props) = node.get("properties").and_then(Value::as_object) else {
            return match node.get("type").and_then(Value::as_str) {
                // an object with no properties map is opaque
                Some("object") | None => None,
                Some(scalar) => Some(format!(
                    "cannot descend into `{scalar}`-typed value with `.{segment}`"
                )),
            };
        };
        if let Some(next) = props.get(segment.as_str()) {
            node = next;
        } else if is_open_object(node) {
            return None; // explicitly open · unknown keys allowed
        } else {
            let clause = suggestion_clause(did_you_mean(segment, props.keys().map(String::as_str)));
            return Some(format!(
                "`{segment}` is not in the declared schema — keys here: [{}]{clause}",
                props.keys().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    None
}

/// Whether an object schema explicitly allows undeclared keys
/// (`additionalProperties: true` or a schema object). JSON Schema's
/// DEFAULT is open, but a structured-output `schema:` compiles strict —
/// flagging unknown keys against declared `properties` is the point of
/// the check, so only an EXPLICIT opt-out goes opaque.
fn is_open_object(node: &Value) -> bool {
    match node.get("additionalProperties") {
        Some(Value::Bool(open)) => *open,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn findings_of(yaml: &str) -> Vec<SchemaTypeFinding> {
        scan_types(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")).0
    }

    fn unverifiable_of(yaml: &str) -> Vec<UnverifiableOutputRef> {
        scan_types(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse")).1
    }

    fn for_each_wf(authority: &str, var_decl: &str) -> String {
        format!(
            "nika: w\nmodel: mock/echo\n{authority}:\n  xs: {var_decl}\n\
             tasks:\n  fan:\n    for_each: {{ items: \"${{{{ {authority}.xs }}}}\" }}\n    \
             with: {{ it: \"${{{{ item }}}}\" }}\n    infer: {{ prompt: \"do ${{{{ with.it }}}}\" }}\n"
        )
    }

    #[test]
    fn for_each_over_an_untyped_non_array_const_is_caught_before_run() {
        // A constant is baked into the file: spec 01 §const is normative —
        // « immutable across the run and never caller-supplied ». The
        // `--var` override that spares an UNTYPED entry reaches `inputs:`
        // only (and an untyped entry is legal in `const:` alone), so an
        // untyped constant's literal IS its run value. A non-array can
        // therefore never become one: the run refuses it (NIKA-VAR-006)
        // and the check must say so first, or `nika check` is a linter.
        for literal in [
            "\"not-an-array\"",
            "3",
            "true",
            // Missing `type:` → a bare literal OBJECT constant, per the
            // spec 01 discriminator (BOTH keys make a typed constant).
            "{ value: [\"x\"] }",
        ] {
            let f = findings_of(&for_each_wf("const", literal));
            assert_eq!(f.len(), 1, "literal {literal} flagged: {f:?}");
            assert!(f[0].detail.contains("for_each"), "{:?}", f[0]);
        }
        // An array literal is exactly what `for_each` wants.
        assert!(
            findings_of(&for_each_wf("const", "[\"x\", \"y\"]")).is_empty(),
            "an array constant is the legal case"
        );
    }

    #[test]
    fn for_each_over_a_typed_non_array_var_is_caught_before_run() {
        // The runtime refuses a non-array for_each collection (NIKA-VAR-006);
        // a var DECLARED type:string can NEVER be an array, so the check
        // must catch it BEFORE the run (audit-before-run). Post-R3b the
        // declared type speaks the full TypeExpr — primitives AND
        // composites are judged by the one type core.
        for t in [
            "string",
            "number",
            "integer",
            "bool",
            "{ enum: [\"a\", \"b\"] }",
            "{ object: { x: string } }",
        ] {
            let f = findings_of(&for_each_wf(
                "inputs",
                &format!("{{ type: {t}, required: true }}"),
            ));
            assert_eq!(f.len(), 1, "type {t} flagged: {f:?}");
            assert!(
                f[0].detail.contains("for_each") && f[0].detail.contains("type:"),
                "{:?}",
                f[0]
            );
        }
        // A union WITH an array member admits an array — never flagged.
        assert!(
            findings_of(&for_each_wf(
                "inputs",
                "{ type: { union: [{ array: string }, string] }, required: true }"
            ))
            .is_empty(),
            "a union admitting an array is not provably non-array"
        );
    }

    #[test]
    fn for_each_over_a_valid_or_unknown_source_is_never_flagged() {
        // Zero false positives: a typed ARRAY var, an untyped literal that
        // IS an array, an inline list, and a `tasks.*` source are all left
        // alone.
        //
        // The « an UNTYPED var (a --var override could pass an array) »
        // exemption that once covered `const: xs: "hello"` here was an
        // inputs-only rule generalised one authority too far: `--var` sets
        // an `inputs:` value and refuses unknown keys, and spec 01 §const
        // is normative — a constant is « immutable across the run and
        // never caller-supplied ». The run proved it, refusing at dispatch
        // (NIKA-VAR-006) a workflow this check had just cleared. That case
        // is now asserted flagged in
        // `for_each_over_an_untyped_non_array_const_is_caught_before_run`.
        assert!(
            findings_of(&for_each_wf(
                "inputs",
                "{ type: { array: string }, required: true }"
            ))
            .is_empty()
        );
        assert!(findings_of(&for_each_wf("const", "[\"a\", \"b\"]")).is_empty()); // untyped literal array
        // An inline list literal source never resolves to a bare authority ref.
        let inline = "nika: w\nmodel: mock/echo\ntasks:\n  fan:\n    \
                      for_each: { items: [1, 2, 3] }\n    with: { it: \"${{ item }}\" }\n    \
                      infer: { prompt: \"do ${{ with.it }}\" }\n";
        assert!(findings_of(inline).is_empty());
    }

    #[test]
    fn bare_authority_reference_matches_only_a_whole_value_authority_ref() {
        assert_eq!(
            bare_authority_reference("${{ inputs.x }}"),
            Some(("inputs".to_owned(), "x".to_owned()))
        );
        assert_eq!(
            bare_authority_reference("${{ const.x }}"),
            Some(("const".to_owned(), "x".to_owned()))
        );
        assert_eq!(bare_authority_reference("${{ vars.x }}"), None); // dead root → not an authority
        assert_eq!(bare_authority_reference("${{ inputs.x.field }}"), None); // path → not bare
        assert_eq!(bare_authority_reference("size(${{ inputs.x }})"), None); // wrapped → not bare
        assert_eq!(bare_authority_reference("${{ tasks.a.output }}"), None); // not an authority
        assert_eq!(bare_authority_reference("prefix ${{ inputs.x }}"), None); // surrounding text
    }

    /// An infer task with a 2-field object schema, consumed by `use_it`
    /// through its `with:` boundary (where deep refs live in W2).
    fn schema_wf(consumer_expr: &str) -> String {
        format!(
            "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  extract:\n    infer:\n      prompt: \"extract\"\n      max_tokens: 100\n      schema:\n        type: object\n        properties:\n          summary: {{ type: string }}\n          tags:\n            type: array\n            items:\n              type: object\n              properties:\n                name: {{ type: string }}\n        required: [summary]\n  use_it:\n    with: {{ src: \"{consumer_expr}\" }}\n    exec: {{ shell: \"echo ${{{{ with.src }}}}\" }}\n"
        )
    }

    #[test]
    fn typo_in_field_name_is_caught() {
        // THE headline: `summray` does not exist — caught with zero tokens.
        let f = findings_of(&schema_wf("${{ tasks.extract.output.summray }}"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].site, "use_it");
        assert_eq!(f[0].reference, "tasks.extract.output.summray");
        assert!(f[0].detail.contains("summary"), "lists the real keys");
    }

    #[test]
    fn valid_field_is_clean() {
        assert!(findings_of(&schema_wf("${{ tasks.extract.output.summary }}")).is_empty());
    }

    #[test]
    fn array_items_descend_transparently() {
        // `tags[0].name` — the index hop drops, items descends.
        assert!(findings_of(&schema_wf("${{ tasks.extract.output.tags[0].name }}")).is_empty());
        let f = findings_of(&schema_wf("${{ tasks.extract.output.tags[0].nmae }}"));
        assert_eq!(f.len(), 1);
        assert!(f[0].detail.contains("name"), "detail: {}", f[0].detail);
    }

    #[test]
    fn scalar_descent_is_proven_invalid() {
        let f = findings_of(&schema_wf("${{ tasks.extract.output.summary.length }}"));
        assert_eq!(f.len(), 1);
        assert!(
            f[0].detail.contains("string"),
            "names the scalar type: {}",
            f[0].detail
        );
    }

    #[test]
    fn unshaped_task_is_opaque() {
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { command: [\"date\"] }\n  b:\n    with: { w: \"${{ tasks.a.output.whatever }}\" }\n    exec: { command: [\"echo\", \"${{ with.w }}\"] }\n";
        assert!(findings_of(yaml).is_empty(), "no schema → no claim");
    }

    #[test]
    fn explicitly_open_schema_is_opaque() {
        let yaml = "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: true\n        properties:\n          known: { type: string }\n  b:\n    with: { u: \"${{ tasks.a.output.unknown_key }}\" }\n    exec: { command: [\"echo\", \"${{ with.u }}\"] }\n";
        assert!(findings_of(yaml).is_empty(), "explicit opt-out honored");
    }

    #[test]
    fn additional_properties_schema_object_is_open() {
        // `additionalProperties: { type: string }` (an OBJECT, not `true`)
        // ALSO opens the object — JSON Schema's value-schema form allows
        // undeclared keys (validated against the inner schema). So an
        // unknown key must NOT be flagged. This pins the
        // `Some(Value::Object(_)) => true` arm of `is_open_object`, which
        // the Bool-form test above does not exercise.
        let yaml = "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        additionalProperties: { type: string }\n        properties:\n          known: { type: string }\n  b:\n    with: { u: \"${{ tasks.a.output.unknown_key }}\" }\n    exec: { command: [\"echo\", \"${{ with.u }}\"] }\n";
        assert!(
            findings_of(yaml).is_empty(),
            "a value-schema additionalProperties opens the object → no finding"
        );

        // Control: with additionalProperties ABSENT, the same unknown key
        // IS flagged — proving the open-object arm is what suppresses it
        // (so deleting that arm becomes observable as a spurious finding).
        let closed = "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        type: object\n        properties:\n          known: { type: string }\n  b:\n    with: { u: \"${{ tasks.a.output.unknown_key }}\" }\n    exec: { command: [\"echo\", \"${{ with.u }}\"] }\n";
        let f = findings_of(closed);
        assert_eq!(f.len(), 1, "closed object flags the unknown key");
        assert!(f[0].detail.contains("known"), "lists the real key");
    }

    #[test]
    fn output_bindings_rebind_the_address_space() {
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { command: [\"cat\", \"data.json\"] }\n    extract:\n      first: \". | .[0]\"\n  b:\n    with:\n      ok: \"${{ tasks.a.output.first }}\"\n      typo: \"${{ tasks.a.output.frist }}\"\n    exec: { command: [\"echo\", \"${{ with.ok }}\", \"${{ with.typo }}\"] }\n";
        let f = findings_of(yaml);
        assert_eq!(f.len(), 1, "first ok, frist flagged");
        assert!(f[0].detail.contains("first"), "lists bindings");
    }

    #[test]
    fn prompt_interpolation_is_checked() {
        // The wow case — a typo'd deep ref feeding an infer PROMPT is
        // caught statically at the boundary that imports it.
        let yaml = "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  extract:\n    infer:\n      prompt: \"extract\"\n      max_tokens: 100\n      schema:\n        type: object\n        properties:\n          summary: { type: string }\n  report:\n    with: { s: \"${{ tasks.extract.output.sumary }}\" }\n    infer: { prompt: \"report on ${{ with.s }}\", max_tokens: 50 }\n";
        let f = findings_of(yaml);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].site, "report");
    }

    #[test]
    fn any_of_admits_when_one_branch_matches() {
        let yaml = "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  a:\n    infer:\n      prompt: \"x\"\n      max_tokens: 10\n      schema:\n        anyOf:\n          - type: object\n            properties:\n              left: { type: string }\n          - type: object\n            properties:\n              right: { type: string }\n  b:\n    with:\n      l: \"${{ tasks.a.output.left }}\"\n      n: \"${{ tasks.a.output.neither }}\"\n    exec: { command: [\"echo\", \"${{ with.l }}\", \"${{ with.n }}\"] }\n";
        let f = findings_of(yaml);
        assert_eq!(f.len(), 1, "left admits, neither fails all branches");
        assert!(f[0].reference.ends_with("neither"));
    }

    /// F3 (2026-07-30): a deep ref into a task with NO declared output
    /// shape (a builtin invoke without `returns:`) stays NO finding —
    /// the soundness law — but it is COUNTED as unverifiable, so the
    /// verdict line names its blind spot instead of reading as
    /// universal. A ref into a SHAPED task is judged, never counted; a
    /// ref into a task that does not exist is the DAG lane's, never
    /// this one's.
    #[test]
    fn a_deep_ref_into_a_shapeless_task_is_unverifiable_not_a_finding() {
        let yaml = "nika: w\nmodel: mock/echo\npermits: { tools: [\"nika:inspect\"] }\ntasks:\n  inspect:\n    invoke: { tool: \"nika:inspect\", args: { view: \"cost\" } }\n  report:\n    infer: { prompt: \"total ${{ tasks.inspect.output.total_usd }}\" }\n";
        let findings = findings_of(yaml);
        assert!(
            findings.is_empty(),
            "no proof either way, no finding: {findings:?}"
        );
        let unverifiable = unverifiable_of(yaml);
        assert_eq!(
            unverifiable.len(),
            1,
            "the blind spot is counted exactly once: {unverifiable:?}"
        );
        assert_eq!(unverifiable[0].site, "report", "{unverifiable:?}");
        assert_eq!(
            unverifiable[0].reference, "tasks.inspect.output.total_usd",
            "{unverifiable:?}"
        );
    }

    #[test]
    fn a_deep_ref_through_a_with_alias_is_unverifiable_too() {
        // The record's own repro (run 5 · F3): the shapeless output is
        // bound WHOLE into `with:` and the deep read happens one hop
        // later, through the alias.
        let yaml = "nika: w\nmodel: mock/echo\npermits: { tools: [\"nika:inspect\"], exec: [\"echo\"] }\ntasks:\n  bill:\n    invoke: { tool: \"nika:inspect\", args: { view: \"cost\" } }\n  report:\n    with: { bill: \"${{ tasks.bill.output }}\" }\n    exec: { command: [\"echo\", \"${{ with.bill.total_usd }}\"] }\n";
        let unverifiable = unverifiable_of(yaml);
        assert_eq!(
            unverifiable.len(),
            1,
            "the aliased blind spot is counted: {unverifiable:?}"
        );
        assert_eq!(unverifiable[0].site, "report", "{unverifiable:?}");
        assert_eq!(
            unverifiable[0].reference, "tasks.bill.output.total_usd (via with.bill)",
            "{unverifiable:?}"
        );
    }

    #[test]
    fn a_ref_into_a_missing_task_is_the_dag_lanes_never_this_count() {
        let unverifiable = unverifiable_of(
            "nika: w\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"${{ tasks.ghost.output.x }}\" }\n",
        );
        assert!(
            unverifiable.is_empty(),
            "a nonexistent target is never this lane's to count: {unverifiable:?}"
        );
    }
}
