// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika:jq` readers a skip leaves on null, which [`super::scan`] cannot judge. A whole
//! binding handed to a filter as its `input:` (`o: ${{ tasks.X.output }}` · `input: ${{ with.o
//! }}`) raises nothing in CEL: the null reaches the filter, which may iterate, index or match it
//! (live gauntlet r02 B7, r05 B1, 2026-10-17: « run ready ✔ » and a rehearsal, then the first
//! day with nothing new failed on « cannot use null as … »). The filter is jq, which this walk
//! does not read, and evaluating it here would run a program with no bound. Each such reader is
//! named, with the producer and the bindings that carry its null, for a host that evaluates jq
//! in a bounded process of its own (a rehearsal) to try on that null.
//!
//! A reader is named only where the skip leaves it to run: the skipped world's fold does not
//! cancel it, none of its bindings raises first (that is [`super::scan`]'s row) and its `when:`
//! is not proven false there. A fan-out, and a filter a template builds, are not named.

use std::collections::BTreeMap;

use nika_schema::expression::Expr;
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask, RawWorkflow};

use super::{Skip, Val, View, may_settle, reads_of, with_key};
use crate::edges::{Edge, producer_ids};
use crate::gates::sole_island;
use crate::settle::{S_ALL, S_CANCELLED, S_SKIPPED, fold_settled};

/// One `nika:jq` reader to try on a skipped producer's null.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NullIntoJq {
    /// The reading task.
    pub reader: String,
    /// The task that can be skipped.
    pub producer: String,
    /// Why the producer can be skipped, in the hint's words.
    pub reasons: Vec<&'static str>,
    /// The reader's `with:` keys that hold the producer's null whole and reach its `input:`, in
    /// the order the reader declares them (a verb field reads a task only through `with:`).
    pub carriers: Vec<String>,
}

/// Every `nika:jq` reader a skip leaves to run with the skipped value whole in its `input:`, in
/// task order, once per (reader, producer). `edges` and `topo_waves` are the analyzed graph, as
/// for [`super::scan`]: call this on a conformant DAG only.
#[must_use]
pub fn null_into_jq(
    wf: &RawWorkflow,
    edges: &[Edge],
    topo_waves: &[Vec<usize>],
) -> Vec<NullIntoJq> {
    let tasks = &wf.tasks;
    let may = |n: usize| tasks.get(n).map_or(S_ALL, |t| may_settle(&t.value));
    let base = fold_settled(tasks.len(), edges, topo_waves, None, may);
    let mut found: Vec<(usize, NullIntoJq)> = Vec::new();
    for (p, producer) in tasks.iter().enumerate() {
        let Some(skip) = Skip::of(&producer.value, base.of(p)) else {
            continue;
        };
        let readers: Vec<(usize, super::Reads)> = (tasks.iter().enumerate())
            .filter(|(_, task)| {
                producer_ids(&task.value)
                    .iter()
                    .any(|id| id == skip.producer)
            })
            .map(|(reader, task)| (reader, reads_of(&task.value, &skip)))
            .collect();
        // The world where `p` skipped, each reader settled as its walk decided (as `scan`).
        let fates: BTreeMap<usize, u8> = (readers.iter())
            .filter_map(|(reader, reads)| reads.settles.map(|s| (*reader, s | S_CANCELLED)))
            .collect();
        let world = fold_settled(tasks.len(), edges, topo_waves, Some((p, S_SKIPPED)), |n| {
            fates.get(&n).copied().unwrap_or_else(|| may(n))
        });
        for (reader, reads) in &readers {
            // A raise first is `scan`'s row, a closed gate skips it, a cancel never runs it.
            if reads.settles.is_some() || world.of(*reader) == S_CANCELLED {
                continue;
            }
            let Some(task) = tasks.get(*reader) else {
                continue;
            };
            if let Some(carriers) = carried(&task.value, &skip) {
                let row = NullIntoJq {
                    reader: task.value.id.value.clone(),
                    producer: skip.producer.to_owned(),
                    reasons: skip.reasons.clone(),
                    carriers,
                };
                found.push((*reader, row));
            }
        }
    }
    found.sort_by_key(|(reader, _)| *reader);
    found.into_iter().map(|(_, row)| row).collect()
}

/// The `with:` keys of a plain `nika:jq` reader that hold the skipped value whole and reach its
/// `input:`; `None` when the reader is a fan-out, its filter is built by a template, or its input
/// carries no such value.
fn carried(task: &RawTask, skip: &Skip<'_>) -> Option<Vec<String>> {
    if task.for_each.is_some() {
        return None;
    }
    let RawAction::Invoke(invoke) = &task.action else {
        return None;
    };
    let RawInvokeTarget::Tool(tool) = &invoke.target else {
        return None;
    };
    let args = &invoke.args.as_ref()?.value;
    let expression = args.get("expression")?.as_str()?;
    if tool.value != "nika:jq" || expression.contains("${{") {
        return None;
    }
    let view = View::body(skip, task);
    let held: Vec<&str> = (task.with.iter())
        .map(|(key, _)| key.value.as_str())
        .filter(|key| view.with.get(*key) == Some(&Val::Skipped))
        .collect();
    let mut keys: Vec<String> = Vec::new();
    visit(args.get("input")?, &mut |expr| {
        if let Some(key) = with_key(expr).filter(|key| held.contains(key))
            && !keys.iter().any(|known| known == key)
        {
            keys.push(key.to_owned());
        }
    });
    (!keys.is_empty()).then_some(keys)
}

/// Each value of `value` that is one whole island, its expression handed to `seen`.
fn visit(value: &serde_json::Value, seen: &mut impl FnMut(&Expr)) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(expr) = sole_island(text) {
                seen(&expr);
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|item| visit(item, seen)),
        serde_json::Value::Object(fields) => fields.values().for_each(|item| visit(item, seen)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn rows(yaml: &str) -> Vec<NullIntoJq> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let analyzed = crate::analyze(&wf).expect("fixture analyzes");
        null_into_jq(&wf, &analyzed.edges, &analyzed.topo_waves)
    }

    /// The live gauntlet r02 shape (2026-10-17): a fetch behind a gate, and the filter after it
    /// with no `when:` of its own, which ran on the fetch's null the day nothing changed.
    const UNGUARDED: &str = "\
nika: skipped-into-jq
permits:
  tools: [nika:jq]
tasks:
  changed:
    invoke:
      tool: nika:jq
      args: { input: {}, expression: 'false' }
  page_again:
    with: { go: '${{ tasks.changed.output }}' }
    when: ${{ with.go == true }}
    invoke:
      tool: nika:jq
      args: { input: {}, expression: '\"<b>Python 3.14.1</b>\"' }
  version:
    with: { html: '${{ tasks.page_again.output }}' }
    invoke:
      tool: nika:jq
      args:
        input: ${{ with.html }}
        expression: '[match(\"([0-9.]+)\") | .captures[0].string][0]'
";

    #[test]
    fn a_filter_after_a_gated_producer_is_named_with_its_carrier() {
        let found = rows(UNGUARDED);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].reader, "version");
        assert_eq!(found[0].producer, "page_again");
        assert_eq!(found[0].carriers, vec!["html".to_owned()]);
        assert_eq!(found[0].reasons, vec!["its `when:` can close"]);
    }

    /// The repair the probe teaches: the reader tests the same gate, which the skip proved
    /// false, so it skips with its producer and is never named.
    #[test]
    fn a_reader_behind_the_same_gate_is_not_named() {
        let guarded = UNGUARDED.replace(
            "    with: { html: '${{ tasks.page_again.output }}' }\n",
            "    with: { html: '${{ tasks.page_again.output }}', go: '${{ tasks.changed.output }}' }\n    when: ${{ with.go == true }}\n",
        );
        assert_eq!(rows(&guarded), Vec::new());
    }

    /// A reader the skip cancels (it waits on the producer's success) never runs on its null.
    #[test]
    fn a_reader_waiting_on_the_producers_success_is_not_named() {
        let waiting = UNGUARDED.replace(
            "    with: { html: '${{ tasks.page_again.output }}' }\n",
            "    after: { page_again: success }\n    with: { html: '${{ tasks.page_again.output }}' }\n",
        );
        assert_eq!(rows(&waiting), Vec::new());
    }

    /// A value read beneath the output is `scan`'s, and a filter a template builds is unknown
    /// before the run: neither is named here.
    #[test]
    fn a_step_beneath_the_value_and_a_templated_filter_are_not_named() {
        let beneath = UNGUARDED.replace(
            "'${{ tasks.page_again.output }}' }",
            "'${{ tasks.page_again.output.body }}' }",
        );
        assert_eq!(rows(&beneath), Vec::new());
        let templated = UNGUARDED.replace(
            "expression: '[match(\"([0-9.]+)\") | .captures[0].string][0]'",
            "expression: '${{ with.html }}'",
        );
        assert_eq!(rows(&templated), Vec::new());
    }

    /// The live gauntlet r05 shape: the null rides one field of an object input, beside the
    /// values of producers that cannot skip.
    #[test]
    fn a_null_inside_an_object_input_is_named() {
        let yaml = "\
nika: skipped-in-object
permits:
  tools: [nika:jq]
tasks:
  count:
    invoke:
      tool: nika:jq
      args: { input: {}, expression: '0' }
  summarize:
    with: { n: '${{ tasks.count.output }}' }
    when: ${{ with.n > 0 }}
    invoke:
      tool: nika:jq
      args: { input: {}, expression: '[\"one line\"]' }
  journal:
    with:
      old: '${{ tasks.count.output }}'
      lines: '${{ tasks.summarize.output }}'
    invoke:
      tool: nika:jq
      args:
        input: { old: '${{ with.old }}', lines: '${{ with.lines }}' }
        expression: '[.lines[] | \"- \" + .] | join(\"\\n\")'
";
        let found = rows(yaml);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].reader.as_str(), found[0].producer.as_str()),
            ("journal", "summarize")
        );
        assert_eq!(found[0].carriers, vec!["lines".to_owned()]);
    }
}
