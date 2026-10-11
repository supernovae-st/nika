// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring prompt's trims (2026-10-10), behind the `unstable-prompt-trims` feature and off
//! by default: an evaluation measures authoring quality with and without them before any default
//! changes. The schema the document door shows loses its `description` annotations (29,749
//! bytes compact, 9,705 without them); the callable contracts follow what the reader's typed plan reaches instead of
//! the whole everyday set (up to about 15 KB for a request that fetches, converts and asks
//! nothing). Each trim reads a type, never the request's words: a schema keyword, an operation,
//! an effect's verb or policy.

use nika_compile_reader::plan::{EffectPolicy, EffectVerb, Op, Plan};
use serde_json::Value;

/// Whether this build trims the authoring prompt.
pub(crate) const ON: bool = cfg!(feature = "unstable-prompt-trims");

/// The keywords whose value is a schema, or a list of schemas in an older draft's `items`.
const SCHEMA: [&str; 10] = [
    "items",
    "additionalProperties",
    "additionalItems",
    "not",
    "if",
    "then",
    "else",
    "contains",
    "propertyNames",
    "unevaluatedProperties",
];
/// The keywords whose value maps names to schemas.
const MAPS: [&str; 5] = [
    "properties",
    "patternProperties",
    "$defs",
    "definitions",
    "dependentSchemas",
];
/// The keywords whose value lists schemas.
const LISTS: [&str; 4] = ["allOf", "anyOf", "oneOf", "prefixItems"];

/// The schema text the document door shows: the Spec's workflow schema, compact, and without its
/// descriptions when this build trims the prompt.
pub(crate) fn schema_text(mut schema: Value) -> String {
    if ON {
        bare(&mut schema);
    }
    schema.to_string()
}

/// `schema` without the `description` annotation of any schema it holds. Only the keyword goes:
/// a property a `properties` map names `description` (an output's own field) is a name of the
/// language and stays, its own schema trimmed in turn.
fn bare(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    object.retain(|key, value| !(key == "description" && value.is_string()));
    for (key, value) in object {
        let key = key.as_str();
        if SCHEMA.contains(&key) || LISTS.contains(&key) {
            match value {
                Value::Array(schemas) => schemas.iter_mut().for_each(bare),
                one => bare(one),
            }
        } else if MAPS.contains(&key)
            && let Some(map) = value.as_object_mut()
        {
            map.values_mut().for_each(bare);
        }
    }
}

/// The everyday builtins the reader's typed plan reaches, in the everyday set's order: what any
/// document may need (`read`, `write`, `glob`, `jq`, `date`), then `convert` for a computation
/// over a parsed table, `prompt` for an effect a human approves first, `fetch` for a fetched or
/// looked-up source, an explored region or an effect beyond a local file, and `notify` for a
/// sent, notified or published effect. A plan with no operation and no effect scopes nothing:
/// the whole everyday set.
pub(crate) fn reached(plan: &Plan) -> Vec<&'static str> {
    if plan.steps.is_empty() && plan.effects.is_empty() {
        return super::EVERYDAY.to_vec();
    }
    let op = |ops: &[Op]| plan.steps.iter().any(|step| ops.contains(&step.op));
    let verb = |verbs: &[EffectVerb]| plan.effects.iter().any(|e| verbs.contains(&e.verb));
    let outbound = plan.effects.iter().any(|e| e.verb != EffectVerb::Write);
    let approved = plan
        .effects
        .iter()
        .any(|e| e.policy == EffectPolicy::HumanFirst);
    let notified = verb(&[EffectVerb::Send, EffectVerb::Notify, EffectVerb::Publish]);
    let fetched = op(&[Op::Fetch, Op::Lookup, Op::Explore]) || outbound;
    let mut names = vec!["read", "write", "glob", "jq"];
    names.extend(op(&[Op::Compute]).then_some("convert"));
    names.extend(approved.then_some("prompt"));
    names.extend(fetched.then_some("fetch"));
    names.extend(notified.then_some("notify"));
    names.push("date");
    names
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{EffectPolicy, EffectVerb, MAPS, Op, Plan, bare, reached};
    use nika_compile_reader::plan::{Effect, Step};
    use serde_json::Value;
    use std::collections::BTreeSet;

    /// Every name a `properties` map declares, at any depth, and whether a `description`
    /// annotation remains at any schema level.
    fn walk(value: &Value, names: &mut BTreeSet<String>, annotated: &mut bool) {
        match value {
            Value::Object(object) => {
                for (key, nested) in object {
                    if key == "description" && nested.is_string() {
                        *annotated = true;
                    }
                    if MAPS.contains(&key.as_str())
                        && let Some(map) = nested.as_object()
                    {
                        names.extend(map.keys().cloned());
                    }
                    walk(nested, names, annotated);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, names, annotated)),
            _ => {}
        }
    }

    #[test]
    fn the_bare_schema_keeps_every_name_and_drops_every_description() {
        let full: Value = serde_json::from_str(nika_pack::schema_json()).expect("the schema");
        let (mut before, mut annotated) = (BTreeSet::new(), false);
        walk(&full, &mut before, &mut annotated);
        assert!(annotated, "the Spec's schema describes its fields");
        let mut trimmed = full.clone();
        bare(&mut trimmed);
        let (mut after, mut left) = (BTreeSet::new(), false);
        walk(&trimmed, &mut after, &mut left);
        assert!(!left, "a description annotation remains");
        assert_eq!(before, after, "a name of the language was dropped");
        let (full, trimmed) = (full.to_string().len(), trimmed.to_string().len());
        assert!(trimmed * 2 < full, "{trimmed} of {full} bytes");
        // A property named `description` is a field, never the annotation.
        let mut field = serde_json::json!({"properties": {"description": {"type": "string",
            "description": "the output's description"}}, "description": "an output"});
        bare(&mut field);
        assert_eq!(
            field,
            serde_json::json!({"properties": {"description": {"type": "string"}}})
        );
    }

    #[test]
    fn the_callables_follow_what_the_typed_plan_reaches() {
        let step = |op: Op| Step::new(op, "e", "d", Vec::new());
        let effect = |verb: EffectVerb, policy: EffectPolicy| Effect::new(verb, "t", "e", policy);
        let mut local = Plan::default();
        local.steps.extend([step(Op::Read), step(Op::Compute)]);
        local
            .effects
            .push(effect(EffectVerb::Write, EffectPolicy::Automatic));
        assert_eq!(
            reached(&local),
            ["read", "write", "glob", "jq", "convert", "date"]
        );
        let mut outbound = Plan::default();
        outbound.steps.push(step(Op::Draft));
        outbound
            .effects
            .push(effect(EffectVerb::Send, EffectPolicy::HumanFirst));
        assert_eq!(
            reached(&outbound),
            [
                "read", "write", "glob", "jq", "prompt", "fetch", "notify", "date"
            ]
        );
        // Nothing typed to scope by: the whole everyday set.
        assert_eq!(reached(&Plan::default()), super::super::EVERYDAY);
    }

    /// A plan that reads a table and totals it: what both builds are compared on.
    fn totals() -> Plan {
        let mut plan = Plan::default();
        plan.steps.push(Step::new(Op::Read, "e", "d", Vec::new()));
        plan.steps
            .push(Step::new(Op::Compute, "e", "d", Vec::new()));
        plan
    }

    #[test]
    #[cfg(not(feature = "unstable-prompt-trims"))]
    fn a_default_build_sends_the_whole_prompt() {
        let refs = crate::shelf::references("Read ./data/orders.csv and total the amounts", 1);
        assert_eq!(
            crate::shelf::builtins_for(&refs, &totals()),
            crate::shelf::builtins_of(&refs)
        );
        let full: Value = serde_json::from_str(nika_pack::schema_json()).expect("the schema");
        assert_eq!(
            crate::foundry::document::create::language(),
            full.to_string()
        );
    }

    #[test]
    #[cfg(feature = "unstable-prompt-trims")]
    fn a_trimmed_build_sends_the_bare_schema_and_the_reached_callables() {
        assert_eq!(
            crate::shelf::builtins_for(&[], &totals()),
            reached(&totals())
        );
        let shown: Value =
            serde_json::from_str(&crate::foundry::document::create::language()).expect("json");
        let (mut names, mut annotated) = (BTreeSet::new(), false);
        walk(&shown, &mut names, &mut annotated);
        assert!(!annotated && !names.is_empty(), "{names:?}");
    }
}
