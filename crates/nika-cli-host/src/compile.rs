// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI transport and explicit materialization for the stateless Compile core.
mod render;

use crate::output::{VerbOutput, exit};
use nika_onboard::compile::{CompileRequest, CompileStatus, compile};
use std::io::Write as _;
use std::path::Path;

/// Explicit CLI inputs. No terminal conversation or ambient authoring policy.
#[derive(Debug, clap::Args)]
#[group(id = "compile_options", multiple = true)]
pub struct CompileArgs {
    /// Exact skeleton name or hello; unsupported words remain incomplete.
    pub intent: Option<String>,
    /// Write a Ready candidate here; omitted means preview only.
    #[arg(group = "destination")]
    pub dest: Option<String>,
    /// Explicit destination for edit mode (or create without a positional destination).
    #[arg(long, conflicts_with = "dest", group = "destination")]
    pub output: Option<String>,
    /// Explicit accepted source for a conservative edit.
    #[arg(long, requires = "change", conflicts_with = "intent")]
    pub base: Option<String>,
    /// Supported edit: `Set const.NAME to JSON_LITERAL`.
    #[arg(long, requires = "base")]
    pub change: Option<String>,
    /// Answer a stable question: `KEY=JSON_LITERAL` (repeatable).
    #[arg(long = "answer")]
    pub answers: Vec<String>,
    /// Replace the explicitly named destination.
    #[arg(long, requires = "destination")]
    pub force: bool,
    /// Print the versioned structured result, including incomplete questions.
    #[arg(long)]
    pub json: bool,
    /// List exact skeleton names without authoring or writing.
    #[arg(long, conflicts_with_all = ["intent", "dest", "output", "base", "answers"])]
    pub list: bool,
}

/// Compile once; only a Ready result with an explicit destination writes files.
#[must_use]
pub fn run(args: &CompileArgs) -> VerbOutput {
    if args.list {
        return render::listing(args.json);
    }
    let dest = args.dest.as_ref().or(args.output.as_ref());
    if dest.is_some_and(|path| !nika_source::is_canonical_program_path(path)) {
        return render::failure(
            "destination_name",
            "destination must be a canonical *.nika program path",
            exit::FILE,
            args.json,
        );
    }
    let mut request = if let Some(base) = &args.base {
        let source = match std::fs::read_to_string(base) {
            Ok(source) => source,
            Err(error) => {
                return render::failure("read_base", &error.to_string(), exit::ENV, args.json);
            }
        };
        CompileRequest::edit(source, args.change.as_deref().unwrap_or(""))
    } else {
        let mut request = CompileRequest::create(args.intent.as_deref().unwrap_or(""));
        if let Some(dest) = dest {
            request = request.with_workflow_id(workflow_id(dest));
        }
        request
    };
    for answer in &args.answers {
        let Some((key, literal)) = answer.split_once('=') else {
            return render::failure(
                "invalid_answer",
                "--answer expects KEY=JSON_LITERAL",
                exit::FILE,
                args.json,
            );
        };
        request = request.answer(key, literal);
    }
    let outcome = match compile(&request) {
        Ok(outcome) => outcome,
        Err(error) => {
            return render::failure("compile_error", &error.to_string(), exit::ENV, args.json);
        }
    };
    let mut written = None;
    if outcome.status == CompileStatus::Ready
        && let (Some(dest), Some(candidate)) = (dest, &outcome.candidate)
    {
        if let Err(error) = materialize(Path::new(dest), candidate, args.force) {
            return render::failure("destination", &error.to_string(), exit::ENV, args.json);
        }
        written = Some(dest.as_str());
        crate::metrics::record_if_enabled(
            crate::metrics::EventKind::DraftCreated,
            crate::metrics::Facts {
                draft: Some(crate::metrics::DraftSource::Compile),
                ..crate::metrics::Facts::none()
            },
        );
    }
    render::outcome(&outcome, written, args.json)
}

fn workflow_id(dest: &str) -> String {
    let name = Path::new(dest)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workflow");
    let stem = nika_source::program_stem(name).unwrap_or(name);
    let id: String = stem
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let id = id.trim_matches('-');
    if id.is_empty() {
        "workflow".to_owned()
    } else if id.starts_with(|c: char| c.is_ascii_digit()) {
        format!("workflow-{id}")
    } else {
        id.to_owned()
    }
}

fn materialize(dest: &Path, candidate: &str, force: bool) -> std::io::Result<()> {
    let conflict = || {
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "destination exists; pass --force to overwrite",
        )
    };
    // Refuse ordinary conflicts before touching even the trace-protection file.
    if !force && dest.symlink_metadata().is_ok() {
        return Err(conflict());
    }
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut pending = tempfile::NamedTempFile::new_in(parent)?;
    pending.write_all(candidate.as_bytes())?;
    pending.as_file().sync_all()?;
    // A protection failure is reported BEFORE destination publication. An existing
    // destination remains byte-identical; the temporary candidate is dropped.
    nika_onboard::project_file::protect_traces(parent)?;
    let result = if force {
        pending.persist(dest)
    } else {
        pending.persist_noclobber(dest)
    };
    result.map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            conflict()
        } else {
            e.error
        }
    })?;
    Ok(())
}
