// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI and ARM adapters for the shared owned-byte execution service.

#![allow(clippy::wildcard_imports)]

use super::*;

#[derive(Clone)]
pub(super) struct AdmittedWorld {
    pub(super) snapshot: nika_execution::ExecutionSnapshot,
    pub(super) display_root: std::path::PathBuf,
    pub(super) execution_id: nika_types::id::ExecutionId,
    pub(super) trace_id: nika_types::id::TraceId,
}

impl AdmittedWorld {
    fn from_context(
        context: nika_execution::ExecutionContext<'_>,
        display_root: std::path::PathBuf,
    ) -> Self {
        Self {
            snapshot: context.snapshot().clone(),
            display_root,
            execution_id: context.execution_id(),
            trace_id: context.trace_id(),
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
struct CliExecutionRequest<'a> {
    file: &'a str,
    json: bool,
    output_json: bool,
    theme: Theme,
    mode: RenderMode,
    dry_run: bool,
    model_override: Option<&'a str>,
    access_pin: Option<&'a str>,
    vars: &'a [String],
    resume: Option<&'a ResumeRequest>,
    no_trace_file: bool,
    task_filter: Option<&'a str>,
    no_outputs: bool,
    max_cost_usd: Option<f64>,
    interruptible: bool,
}

/// ARM's in-process adapter over exact service-admitted bytes; it never reopens
/// workflows, scans a latest trace, shells to the CLI, or crosses HTTP.
pub(crate) fn run_arm_context(
    context: nika_execution::ExecutionContext<'_>,
    file: &str,
    display_root: std::path::PathBuf,
    max_cost_usd: f64,
) -> RunVerdict {
    run_start_gc(false, false);
    let request = CliExecutionRequest {
        file,
        json: false,
        output_json: false,
        theme: Theme::new(false, true, false),
        mode: RenderMode::Plain,
        dry_run: false,
        model_override: None,
        access_pin: None,
        vars: &[],
        resume: None,
        no_trace_file: false,
        task_filter: None,
        no_outputs: false,
        max_cost_usd: Some(max_cost_usd),
        interruptible: false,
    };
    run_admitted_context(context, &request, display_root)
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(super) fn run_admitted(
    file: &str,
    preview: &crate::verbs::RunSource,
    (json, output_json): (bool, bool),
    theme: Theme,
    mode: RenderMode,
    dry_run: bool,
    model_override: Option<&str>,
    access_pin: Option<&str>,
    vars: &[String],
    resume: Option<&ResumeRequest>,
    no_trace_file: bool,
    task_filter: Option<&str>,
    no_outputs: bool,
    max_cost_usd: Option<f64>,
    interruptible: bool,
) -> RunVerdict {
    let (project, root, display_root) = match execution_project(file) {
        Ok(parts) => parts,
        Err(error) => {
            epilogue::emit_diagnostic(&format!("nika run: environment: {error}"), output_json);
            return RunVerdict::bare(exit::ENV);
        }
    };
    let service = nika_execution::ExecutionService::default();
    let admitted = match admit_source(&service, &project, &root, preview) {
        Ok(admitted) => admitted,
        Err(error) => return admission_refusal(&error, output_json),
    };
    if admitted.snapshot().text(admitted.snapshot().root()) != Some(preview.source()) {
        epilogue::emit_diagnostic(
            "nika run: execution admission: workflow changed during admission; retry the run",
            output_json,
        );
        return RunVerdict::bare(exit::ENV);
    }
    let request = CliExecutionRequest {
        file,
        json,
        output_json,
        theme,
        mode,
        dry_run,
        model_override,
        access_pin,
        vars,
        resume,
        no_trace_file,
        task_filter,
        no_outputs,
        max_cost_usd,
        interruptible,
    };
    let verdict = service.execute_with(admitted, move |context| {
        run_admitted_context(context, &request, display_root)
    });
    verdict.into_outcome()
}

pub(super) fn admit_source(
    service: &nika_execution::ExecutionService,
    project: &nika_fs::OwnedDir,
    root: &std::path::Path,
    source: &crate::verbs::RunSource,
) -> Result<nika_execution::AdmittedExecution, nika_execution::ExecutionError> {
    if source.logical_path() == "-" {
        service.admit_root_bytes(project, root, source.source().as_bytes())
    } else {
        service.admit(project, root)
    }
}

fn run_admitted_context(
    context: nika_execution::ExecutionContext<'_>,
    request: &CliExecutionRequest<'_>,
    display_root: std::path::PathBuf,
) -> RunVerdict {
    let world = AdmittedWorld::from_context(context, display_root);
    let (wf, report, skills) = match admitted_program(context, request, &world.snapshot) {
        Ok(program) => program,
        Err(verdict) => return *verdict,
    };
    let inputs = match inputs::validated_var_overrides(request.vars, &wf, request.output_json) {
        Ok(map) => map,
        Err(code) => return RunVerdict::bare(code),
    };
    if request.dry_run {
        return dry_run::lane(
            request.file,
            &wf,
            &report,
            request.model_override,
            request.json,
            request.theme,
            request.output_json,
        );
    }
    if let Err(code) = budget::preflight(
        &wf,
        &report,
        request.model_override,
        request.max_cost_usd,
        request.output_json,
    ) {
        return RunVerdict::bare(code);
    }
    let Some(source) = admitted_root_source(&world.snapshot) else {
        return admitted_root_refusal(request.output_json);
    };
    let setup = match resume_setup(
        request.resume,
        &wf,
        source,
        request.model_override,
        request.output_json,
    ) {
        Ok(setup) => setup,
        Err(code) => return RunVerdict::bare(code),
    };
    let runtime = match composed_runtime(
        &wf,
        source,
        request.model_override,
        request.access_pin,
        inputs,
        setup,
        request.max_cost_usd,
        skills.clone(),
        (request.no_trace_file, request.output_json),
        &report,
        &world,
    ) {
        Ok(runtime) => runtime,
        Err(code) => return RunVerdict::bare(code),
    };
    announce_access_pin(
        request.access_pin,
        (request.json, request.output_json),
        request.mode,
        &report,
    );
    execute_and_ask(
        &runtime,
        (request.file, source),
        (&wf, &report),
        request.resume.is_some_and(|resume| resume.trace.is_some()),
        request.vars,
        request.model_override,
        request.access_pin,
        request.max_cost_usd,
        &skills,
        request.theme,
        request.mode,
        (
            request.json,
            request.output_json,
            request.no_trace_file,
            request.no_outputs,
        ),
        request.interruptible,
        &world,
    )
}

fn admitted_program(
    context: nika_execution::ExecutionContext<'_>,
    request: &CliExecutionRequest<'_>,
    snapshot: &nika_execution::ExecutionSnapshot,
) -> Result<(RawWorkflow, CheckReport, BTreeMap<String, String>), Box<RunVerdict>> {
    let mut report = context.check().clone();
    crate::verbs::stamp_judged_semantic(context.workflow(), &mut report);
    let (wf, report) = match apply_task_scope(
        context.workflow().clone(),
        report,
        request.task_filter,
        request.output_json,
    ) {
        Ok(pair) => pair,
        Err(code) => return Err(Box::new(RunVerdict::bare(code))),
    };
    let skills = match admitted_skills(&wf, snapshot) {
        Ok(skills) => skills,
        Err(error) => {
            epilogue::emit_diagnostic(&format!("nika run: {error}"), request.output_json);
            return Err(Box::new(RunVerdict::bare(exit::FILE)));
        }
    };
    Ok((wf, report, skills))
}

fn admitted_root_source(snapshot: &nika_execution::ExecutionSnapshot) -> Option<&str> {
    snapshot.text(snapshot.root())
}

fn admitted_root_refusal(output_json: bool) -> RunVerdict {
    epilogue::emit_diagnostic(
        "nika run: execution admission lost its root unit",
        output_json,
    );
    RunVerdict::bare(exit::ENV)
}

fn execution_project(
    file: &str,
) -> Result<(nika_fs::OwnedDir, std::path::PathBuf, std::path::PathBuf), String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let path = std::path::Path::new(file);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let absolute = lexical_path(&absolute);
    let (display_root, root) = absolute.strip_prefix(&cwd).map_or_else(
        |_| {
            let parent = absolute
                .parent()
                .ok_or_else(|| format!("`{file}` has no project directory"))?;
            let name = absolute
                .file_name()
                .ok_or_else(|| format!("`{file}` has no workflow filename"))?;
            Ok::<_, String>((parent.to_path_buf(), std::path::PathBuf::from(name)))
        },
        |relative| Ok((cwd.clone(), relative.to_path_buf())),
    )?;
    let project = nika_fs::OwnedDir::open(&display_root)
        .map_err(|error| format!("cannot hold project `{}`: {error}", display_root.display()))?;
    Ok((project, root, display_root))
}

fn lexical_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => {
                normalized.push(std::path::Path::new(std::path::MAIN_SEPARATOR_STR));
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn admitted_skills(
    workflow: &RawWorkflow,
    snapshot: &nika_execution::ExecutionSnapshot,
) -> Result<BTreeMap<String, String>, String> {
    let owner = snapshot.root();
    let mut reader = |authored: &str| {
        let logical = child_runner::resolve_admitted(owner, authored)?;
        snapshot
            .text(&logical)
            .map(str::to_owned)
            .ok_or_else(|| format!("captured world has no unit `{logical}`"))
    };
    let resolved = nika_schema::resolve_skills(workflow, &mut reader);
    if resolved.findings.is_empty() {
        Ok(resolved.texts)
    } else {
        Err(resolved
            .findings
            .iter()
            .map(nika_schema::SkillFinding::row)
            .collect::<Vec<_>>()
            .join(" | "))
    }
}

fn admission_refusal(error: &nika_execution::ExecutionError, output_json: bool) -> RunVerdict {
    let code = if matches!(error, nika_execution::ExecutionError::Io { .. }) {
        exit::ENV
    } else {
        exit::FILE
    };
    epilogue::emit_diagnostic(
        &format!("nika run: execution admission: {error}"),
        output_json,
    );
    RunVerdict::bare(code)
}
