// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI and ARM adapters for the shared owned-byte execution service.

#![allow(clippy::wildcard_imports)]

use super::*;

pub(super) struct AdmittedWorld {
    pub(super) driver: nika_service_execution::ServiceExecutionDriver,
    pub(super) execution_id: nika_types::id::ExecutionId,
    pub(super) trace_id: nika_types::id::TraceId,
    pub(super) snapshot_digest: String,
    snapshot: nika_execution::ExecutionSnapshot,
    display_root: std::path::PathBuf,
    task_scope: Option<String>,
    trace: bool,
}

impl AdmittedWorld {
    fn from_context(
        context: nika_execution::ExecutionContext<'_>,
        display_root: std::path::PathBuf,
        trace: bool,
    ) -> Option<Self> {
        let execution_id = context.execution_id();
        let trace_id = context.trace_id();
        let snapshot_digest = context.snapshot().digest().to_owned();
        let snapshot = context.snapshot().clone();
        let driver = nika_service_execution::ServiceExecutionDriver::for_local_interface(
            context,
            display_root.clone(),
        )?
        .with_child_trace_factory(std::sync::Arc::new(
            child_runner::CliChildTraceFactory::new(trace),
        ));
        Some(Self {
            driver,
            execution_id,
            trace_id,
            snapshot_digest,
            snapshot,
            display_root,
            task_scope: None,
            trace,
        })
    }

    fn with_task_scope(
        mut self,
        scope: Option<&str>,
        output_json: bool,
    ) -> Result<Self, Box<RunVerdict>> {
        self.driver = self.driver.with_task_scope(scope).map_err(|message| {
            epilogue::emit_diagnostic(&message, output_json);
            Box::new(RunVerdict::bare(exit::ENV))
        })?;
        self.task_scope = scope.map(ToOwned::to_owned);
        Ok(self)
    }

    /// A gate answer starts a new execution leg over the captured world.
    /// Only the engine admission service mints its execution/trace identity;
    /// the CLI keeps the same bytes, scope and trace policy across legs.
    pub(super) fn readmit_leg(
        &self,
        model_override: Option<&str>,
        output_json: bool,
    ) -> Result<(nika_execution::ExecutionSession, Self), Box<RunVerdict>> {
        let service = nika_execution::ExecutionService::default();
        let admitted = service
            .readmit_snapshot_with_model_override(self.snapshot.clone(), model_override)
            .map_err(|error| Box::new(admission_refusal(&error, output_json)))?;
        let session = service.begin(admitted);
        let world = Self::from_context(session.context(), self.display_root.clone(), self.trace)
            .ok_or_else(|| Box::new(admitted_root_refusal(output_json)))?
            .with_task_scope(self.task_scope.as_deref(), output_json)?;
        Ok((session, world))
    }
}

#[allow(clippy::struct_excessive_bools)]
struct CliExecutionRequest<'a> {
    file: &'a str,
    repair_target: nika_display::check_render::RepairTarget,
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
        repair_target: nika_display::check_render::RepairTarget::WorkspaceFile,
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
) -> RunVerdict {
    let (project, root, display_root) = match execution_project(file) {
        Ok(parts) => parts,
        Err(error) => {
            epilogue::emit_diagnostic(&format!("nika run: environment: {error}"), output_json);
            return RunVerdict::bare(exit::ENV);
        }
    };
    let service = nika_execution::ExecutionService::default();
    let admitted = match admit_source(&service, &project, &root, preview, model_override) {
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
        repair_target: preview.repair_target(),
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
    };
    let session = service.begin(admitted);
    let outcome = run_admitted_context(session.context(), &request, display_root);
    let verdict = session.complete(outcome);
    verdict.into_outcome()
}

pub(super) fn admit_source(
    service: &nika_execution::ExecutionService,
    project: &nika_fs::OwnedDir,
    root: &std::path::Path,
    source: &crate::verbs::RunSource,
    model_override: Option<&str>,
) -> Result<nika_execution::AdmittedExecution, nika_execution::ExecutionError> {
    if source.logical_path() == "-" {
        service.admit_root_bytes_with_model_override(
            project,
            root,
            source.source().as_bytes(),
            model_override,
        )
    } else {
        service.admit_with_model_override(project, root, model_override)
    }
}

fn run_admitted_context(
    context: nika_execution::ExecutionContext<'_>,
    request: &CliExecutionRequest<'_>,
    display_root: std::path::PathBuf,
) -> RunVerdict {
    let Some(world) = AdmittedWorld::from_context(context, display_root, !request.no_trace_file)
    else {
        return admitted_root_refusal(request.output_json);
    };
    let world = match world.with_task_scope(request.task_filter, request.output_json) {
        Ok(world) => world,
        Err(verdict) => return *verdict,
    };
    let wf = world.driver.workflow().clone();
    let report = world.driver.report().clone();
    let inputs = match inputs::validated_var_overrides(request.vars, &wf, request.output_json) {
        Ok(map) => map,
        Err(code) => return RunVerdict::bare(code),
    };
    let source = world.driver.root_source();
    // One Door · wave 1: the access plan is resolved ONCE per attempt —
    // the dry-run preview, the composer, the announce, the resume
    // judgment and the admission belt all PROJECT this value.
    let plan = world
        .driver
        .resolve_access_plan(request.model_override, request.access_pin);
    if request.dry_run {
        return dry_run::lane(
            request.file,
            source,
            &wf,
            &report,
            world.driver.skills(),
            request.repair_target,
            request.model_override,
            &plan,
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
    let setup = match resume_setup(
        request.resume,
        &wf,
        source,
        request.model_override,
        (&plan, request.access_pin),
        request.output_json || request.json,
    ) {
        Ok(setup) => setup,
        Err(code) => return RunVerdict::bare(code),
    };
    let cancel = nika_types::cancel::CancelCtx::new();
    let runtime = match composed_runtime(
        request.model_override,
        &plan,
        inputs,
        setup,
        request.max_cost_usd,
        (request.no_trace_file, request.output_json),
        &world,
    ) {
        // #1438 · ONE cancel context: the driver flips it on the first signal.
        Ok(runtime) => runtime.with_cancel(cancel.clone()),
        Err(code) => return RunVerdict::bare(code),
    };
    announce_access(&plan, (request.json, request.output_json), request.mode);
    execute_and_ask(
        &runtime,
        request.file,
        (&wf, &report),
        request.resume.is_some_and(|resume| resume.trace.is_some()),
        request.vars,
        request.model_override,
        request.access_pin,
        request.max_cost_usd,
        request.theme,
        request.mode,
        (
            request.json,
            request.output_json,
            request.no_trace_file,
            request.no_outputs,
        ),
        &cancel,
        &world,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_override_reaches_file_and_stdin_admission() {
        let directory = tempfile::tempdir().expect("execution project");
        let bytes = b"nika: override\nmodel: openai/gpt-5.2\npermits: {}\ntasks:\n  answer:\n    infer: { prompt: hi, max_tokens: 32 }\n";
        let project = nika_fs::OwnedDir::open(directory.path()).expect("held project");
        let service = nika_execution::ExecutionService::default();
        for logical in ["root.nika.yaml", "-"] {
            if logical != "-" {
                std::fs::write(directory.path().join(logical), bytes).expect("root bytes");
            }
            let source =
                crate::verbs::RunSource::from_bytes(logical, bytes.to_vec()).expect("UTF-8 source");
            let root = std::path::Path::new(logical);
            assert!(admit_source(&service, &project, root, &source, None).is_err());
            let admitted = admit_source(&service, &project, root, &source, Some("mock/echo"))
                .expect("effective override admits the tiny cap");
            assert_eq!(admitted.snapshot().bytes(logical), Some(bytes.as_slice()));
        }
        assert!(!directory.path().join("-").exists());
    }

    #[test]
    fn answered_worlds_readmit_captured_bytes_and_task_scope_with_fresh_identities() {
        let directory = tempfile::tempdir().expect("execution project");
        let root = directory.path().join("root.nika.yaml");
        let source = "nika: root\nmodel: openai/gpt-5.2\npermits: {}\ntasks:\n  chosen:\n    infer: { prompt: captured, max_tokens: 32 }\n  unrelated:\n    infer: { prompt: excluded }\n";
        std::fs::write(&root, source).expect("original workflow");
        let project = nika_fs::OwnedDir::open(directory.path()).expect("held project");
        let service = nika_execution::ExecutionService::default();
        let admitted = service
            .admit_with_model_override(
                &project,
                std::path::Path::new("root.nika.yaml"),
                Some("mock/echo"),
            )
            .expect("original world admits");
        let session = service.begin(admitted);
        let world =
            AdmittedWorld::from_context(session.context(), directory.path().to_path_buf(), true)
                .expect("root world")
                .with_task_scope(Some("chosen"), false)
                .ok()
                .expect("chosen task scopes");
        std::fs::remove_file(&root).expect("remove visible workflow");
        assert!(
            world.readmit_leg(None, false).is_err(),
            "dropping the override refuses the tiny model cap"
        );
        let mut executions = vec![world.execution_id];
        let mut traces = vec![world.trace_id];
        for _ in 0..2 {
            let (leg, answered) = world
                .readmit_leg(Some("mock/echo"), false)
                .ok()
                .expect("captured world readmits without a pathname");
            assert!(!executions.contains(&answered.execution_id));
            assert!(!traces.contains(&answered.trace_id));
            executions.push(answered.execution_id);
            traces.push(answered.trace_id);
            assert_eq!(answered.execution_id, leg.context().execution_id());
            assert_eq!(
                answered.trace_id,
                nika_types::id::TraceId::from(answered.execution_id)
            );
            assert_eq!(answered.driver.execution_id(), answered.execution_id);
            assert_eq!(answered.snapshot_digest, world.snapshot_digest);
            assert_eq!(answered.driver.root_source(), source);
            assert_eq!(answered.driver.workflow().tasks.len(), 1);
            assert_eq!(answered.driver.workflow().tasks[0].value.id.value, "chosen");
            let verdict = leg.complete(());
            assert_eq!(verdict.execution_id(), answered.execution_id);
            assert_eq!(verdict.trace_id(), answered.trace_id);
        }
        assert_eq!(executions.len(), 3);
        assert_eq!(traces.len(), 3);
    }

    #[test]
    fn invalid_dry_run_override_does_not_reopen_root() {
        let directory = tempfile::tempdir().expect("execution project");
        let root = directory.path().join("root.nika.yaml");
        std::fs::write(
            &root,
            "nika: root\nmodel: mock/echo\npermits: {}\ntasks:\n  greet:\n    infer: { prompt: hi }\n",
        )
        .expect("original workflow");
        let project = nika_fs::OwnedDir::open(directory.path()).expect("held project");
        let service = nika_execution::ExecutionService::default();
        let admitted = service
            .admit(&project, std::path::Path::new("root.nika.yaml"))
            .expect("original world admits");

        std::fs::write(&root, "nika: replacement\nceiling: 0.50\n")
            .expect("replace visible pathname after admission");
        let file = root.to_string_lossy();
        let request = CliExecutionRequest {
            file: &file,
            repair_target: nika_display::check_render::RepairTarget::WorkspaceFile,
            json: false,
            output_json: false,
            theme: Theme::new(false, true, false),
            mode: RenderMode::Plain,
            dry_run: true,
            model_override: Some("nonexistent/model"),
            access_pin: None,
            vars: &[],
            resume: None,
            no_trace_file: true,
            task_filter: None,
            no_outputs: false,
            max_cost_usd: None,
        };
        let session = service.begin(admitted);
        let outcome =
            run_admitted_context(session.context(), &request, directory.path().to_path_buf());
        let verdict = session.complete(outcome);

        assert_eq!(verdict.into_outcome().code, exit::FILE);
    }
}
