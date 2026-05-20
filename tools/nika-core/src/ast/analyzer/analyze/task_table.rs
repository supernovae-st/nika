// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task table: building, validating, and reference checking.

use super::*;

/// Build the task table from raw tasks, detecting duplicates.
///
/// Collect prefixes declared in `include:` entries.
///
/// When a workflow uses `include: [{ path: ./lib/seo.nika.yaml, prefix: seo_ }]`,
/// task references like `$seo_generate_title` should not be flagged as unknown
/// during analysis — they'll be resolved after `expand_includes()` merges the
/// included tasks into the DAG.
pub(super) fn collect_include_prefixes(raw: &RawWorkflow, ctx: &mut AnalyzerContext) {
    if let Some(ref include) = raw.include {
        let mut seen = HashSet::new();
        for spec in &include.value {
            if let Some(ref prefix) = spec.value.prefix {
                if !seen.insert(prefix.value.clone()) {
                    ctx.add_error(AnalyzeError::new(
                        AnalyzeErrorKind::InvalidValue,
                        prefix.span,
                        format!("duplicate include prefix '{}'", prefix.value),
                    ));
                }
                ctx.include_prefixes.push(prefix.value.clone());
            }
        }
    }
}

/// Shared between `validate()` and `analyze()`.
pub(super) fn build_task_table(tasks: &[Spanned<RawTask>], ctx: &mut AnalyzerContext) {
    for task in tasks.iter() {
        let task_name = &task.value.id.value;
        let task_span = task.value.id.span;

        // Validate task ID format before inserting
        if !validate_task_id(task_name, task_span, ctx) {
            continue;
        }

        if let Some(first_span) = ctx.task_spans.get(task_name) {
            ctx.add_error(AnalyzeError::duplicate_task(
                task_span,
                task_name,
                *first_span,
            ));
        } else {
            ctx.task_table.insert(task_name);
            ctx.task_spans.insert(task_name.clone(), task_span);
        }
    }
}

/// Validate task ID format: non-empty, alphanumeric with hyphens/underscores/dots,
/// must not start with `$` (reserved for binding references).
pub(super) fn validate_task_id(name: &str, span: Span, ctx: &mut AnalyzerContext) -> bool {
    if name.is_empty() {
        ctx.add_error(AnalyzeError::new(
            AnalyzeErrorKind::InvalidValue,
            span,
            "task ID must not be empty",
        ));
        return false;
    }
    if name.starts_with('$') {
        ctx.add_error(
            AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                span,
                format!(
                    "task ID '{}' must not start with '$' (reserved for binding references)",
                    name
                ),
            )
            .with_suggestion("remove the leading '$' from the task ID"),
        );
        return false;
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        ctx.add_error(
            AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                span,
                format!("task ID '{}' contains invalid characters", name),
            )
            .with_suggestion("use only alphanumeric characters, hyphens, underscores, and dots"),
        );
        return false;
    }
    true
}

/// Validate task references without building analyzed tasks.
///
/// Checks `with:` bindings for unknown task references and invalid expressions,
/// and `depends_on:` for unknown task references. Used by `validate()`.
pub(super) fn validate_task_refs(
    raw: &RawTask,
    task_table: &TaskTable,
    all_task_names: &[String],
    ctx: &mut AnalyzerContext,
) {
    // Validate with: bindings
    if let Some(ref with_refs) = raw.with_refs {
        for (_alias_spanned, value_spanned) in with_refs.value.iter() {
            let expr = &value_spanned.value;

            match parse_with_entry(expr) {
                Ok(entry) => {
                    // Check for unknown task references
                    if let Some(dep_task_name) = entry.task_id() {
                        if task_table.get_id(dep_task_name).is_none()
                            && !ctx.is_included_task(dep_task_name)
                            && dep_task_name != crate::ast::PARENT_CONTEXT_BINDING
                        {
                            // Check if this is a for_each loop variable
                            let is_loop_var = raw.for_each.as_ref().is_some_and(|fe| {
                                fe.value
                                    .as_var
                                    .as_ref()
                                    .is_some_and(|v| v.value == dep_task_name)
                            });

                            if is_loop_var {
                                let mut err = AnalyzeError::new(
                                    AnalyzeErrorKind::UnknownTask,
                                    value_spanned.span,
                                    format!(
                                        "'{}' is a for_each loop variable, not a task reference. \
                                         Access it as {{{{with.{}}}}} in templates",
                                        dep_task_name, dep_task_name
                                    ),
                                );
                                err = err.with_suggestion(format!(
                                    "remove '${}' from with: — loop variables are auto-injected",
                                    dep_task_name
                                ));
                                ctx.add_error(err);
                            } else {
                                let all_names: Vec<&str> =
                                    all_task_names.iter().map(|s| s.as_str()).collect();
                                let suggestion = find_similar(dep_task_name, &all_names, 0.6);
                                ctx.add_error(AnalyzeError::unknown_task(
                                    value_spanned.span,
                                    dep_task_name,
                                    suggestion.as_deref(),
                                ));
                            }
                        }
                    }
                }
                Err(parse_err) => {
                    ctx.add_error(AnalyzeError::invalid_binding(
                        value_spanned.span,
                        expr,
                        &parse_err.reason,
                    ));
                }
            }
        }
    }

    // Validate depends_on: references
    if let Some(ref depends_on) = raw.depends_on {
        for dep_spanned in &depends_on.value {
            let dep_name = &dep_spanned.value;
            if task_table.get_id(dep_name).is_none() && !ctx.is_included_task(dep_name) {
                let all_names: Vec<&str> = all_task_names.iter().map(|s| s.as_str()).collect();
                let suggestion = find_similar(dep_name, &all_names, 0.6);
                ctx.add_error(AnalyzeError::unknown_task(
                    dep_spanned.span,
                    dep_name,
                    suggestion.as_deref(),
                ));
            }
        }
    }
}
