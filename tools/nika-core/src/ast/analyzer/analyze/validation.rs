// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Validation functions: model checking, infer fields, template refs, schema, feature gates.

use super::*;

/// Warn if a model name is not recognized.
///
/// Skips models with `org/model` format (custom endpoints) and known provider defaults.
pub(super) fn check_model_name(model: &Spanned<String>, ctx: &mut AnalyzerContext) {
    if !model.value.contains('/')
        && ModelResolver::infer_provider_from_model(&model.value).is_none()
    {
        let is_known_default = crate::catalogs::resolver::PROVIDER_DEFAULTS
            .iter()
            .any(|(_, m)| *m == model.value.as_str());
        if !is_known_default {
            ctx.add_warning(AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                model.span,
                format!(
                    "model '{}' is not a recognized model name. \
                     Known prefixes: claude, gpt, gemini, mistral, deepseek, grok. \
                     Custom models should use 'org/model' format.",
                    model.value
                ),
            ));
        }
    }
}

/// Validate infer-specific fields (temperature, thinking_budget).
pub(super) fn check_infer_fields(infer: &RawInferAction, ctx: &mut AnalyzerContext) {
    if let Some(ref temp) = infer.temperature {
        // Only validate concrete values; template expressions are resolved at runtime
        if let Templatable::Value(v) = &temp.value {
            if *v < 0.0 || *v > 2.0 {
                ctx.add_warning(AnalyzeError::new(
                    AnalyzeErrorKind::InvalidValue,
                    temp.span,
                    format!(
                        "temperature: {} is outside the valid range [0.0, 2.0]. \
                         Most providers reject values outside this range.",
                        v
                    ),
                ));
            }
        }
    }

    if let Some(ref budget) = infer.thinking_budget {
        let thinking_enabled = infer
            .extended_thinking
            .as_ref()
            .and_then(|s| s.value.as_value().copied())
            .unwrap_or(false);
        if !thinking_enabled {
            ctx.add_warning(AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                budget.span,
                "thinking_budget: is set but extended_thinking: true is missing. \
                 The budget will be ignored without extended_thinking: true."
                    .to_string(),
            ));
        }
    }
}

/// Validate agent-specific infer fields (temperature, thinking_budget).
pub(super) fn check_agent_infer_fields(agent: &RawAgentAction, ctx: &mut AnalyzerContext) {
    if let Some(ref temp) = agent.temperature {
        // Only validate concrete values; template expressions are resolved at runtime
        if let Templatable::Value(v) = &temp.value {
            if *v < 0.0 || *v > 2.0 {
                ctx.add_warning(AnalyzeError::new(
                    AnalyzeErrorKind::InvalidValue,
                    temp.span,
                    format!(
                        "temperature: {} is outside the valid range [0.0, 2.0]. \
                         Most providers reject values outside this range.",
                        v
                    ),
                ));
            }
        }
    }

    if let Some(ref budget) = agent.thinking_budget {
        let thinking_enabled = agent
            .extended_thinking
            .as_ref()
            .and_then(|s| s.value.as_value().copied())
            .unwrap_or(false);
        if !thinking_enabled {
            ctx.add_warning(AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                budget.span,
                "thinking_budget: is set but extended_thinking: true is missing. \
                 The budget will be ignored without extended_thinking: true."
                    .to_string(),
            ));
        }
    }
}

/// Extract template references for a given namespace from a string.
///
/// Scans for `{{...namespace.key...}}` patterns and returns the first-level key.
/// Example: `"Hello {{inputs.topic}} and {{inputs.locale | upper}}"` with `"inputs"`
///          → `["topic", "locale"]`
pub(super) fn extract_template_refs<'a>(text: &'a str, namespace: &str) -> Vec<&'a str> {
    let prefix = format!("{}.", namespace);
    let mut refs = Vec::new();
    let bytes = text.as_bytes();
    let mut pos = 0;
    while pos + 1 < bytes.len() {
        if bytes[pos] == b'{' && bytes[pos + 1] == b'{' {
            if let Some(end_offset) = text[pos..].find("}}") {
                let inner = &text[pos + 2..pos + end_offset];
                let trimmed = inner.trim_start();
                if let Some(ns_pos) = trimmed.find(prefix.as_str()) {
                    let key_start = ns_pos + prefix.len();
                    let key_end = trimmed[key_start..]
                        .find(|c: char| !c.is_alphanumeric() && c != '_')
                        .map(|p| key_start + p)
                        .unwrap_or(trimmed.len());
                    if key_end > key_start {
                        let key = &trimmed[key_start..key_end];
                        if !refs.contains(&key) {
                            refs.push(key);
                        }
                    }
                }
                pos += end_offset + 2;
            } else {
                break;
            }
        } else {
            pos += 1;
        }
    }
    refs
}

/// Collect all template-bearing strings from a raw task and its action.
pub(super) fn collect_raw_task_templates(task: &RawTask) -> Vec<(&str, Span)> {
    let mut templates = Vec::new();

    if let Some(ref when) = task.when {
        templates.push((when.value.as_str(), when.span));
    }

    if let Some(ref action) = task.action {
        match action {
            RawTaskAction::Infer(ref infer) => {
                templates.push((infer.value.prompt.value.as_str(), infer.value.prompt.span));
                if let Some(ref sys) = infer.value.system {
                    templates.push((sys.value.as_str(), sys.span));
                }
                if let Some(ref content) = infer.value.content {
                    for part in &content.value {
                        if let crate::ast::content::RawContentPart::Text { ref text } = part {
                            templates.push((text.value.as_str(), text.span));
                        }
                    }
                }
            }
            RawTaskAction::Agent(ref agent) => {
                templates.push((agent.value.prompt.value.as_str(), agent.value.prompt.span));
                if let Some(ref sys) = agent.value.system {
                    templates.push((sys.value.as_str(), sys.span));
                }
            }
            RawTaskAction::Exec(ref exec) => {
                templates.push((exec.value.command.value.as_str(), exec.value.command.span));
            }
            RawTaskAction::Fetch(ref fetch) => {
                templates.push((fetch.value.url.value.as_str(), fetch.value.url.span));
                if let Some(ref body) = fetch.value.body {
                    templates.push((body.value.as_str(), body.span));
                }
            }
            RawTaskAction::Invoke(_) => {}
        }
    }

    templates
}

/// Validate template references against declared inputs, context files, and with bindings.
///
/// - G5: Warn when `{{inputs.X}}` references X not declared in `inputs:`.
/// - G6: Warn when `{{context.X}}` references X not declared in `context: files:`.
/// - G4: Warn when vision content text parts reference `{{with.X}}` not in `with:`.
pub(super) fn validate_template_refs(raw: &RawWorkflow, ctx: &mut AnalyzerContext) {
    let input_keys: Vec<String> = raw
        .inputs
        .as_ref()
        .map(|inputs| inputs.value.keys().map(|k| k.value.clone()).collect())
        .unwrap_or_default();

    let context_aliases: Vec<String> = raw
        .context
        .as_ref()
        .and_then(|c| c.value.files.as_ref())
        .map(|files| files.keys().map(|k| k.value.clone()).collect())
        .unwrap_or_default();

    for raw_task in &raw.tasks.value {
        let task = &raw_task.value;
        let templates = collect_raw_task_templates(task);

        // G5: inputs.X references
        for (text, span) in &templates {
            for key in extract_template_refs(text, "inputs") {
                if !input_keys.iter().any(|k| k == key) {
                    ctx.add_warning(AnalyzeError::new(
                        AnalyzeErrorKind::InvalidBinding,
                        *span,
                        format!(
                            "Task '{}' references {{{{inputs.{}}}}} but '{}' is not declared \
                             in the workflow inputs: block.{}",
                            task.id.value,
                            key,
                            key,
                            if input_keys.is_empty() {
                                " No inputs are declared.".to_string()
                            } else {
                                format!(" Declared inputs: {}", input_keys.join(", "))
                            }
                        ),
                    ));
                }
            }
        }

        // G6: context.X references
        for (text, span) in &templates {
            for key in extract_template_refs(text, "context") {
                if !context_aliases.iter().any(|a| a == key) {
                    ctx.add_warning(AnalyzeError::new(
                        AnalyzeErrorKind::InvalidBinding,
                        *span,
                        format!(
                            "Task '{}' references {{{{context.{}}}}} but '{}' is not declared \
                             in the workflow context: files: block.{}",
                            task.id.value,
                            key,
                            key,
                            if context_aliases.is_empty() {
                                " No context files are declared.".to_string()
                            } else {
                                format!(" Declared context files: {}", context_aliases.join(", "))
                            }
                        ),
                    ));
                }
            }
        }

        // G4: with.X references in vision content text parts
        if let Some(RawTaskAction::Infer(ref infer)) = task.action {
            if let Some(ref content) = infer.value.content {
                let with_aliases: Vec<String> = task
                    .with_refs
                    .as_ref()
                    .map(|w| w.value.keys().map(|k| k.value.clone()).collect())
                    .unwrap_or_default();

                for part in &content.value {
                    if let crate::ast::content::RawContentPart::Text { ref text } = part {
                        for key in extract_template_refs(&text.value, "with") {
                            if !with_aliases.iter().any(|a| a == key) {
                                ctx.add_warning(AnalyzeError::new(
                                    AnalyzeErrorKind::InvalidBinding,
                                    text.span,
                                    format!(
                                        "Task '{}' vision content references \
                                         {{{{with.{}}}}} but '{}' is not declared in \
                                         the task's with: block.{}",
                                        task.id.value,
                                        key,
                                        key,
                                        if with_aliases.is_empty() {
                                            " No with: bindings are declared.".to_string()
                                        } else {
                                            format!(
                                                " Declared bindings: {}",
                                                with_aliases.join(", ")
                                            )
                                        }
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Analyze and validate the schema version.
pub(super) fn analyze_schema(
    raw: &RawWorkflow,
    ctx: &mut AnalyzerContext,
) -> Option<SchemaVersion> {
    let schema_str = &raw.schema.value;

    if let Some(version) = SchemaVersion::parse(schema_str) {
        Some(version)
    } else {
        // Find similar schema version
        let all_versions: Vec<&str> = SchemaVersion::all().iter().map(|v| v.as_str()).collect();
        let suggestion = find_similar(schema_str, &all_versions, 0.6);

        ctx.add_error(AnalyzeError::invalid_schema(
            raw.schema.span,
            schema_str,
            suggestion.as_deref(),
        ));
        None
    }
}

/// Validate version-gated features.
///
/// Checks that features used in the workflow are available in the declared schema version.
pub(super) fn validate_feature_gates(
    raw: &RawWorkflow,
    version: SchemaVersion,
    ctx: &mut AnalyzerContext,
) {
    let version_str = version.as_str();

    // Check MCP servers
    if let Some(ref mcp) = raw.mcp {
        if !version.supports_mcp() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                mcp.span,
                "mcp",
                version_str,
                "nika/workflow@0.2",
            ));
        }
    }

    // Check context files
    if let Some(ref context) = raw.context {
        if !version.supports_context() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                context.span,
                "context",
                version_str,
                "nika/workflow@0.9",
            ));
        }
    }

    // Check include
    if let Some(ref include) = raw.include {
        if !version.supports_includes() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                include.span,
                "include",
                version_str,
                "nika/workflow@0.12",
            ));
        }
    }

    // Check inputs
    if let Some(ref inputs) = raw.inputs {
        if !version.supports_inputs() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                inputs.span,
                "inputs",
                version_str,
                "nika/workflow@0.10",
            ));
        }
    }

    // Check task-level features
    for task in raw.tasks.value.iter() {
        validate_task_feature_gates(&task.value, version, version_str, ctx);
    }
}

/// Validate version-gated features in a task.
pub(super) fn validate_task_feature_gates(
    task: &RawTask,
    version: SchemaVersion,
    version_str: &str,
    ctx: &mut AnalyzerContext,
) {
    // Check for_each
    if let Some(ref for_each) = task.for_each {
        if !version.supports_for_each() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                for_each.span,
                "for_each",
                version_str,
                "nika/workflow@0.3",
            ));
        }
    }

    // Check retry
    if let Some(ref retry) = task.retry {
        if !version.supports_retry() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                retry.span,
                "retry",
                version_str,
                "nika/workflow@0.3",
            ));
        }

        // retry: is valid on ALL verbs (fetch, infer, exec, invoke, agent).
        // Fetch handles retry internally in its executor; other verbs use
        // runner-level retry wrapper.
    }

    // Check with: bindings
    if let Some(ref with_refs) = task.with_refs {
        if !version.supports_with() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                with_refs.span,
                "with",
                version_str,
                "nika/workflow@0.12",
            ));
        }
    }

    // Check depends_on:
    if let Some(ref depends_on) = task.depends_on {
        if !version.supports_depends_on() {
            ctx.add_error(AnalyzeError::unsupported_feature(
                depends_on.span,
                "depends_on",
                version_str,
                "nika/workflow@0.12",
            ));
        }
    }

    // Check invoke/agent verbs
    if let Some(ref action) = task.action {
        match action {
            RawTaskAction::Invoke(invoke) => {
                if !version.supports_invoke_agent() {
                    ctx.add_error(AnalyzeError::unsupported_feature(
                        invoke.span,
                        "invoke verb",
                        version_str,
                        "nika/workflow@0.2",
                    ));
                }
            }
            RawTaskAction::Agent(agent) => {
                if !version.supports_invoke_agent() {
                    ctx.add_error(AnalyzeError::unsupported_feature(
                        agent.span,
                        "agent verb",
                        version_str,
                        "nika/workflow@0.2",
                    ));
                }
            }
            _ => {}
        }
    }
}
