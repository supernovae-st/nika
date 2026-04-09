// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika check` and `nika check --strict` command handlers.
//!
//! Implements multi-phase pre-flight validation:
//! schema → parse → includes → DAG → bindings → schemas → security → providers
//! The `--strict` mode additionally connects to MCP servers and validates invoke
//! parameter schemas against live tool definitions.

use std::path::Path;

use nika_engine::ast::output::SchemaRef;
use nika_engine::ast::schema_validator::WorkflowSchemaValidator;
use nika_engine::ast::{parse_analyzed, parse_workflow_with_includes, TaskAction};
use nika_engine::dag::{validate_bindings, Dag};
use nika_engine::error::NikaError;
use nika_engine::mcp::validation::{McpValidator, ValidationConfig};
use nika_engine::mcp::{McpClient, McpConfig};

use crate::discover::resolve_workflow_path;

/// Run `nika check` — multi-phase pre-flight validation without connecting to MCP servers.
pub async fn validate_workflow(file: &str, quiet: bool, security: bool) -> Result<(), NikaError> {
    use nika_engine::display::{
        print_check_header, print_check_summary, print_check_warnings, print_phase,
        print_phase_skipped, PhaseResult,
    };
    use std::time::Instant;

    let total_start = Instant::now();
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    // Phase 1: Schema validation
    let t = Instant::now();
    let validator = WorkflowSchemaValidator::new()?;
    validator.validate_yaml(&yaml)?;
    let schema_elapsed = t.elapsed();

    // Phase 2: Parse (decomposed to capture analyzer warnings)
    let t = Instant::now();
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let raw = nika_engine::ast::raw::parse(&yaml, nika_engine::source::FileId(0)).map_err(|e| {
        NikaError::ParseError {
            details: format!("[{}] {}", e.kind.code(), e.message),
        }
    })?;
    let raw = nika_engine::ast::expand_raw_include(raw, base_path)?;
    let analyze_result = nika_engine::ast::analyzer::analyze(raw);

    // Capture warnings before into_result() drops them
    let analyzer_warnings: Vec<String> = analyze_result
        .warnings
        .iter()
        .map(|w| {
            let code = w.kind.code();
            if let Some(ref suggestion) = w.suggestion {
                format!("[{}] {} ({})", code, w.message, suggestion)
            } else {
                format!("[{}] {}", code, w.message)
            }
        })
        .collect();

    let analyzed = analyze_result.into_result().map_err(|errors| {
        let messages: Vec<String> = errors
            .iter()
            .map(|e| format!("[{}] {}", e.kind.code(), e))
            .collect();
        NikaError::ValidationError {
            reason: messages.join("; "),
        }
    })?;

    // Run taint analysis on the analyzed AST (before lowering consumes it)
    let taint_report = if security {
        use nika_engine::trust::InvocationSource;
        Some(nika_engine::ast::analyzer::taint::TaintAnalyzer::analyze(
            &analyzed,
            InvocationSource::Cli,
        ))
    } else {
        None
    };

    let workflow = nika_engine::ast::lower::lower(analyzed)?;
    let parse_elapsed = t.elapsed();
    let includes_elapsed = std::time::Duration::ZERO;

    // Phase 4: DAG
    let t = Instant::now();
    let flow_graph = Dag::from_workflow(&workflow)?;
    let dag_cycle_result = flow_graph.detect_cycles();
    let dag_elapsed = t.elapsed();

    if let Err(ref e) = dag_cycle_result {
        if !quiet {
            print_check_header(file, false, env!("CARGO_PKG_VERSION"));
            print_phase(&PhaseResult {
                name: "schema",
                passed: true,
                detail: format!("YAML valid against @{}", workflow.schema),
                duration_ms: schema_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
            print_phase(&PhaseResult {
                name: "parse",
                passed: true,
                detail: format!(
                    "{} tasks \u{00B7} provider: {} \u{00B7} model: {}",
                    workflow.tasks.len(),
                    workflow.provider,
                    workflow.model.as_deref().unwrap_or("(default)")
                ),
                duration_ms: parse_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
            print_phase(&PhaseResult {
                name: "includes",
                passed: true,
                detail: "resolved".to_string(),
                duration_ms: includes_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
            print_phase(&PhaseResult {
                name: "dag",
                passed: false,
                detail: "CYCLE DETECTED".to_string(),
                duration_ms: dag_elapsed.as_millis() as u64,
                errors: vec![e.to_string()],
                hints: vec![
                    "Remove one dependency to break the cycle.".to_string(),
                    "Common fix: use with: binding instead of depends_on.".to_string(),
                ],
            });
            print_phase_skipped("bindings", "DAG invalid");
            print_phase_skipped("schemas", "DAG invalid");
            println!();
            print_check_summary(
                false,
                total_start.elapsed().as_millis() as u64,
                workflow.tasks.len(),
                workflow.flow_count(),
                0,
                0,
                None,
                &[("NIKA-020", "Circular dependency detected")],
            );
        }
        return dag_cycle_result;
    }

    // Phase 5: Bindings
    let t = Instant::now();
    validate_bindings(&workflow, &flow_graph)?;
    let bindings_elapsed = t.elapsed();

    // Phase 6: Validate structured output schema files
    let t = Instant::now();
    let mut schema_count = 0u32;
    for task in &workflow.tasks {
        // Check output.schema file references
        if let Some(ref output) = task.output {
            if let Some(SchemaRef::File(ref path)) = output.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
        // Check structured.schema file references
        if let Some(ref spec) = task.structured {
            if let Some(SchemaRef::File(ref path)) = spec.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
    }
    let schemas_elapsed = t.elapsed();

    // Phase 7: Security hints (shell escape warnings)
    let t = Instant::now();
    let mut security_hints: Vec<String> = Vec::new();
    {
        let binding_re = regex::Regex::new(r"\{\{(with\.[^}]+|inputs\.[^}]+)\}\}").unwrap();
        let shell_guard_re = regex::Regex::new(r"\|\s*shell\b").unwrap();
        for task in &workflow.tasks {
            if let TaskAction::Exec { exec } = &task.action {
                if exec.shell == Some(true) {
                    for cap in binding_re.captures_iter(&exec.command) {
                        let inner = &cap[1];
                        if !shell_guard_re.is_match(inner) {
                            security_hints.push(format!(
                                "task '{}': shell: true with unescaped {{{{{}}}}} — use | shell transform",
                                task.id, inner
                            ));
                        }
                    }
                }
            }
        }
    }
    let security_elapsed = t.elapsed();

    // Phase 8: Provider API keys (BUG 6 / NIKA-032)
    let t = Instant::now();
    let mut provider_warnings: Vec<String> = Vec::new();
    {
        let mut providers_used: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        providers_used.insert(workflow.provider.to_string());

        // Collect per-task providers from analyzed AST
        if let Ok(analyzed) = parse_analyzed(&yaml) {
            for task in &analyzed.tasks {
                if let Some(ref p) = task.provider {
                    providers_used.insert(p.to_string());
                }
            }
        }

        for provider_name in &providers_used {
            if let Some(provider) = nika_engine::core::find_provider(provider_name) {
                if provider.requires_key && !nika_engine::secrets::has_provider_key(provider) {
                    provider_warnings.push(format!(
                        "{} not set (provider '{}' used in workflow)",
                        provider.env_var, provider_name
                    ));
                }
            }
        }
    }
    let providers_elapsed = t.elapsed();

    if !quiet {
        print_check_header(file, false, env!("CARGO_PKG_VERSION"));

        // Phase 1: Schema
        print_phase(&PhaseResult {
            name: "schema",
            passed: true,
            detail: format!("YAML valid against @{}", workflow.schema),
            duration_ms: schema_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 2: Parse
        print_phase(&PhaseResult {
            name: "parse",
            passed: true,
            detail: format!(
                "{} tasks \u{00B7} provider: {} \u{00B7} model: {}",
                workflow.tasks.len(),
                workflow.provider,
                workflow.model.as_deref().unwrap_or("(default)")
            ),
            duration_ms: parse_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 3: Includes
        print_phase(&PhaseResult {
            name: "includes",
            passed: true,
            detail: "resolved".to_string(),
            duration_ms: includes_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 4: DAG
        print_phase(&PhaseResult {
            name: "dag",
            passed: true,
            detail: format!("{} edges \u{00B7} acyclic", workflow.flow_count()),
            duration_ms: dag_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 5: Bindings
        print_phase(&PhaseResult {
            name: "bindings",
            passed: true,
            detail: "all references valid".to_string(),
            duration_ms: bindings_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 6: Schemas
        let schemas_detail = if schema_count > 0 {
            format!("{schema_count} validated")
        } else {
            "none required".to_string()
        };
        print_phase(&PhaseResult {
            name: "schemas",
            passed: true,
            detail: schemas_detail,
            duration_ms: schemas_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });

        // Phase 7: Security
        if security_hints.is_empty() {
            print_phase(&PhaseResult {
                name: "security",
                passed: true,
                detail: "no issues".to_string(),
                duration_ms: security_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
        } else {
            print_phase(&PhaseResult {
                name: "security",
                passed: true, // warnings, not errors
                detail: format!("{} hint(s)", security_hints.len()),
                duration_ms: security_elapsed.as_millis() as u64,
                errors: vec![],
                hints: security_hints,
            });
        }

        // Phase 8: Provider API keys
        if provider_warnings.is_empty() {
            print_phase(&PhaseResult {
                name: "providers",
                passed: true,
                detail: "all API keys present".to_string(),
                duration_ms: providers_elapsed.as_millis() as u64,
                errors: vec![],
                hints: vec![],
            });
        } else {
            print_phase(&PhaseResult {
                name: "providers",
                passed: false,
                detail: format!("{} missing API key(s)", provider_warnings.len()),
                duration_ms: providers_elapsed.as_millis() as u64,
                errors: provider_warnings.clone(),
                hints: vec!["Run 'nika keys set <name>' to configure API keys".to_string()],
            });
        }

        // Phase 9: Nika Shield taint analysis (--security flag)
        if let Some(ref report) = taint_report {
            let summary = report.trust_summary();
            let trusted = summary
                .get(&nika_engine::trust::TrustLevel::Trusted)
                .unwrap_or(&0);
            let generated = summary
                .get(&nika_engine::trust::TrustLevel::ModelGenerated)
                .unwrap_or(&0);
            let tainted = summary
                .get(&nika_engine::trust::TrustLevel::ModelTainted)
                .unwrap_or(&0);
            let untrusted = summary
                .get(&nika_engine::trust::TrustLevel::Untrusted)
                .unwrap_or(&0);

            if report.is_clean() {
                print_phase(&PhaseResult {
                    name: "shield",
                    passed: true,
                    detail: format!(
                        "{trusted} Trusted \u{00B7} {generated} ModelGenerated \u{00B7} {tainted} ModelTainted \u{00B7} {untrusted} Untrusted"
                    ),
                    duration_ms: 0,
                    errors: vec![],
                    hints: vec![],
                });
            } else {
                let warning_msgs: Vec<String> = report
                    .warnings
                    .iter()
                    .map(|w| {
                        format!(
                            "[{}] {}\n         Recommendation: {}",
                            w.code(),
                            w.message(),
                            w.recommendation()
                        )
                    })
                    .collect();
                print_phase(&PhaseResult {
                    name: "shield",
                    passed: true, // warnings, not hard errors
                    detail: format!(
                        "{} warning(s) \u{00B7} {trusted}T {generated}G {tainted}M {untrusted}U",
                        report.warnings.len()
                    ),
                    duration_ms: 0,
                    errors: vec![],
                    hints: warning_msgs,
                });
            }
        }

        // Show DAG visualization for multi-task workflows
        if workflow.tasks.len() > 1 {
            use nika_engine::display::{render_dag, DagTask, DagTaskStatus};
            use std::collections::HashMap;

            let dag_tasks: Vec<DagTask> = workflow
                .tasks
                .iter()
                .map(|t| DagTask {
                    id: t.id.clone(),
                    verb: t.action.verb_name().to_string(),
                    status: DagTaskStatus::Pending,
                    meta: None,
                    tags: Vec::new(),
                })
                .collect();

            let mut deps_map: HashMap<String, Vec<String>> = HashMap::new();
            for task in &workflow.tasks {
                if let Some(ref task_deps) = task.depends_on {
                    deps_map.insert(task.id.clone(), task_deps.clone());
                }
            }

            render_dag(&dag_tasks, &deps_map);
        }

        // Compute layer count for summary
        let layer_count = {
            let mut depths: std::collections::HashMap<&str, usize> =
                workflow.tasks.iter().map(|t| (t.id.as_str(), 0)).collect();
            let mut changed = true;
            let mut iters = 0;
            while changed && iters < 100 {
                changed = false;
                iters += 1;
                for task in &workflow.tasks {
                    if let Some(ref task_deps) = task.depends_on {
                        for dep in task_deps {
                            if let Some(&dep_depth) = depths.get(dep.as_str()) {
                                let new_depth = dep_depth + 1;
                                if new_depth > depths[task.id.as_str()] {
                                    depths.insert(&task.id, new_depth);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            depths.values().max().copied().unwrap_or(0) + 1
        };

        // Analyzer warnings (surfaced from analyze phase)
        print_check_warnings(&analyzer_warnings);

        // Summary footer
        println!();
        print_check_summary(
            true,
            total_start.elapsed().as_millis() as u64,
            workflow.tasks.len(),
            workflow.flow_count(),
            layer_count,
            schema_count,
            None,
            &[],
        );
    }

    Ok(())
}

/// Validate a schema file exists and contains valid JSON.
pub async fn validate_schema_file(
    task_id: &str,
    path: &str,
    base_path: &Path,
) -> Result<(), NikaError> {
    let resolved = base_path.join(path);
    if !resolved.exists() {
        return Err(NikaError::SchemaFileNotFound {
            task_id: task_id.to_string(),
            path: path.to_string(),
        });
    }

    let content =
        tokio::fs::read_to_string(&resolved)
            .await
            .map_err(|e| NikaError::SchemaFileNotFound {
                task_id: task_id.to_string(),
                path: format!("{path}: {e}"),
            })?;

    serde_json::from_str::<serde_json::Value>(&content).map_err(|e| {
        NikaError::SchemaFileInvalid {
            task_id: task_id.to_string(),
            path: path.to_string(),
            reason: e.to_string(),
        }
    })?;

    Ok(())
}

/// Run `nika check --strict` — connects to MCP servers and validates invoke parameter schemas.
pub async fn validate_workflow_strict(file: &str) -> Result<(), NikaError> {
    use nika_engine::display::{
        print_check_header, print_check_summary, print_mcp_validation, print_phase,
        print_phase_skipped, McpCallValidation, McpCheckResult, McpParamError, PhaseResult,
    };
    use std::time::Instant;

    let total_start = Instant::now();
    let resolved_path = resolve_workflow_path(file).await?;

    let yaml = tokio::fs::read_to_string(&resolved_path).await?;

    // Phase 1: Schema validation
    let t = Instant::now();
    let schema_validator = WorkflowSchemaValidator::new()?;
    schema_validator.validate_yaml(&yaml)?;
    let schema_elapsed = t.elapsed();

    // Phase 2: Parse
    let t = Instant::now();
    let base_path = resolved_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let workflow = parse_workflow_with_includes(&yaml, base_path)?;
    let parse_elapsed = t.elapsed();
    let includes_elapsed = std::time::Duration::ZERO;

    // Phase 4: DAG
    let t = Instant::now();
    let flow_graph = Dag::from_workflow(&workflow)?;
    let dag_cycle_result = flow_graph.detect_cycles();
    let dag_elapsed = t.elapsed();

    if let Err(ref e) = dag_cycle_result {
        print_check_header(file, true, env!("CARGO_PKG_VERSION"));
        print_phase(&PhaseResult {
            name: "schema",
            passed: true,
            detail: format!("YAML valid against @{}", workflow.schema),
            duration_ms: schema_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
        print_phase(&PhaseResult {
            name: "parse",
            passed: true,
            detail: format!(
                "{} tasks \u{00B7} provider: {} \u{00B7} model: {}",
                workflow.tasks.len(),
                workflow.provider,
                workflow.model.as_deref().unwrap_or("(default)")
            ),
            duration_ms: parse_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
        print_phase(&PhaseResult {
            name: "includes",
            passed: true,
            detail: "resolved".to_string(),
            duration_ms: includes_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
        print_phase(&PhaseResult {
            name: "dag",
            passed: false,
            detail: "CYCLE DETECTED".to_string(),
            duration_ms: dag_elapsed.as_millis() as u64,
            errors: vec![e.to_string()],
            hints: vec![
                "Remove one dependency to break the cycle.".to_string(),
                "Common fix: use with: binding instead of depends_on.".to_string(),
            ],
        });
        print_phase_skipped("bindings", "DAG invalid");
        print_phase_skipped("schemas", "DAG invalid");
        println!();
        print_check_summary(
            false,
            total_start.elapsed().as_millis() as u64,
            workflow.tasks.len(),
            workflow.flow_count(),
            0,
            0,
            None,
            &[("NIKA-020", "Circular dependency detected")],
        );
        return dag_cycle_result;
    }

    // Phase 5: Bindings
    let t = Instant::now();
    validate_bindings(&workflow, &flow_graph)?;
    let bindings_elapsed = t.elapsed();

    // Phase 6: Validate structured output schema files
    let t = Instant::now();
    let mut schema_count = 0u32;
    for task in &workflow.tasks {
        if let Some(ref output) = task.output {
            if let Some(SchemaRef::File(ref path)) = output.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
        if let Some(ref spec) = task.structured {
            if let Some(SchemaRef::File(ref path)) = spec.schema {
                validate_schema_file(&task.id, path, base_path).await?;
                schema_count += 1;
            }
        }
    }
    let schemas_elapsed = t.elapsed();

    // Phase 7: Security hints (shell escape warnings)
    let t = Instant::now();
    let mut strict_security_hints: Vec<String> = Vec::new();
    {
        let binding_re = regex::Regex::new(r"\{\{(with\.[^}]+|inputs\.[^}]+)\}\}").unwrap();
        let shell_guard_re = regex::Regex::new(r"\|\s*shell\b").unwrap();
        for task in &workflow.tasks {
            if let TaskAction::Exec { exec } = &task.action {
                if exec.shell == Some(true) {
                    for cap in binding_re.captures_iter(&exec.command) {
                        let inner = &cap[1];
                        if !shell_guard_re.is_match(inner) {
                            strict_security_hints.push(format!(
                                "task '{}': shell: true with unescaped {{{{{}}}}} — use | shell transform",
                                task.id, inner
                            ));
                        }
                    }
                }
            }
        }
    }
    let strict_security_elapsed = t.elapsed();

    // Phase 8: Provider API keys
    let t = Instant::now();
    let mut provider_warnings: Vec<String> = Vec::new();
    {
        let mut providers_used: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        providers_used.insert(workflow.provider.to_string());

        if let Ok(analyzed) = parse_analyzed(&yaml) {
            for task in &analyzed.tasks {
                if let Some(ref p) = task.provider {
                    providers_used.insert(p.to_string());
                }
            }
        }

        for provider_name in &providers_used {
            if let Some(provider) = nika_engine::core::find_provider(provider_name) {
                if provider.requires_key && !nika_engine::secrets::has_provider_key(provider) {
                    provider_warnings.push(format!(
                        "{} not set (provider '{}' used in workflow)",
                        provider.env_var, provider_name
                    ));
                }
            }
        }
    }
    let providers_elapsed = t.elapsed();

    // MCP parameter validation (strict mode)
    let invoke_tasks: Vec<_> = workflow
        .tasks
        .iter()
        .filter_map(|t| {
            if let TaskAction::Invoke { invoke: ref params } = t.action {
                Some((t.id.as_str(), params))
            } else {
                None
            }
        })
        .collect();

    // Also collect agent tasks that reference MCP servers
    let agent_tasks: Vec<(&str, Vec<String>)> = workflow
        .tasks
        .iter()
        .filter_map(|t| {
            if let TaskAction::Agent { agent: ref params } = t.action {
                if !params.mcp.is_empty() {
                    Some((t.id.as_str(), params.mcp.clone()))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    let mut mcp_results: Vec<McpCheckResult> = Vec::new();
    let mut all_valid = true;
    let mut total_calls = 0u32;
    let mut valid_calls = 0u32;
    let mut total_param_errors = 0u32;

    if !invoke_tasks.is_empty() || !agent_tasks.is_empty() {
        let mcp_validator = McpValidator::new(ValidationConfig::default());

        let mut mcp_servers: std::collections::HashSet<&str> = invoke_tasks
            .iter()
            .filter_map(|(_, p)| p.mcp.as_deref())
            .collect();

        // Add MCP servers referenced by agent tasks
        for (_, servers) in &agent_tasks {
            for server in servers {
                mcp_servers.insert(server.as_str());
            }
        }

        let mcp_configs = workflow
            .mcp
            .as_ref()
            .ok_or_else(|| NikaError::ValidationError {
                reason: "Workflow has invoke tasks but no mcp: configuration".to_string(),
            })?;

        for server_name in mcp_servers {
            let Some(inline_config) = mcp_configs.get::<str>(server_name) else {
                return Err(NikaError::McpNotConnected {
                    name: server_name.to_string(),
                });
            };

            let connect_start = Instant::now();

            let mut config = McpConfig::new(server_name, &inline_config.command)
                .with_args(inline_config.args.iter().cloned());
            for (key, value) in &inline_config.env {
                config = config.with_env(key, value);
            }
            if let Some(ref cwd) = inline_config.cwd {
                config = config.with_cwd(cwd);
            }

            let client = McpClient::new(config)?;
            client.connect().await?;

            let tools = client.list_tools().await?;
            let connect_ms = connect_start.elapsed().as_millis() as u64;

            mcp_validator.cache().populate(server_name, &tools)?;

            // Validate invoke tasks targeting this server
            let mut validations: Vec<McpCallValidation> = Vec::new();
            for (task_id, params) in &invoke_tasks {
                if params.mcp.as_deref() != Some(server_name) {
                    continue;
                }
                total_calls += 1;

                if let Some(ref tool) = params.tool {
                    let invoke_params = params.params.clone().unwrap_or_default();
                    let result = mcp_validator.validate(server_name, tool, &invoke_params);

                    if result.is_valid {
                        valid_calls += 1;
                        validations.push(McpCallValidation {
                            task_id: task_id.to_string(),
                            tool_name: tool.clone(),
                            valid: true,
                            errors: vec![],
                        });
                    } else {
                        all_valid = false;
                        let errors: Vec<McpParamError> = result
                            .errors
                            .iter()
                            .map(|e| McpParamError {
                                path: e.path.clone(),
                                message: e.message.clone(),
                            })
                            .collect();
                        total_param_errors += errors.len() as u32;
                        validations.push(McpCallValidation {
                            task_id: task_id.to_string(),
                            tool_name: tool.clone(),
                            valid: false,
                            errors,
                        });
                    }
                } else {
                    // Resource read -- no params to validate
                    valid_calls += 1;
                    validations.push(McpCallValidation {
                        task_id: task_id.to_string(),
                        tool_name: "(resource read)".to_string(),
                        valid: true,
                        errors: vec![],
                    });
                }
            }

            // Agent tasks — connectivity validated, tools are dynamic
            for (task_id, servers) in &agent_tasks {
                if servers.iter().any(|s| s.as_str() == server_name) {
                    total_calls += 1;
                    valid_calls += 1;
                    validations.push(McpCallValidation {
                        task_id: task_id.to_string(),
                        tool_name: "(agent: dynamic tools)".to_string(),
                        valid: true,
                        errors: vec![],
                    });
                }
            }

            mcp_results.push(McpCheckResult {
                server_name: server_name.to_string(),
                tool_count: tools.len(),
                connect_ms,
                validations,
            });
        }
    }

    // Print all output
    print_check_header(file, true, env!("CARGO_PKG_VERSION"));

    // Phase 1: Schema
    print_phase(&PhaseResult {
        name: "schema",
        passed: true,
        detail: format!("YAML valid against @{}", workflow.schema),
        duration_ms: schema_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 2: Parse
    print_phase(&PhaseResult {
        name: "parse",
        passed: true,
        detail: format!(
            "{} tasks \u{00B7} provider: {} \u{00B7} model: {}",
            workflow.tasks.len(),
            workflow.provider,
            workflow.model.as_deref().unwrap_or("(default)")
        ),
        duration_ms: parse_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 3: Includes
    print_phase(&PhaseResult {
        name: "includes",
        passed: true,
        detail: "resolved".to_string(),
        duration_ms: includes_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 4: DAG
    print_phase(&PhaseResult {
        name: "dag",
        passed: true,
        detail: format!("{} edges \u{00B7} acyclic", workflow.flow_count()),
        duration_ms: dag_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 5: Bindings
    print_phase(&PhaseResult {
        name: "bindings",
        passed: true,
        detail: "all references valid".to_string(),
        duration_ms: bindings_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 6: Schemas
    let schemas_detail = if schema_count > 0 {
        format!("{schema_count} validated")
    } else {
        "none required".to_string()
    };
    print_phase(&PhaseResult {
        name: "schemas",
        passed: true,
        detail: schemas_detail,
        duration_ms: schemas_elapsed.as_millis() as u64,
        errors: vec![],
        hints: vec![],
    });

    // Phase 7: Security
    if strict_security_hints.is_empty() {
        print_phase(&PhaseResult {
            name: "security",
            passed: true,
            detail: "no issues".to_string(),
            duration_ms: strict_security_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
    } else {
        print_phase(&PhaseResult {
            name: "security",
            passed: true, // warnings, not errors
            detail: format!("{} hint(s)", strict_security_hints.len()),
            duration_ms: strict_security_elapsed.as_millis() as u64,
            errors: vec![],
            hints: strict_security_hints,
        });
    }

    // Phase 8: Provider API keys
    if provider_warnings.is_empty() {
        print_phase(&PhaseResult {
            name: "providers",
            passed: true,
            detail: "all API keys present".to_string(),
            duration_ms: providers_elapsed.as_millis() as u64,
            errors: vec![],
            hints: vec![],
        });
    } else {
        print_phase(&PhaseResult {
            name: "providers",
            passed: false,
            detail: format!("{} missing API key(s)", provider_warnings.len()),
            duration_ms: providers_elapsed.as_millis() as u64,
            errors: provider_warnings,
            hints: vec!["Run 'nika keys set <name>' to configure API keys".to_string()],
        });
    }

    // MCP Validation section
    if !mcp_results.is_empty() {
        print_mcp_validation(&mcp_results);
    }

    // Show DAG visualization for multi-task workflows
    if workflow.tasks.len() > 1 {
        use nika_engine::display::{render_dag, DagTask, DagTaskStatus};
        use std::collections::HashMap;

        // Build a set of task IDs that failed MCP validation
        let failed_task_ids: std::collections::HashSet<String> = mcp_results
            .iter()
            .flat_map(|r| &r.validations)
            .filter(|v| !v.valid)
            .map(|v| v.task_id.clone())
            .collect();

        let dag_tasks: Vec<DagTask> = workflow
            .tasks
            .iter()
            .map(|t| {
                let status = if failed_task_ids.contains(&t.id) {
                    DagTaskStatus::Failed
                } else if invoke_tasks.iter().any(|(id, _)| *id == t.id)
                    || agent_tasks.iter().any(|(id, _)| *id == t.id)
                {
                    DagTaskStatus::Success
                } else {
                    DagTaskStatus::Pending
                };
                DagTask {
                    id: t.id.clone(),
                    verb: t.action.verb_name().to_string(),
                    status,
                    meta: None,
                    tags: Vec::new(),
                }
            })
            .collect();

        let mut deps_map: HashMap<String, Vec<String>> = HashMap::new();
        for task in &workflow.tasks {
            if let Some(ref task_deps) = task.depends_on {
                deps_map.insert(task.id.clone(), task_deps.clone());
            }
        }

        render_dag(&dag_tasks, &deps_map);
    }

    // Compute layer count for summary
    let layer_count = {
        let mut depths: std::collections::HashMap<&str, usize> =
            workflow.tasks.iter().map(|t| (t.id.as_str(), 0)).collect();
        let mut changed = true;
        let mut iters = 0;
        while changed && iters < 100 {
            changed = false;
            iters += 1;
            for task in &workflow.tasks {
                if let Some(ref task_deps) = task.depends_on {
                    for dep in task_deps {
                        if let Some(&dep_depth) = depths.get(dep.as_str()) {
                            let new_depth = dep_depth + 1;
                            if new_depth > depths[task.id.as_str()] {
                                depths.insert(&task.id, new_depth);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        depths.values().max().copied().unwrap_or(0) + 1
    };

    // Build error codes for summary
    let mut error_codes: Vec<(&str, &str)> = Vec::new();
    if !all_valid {
        error_codes.push((
            "NIKA-100",
            "Strict validation failed: invoke parameter mismatch",
        ));
    }

    // Strict info for summary
    let strict_info = if total_calls > 0 {
        Some((valid_calls, total_calls, total_param_errors))
    } else {
        None
    };

    // Summary footer
    println!();
    print_check_summary(
        all_valid,
        total_start.elapsed().as_millis() as u64,
        workflow.tasks.len(),
        workflow.flow_count(),
        layer_count,
        schema_count,
        strict_info,
        &error_codes,
    );

    if !all_valid {
        return Err(NikaError::ValidationError {
            reason: "Strict validation failed: invoke parameters don't match tool schemas"
                .to_string(),
        });
    }

    Ok(())
}
