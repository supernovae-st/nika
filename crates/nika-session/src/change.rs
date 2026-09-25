//! Project changes from the session (ADR-126): one typed change set,
//! built from a compiler candidate by [`crate::review`] (or directly by
//! a host) and consumed by BOTH the preview and the apply so the two
//! cannot diverge; witnesses against stale bytes; the engine's own audit
//! of the exact bytes as the preview's effects; consent a session event,
//! never a reasoner's tool. No reply is ever read for a file here.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use nika_cli_host::oracle::{AuditOptions, audit_source};
use nika_fs::OwnedDir;

/// The blake3 of the bytes a preview was built over (hex).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Witness(pub String);

impl Witness {
    /// The witness of these bytes.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    /// The first eight hex digits, for a preview line.
    #[must_use]
    pub fn short(&self) -> &str {
        self.0.get(..8).unwrap_or(&self.0)
    }
}

/// One durable change: bytes added or replaced at a path inside the root.
/// No delete, move or rename: a set can only add or replace bytes the
/// human has seen in full.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectChange {
    /// A workflow file that does not exist yet.
    CreateWorkflow {
        /// Relative to the root.
        path: PathBuf,
        /// The exact bytes.
        content: String,
    },
    /// A workflow file replaced whole, witnessed.
    UpdateWorkflow {
        /// Relative to the root.
        path: PathBuf,
        /// The witness of the bytes the preview was built over.
        before: Witness,
        /// The exact bytes.
        content: String,
    },
    /// The project file (`nika.yaml`) created.
    CreateProjectFile {
        /// The exact bytes.
        content: String,
    },
    /// The project file replaced whole, witnessed.
    UpdateProjectFile {
        /// The witness of the bytes the preview was built over.
        before: Witness,
        /// The exact bytes.
        content: String,
    },
    /// A file the human named, created.
    CreateSupportingFile {
        /// Relative to the root.
        path: PathBuf,
        /// The exact bytes.
        content: String,
    },
    /// A file the human named, replaced whole, witnessed.
    UpdateSupportingFile {
        /// Relative to the root.
        path: PathBuf,
        /// The witness of the bytes the preview was built over.
        before: Witness,
        /// The exact bytes.
        content: String,
    },
}

impl ProjectChange {
    /// The path, relative to the root.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        match self {
            Self::CreateWorkflow { path, .. }
            | Self::UpdateWorkflow { path, .. }
            | Self::CreateSupportingFile { path, .. }
            | Self::UpdateSupportingFile { path, .. } => path.clone(),
            Self::CreateProjectFile { .. } | Self::UpdateProjectFile { .. } => {
                PathBuf::from(PROJECT_FILE)
            }
        }
    }

    /// The exact bytes the change lands.
    #[must_use]
    pub fn content(&self) -> &str {
        match self {
            Self::CreateWorkflow { content, .. }
            | Self::UpdateWorkflow { content, .. }
            | Self::CreateProjectFile { content }
            | Self::UpdateProjectFile { content, .. }
            | Self::CreateSupportingFile { content, .. }
            | Self::UpdateSupportingFile { content, .. } => content,
        }
    }

    /// The witness an update carries; a create carries none.
    #[must_use]
    pub fn witness(&self) -> Option<&Witness> {
        match self {
            Self::UpdateWorkflow { before, .. }
            | Self::UpdateProjectFile { before, .. }
            | Self::UpdateSupportingFile { before, .. } => Some(before),
            _ => None,
        }
    }

    /// Whether the change lands a workflow (checked after apply).
    #[must_use]
    pub fn is_workflow(&self) -> bool {
        matches!(
            self,
            Self::CreateWorkflow { .. } | Self::UpdateWorkflow { .. }
        )
    }
}

const PROJECT_FILE: &str = "nika.yaml";

/// A one-time run the human asked for with the change (« create and run
/// it once »): distinct from durable automation, which is project intent.
#[derive(Clone, Debug, PartialEq)]
pub struct RunRequest {
    /// The workflow, relative to the root.
    pub workflow: PathBuf,
    /// `--var k=v` pairs.
    pub vars: Vec<String>,
    /// The ceiling the run is announced with.
    pub max_cost_usd: f64,
}

/// The engine's audit of one workflow's exact bytes: the preview's truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowAudit {
    /// Relative to the root.
    pub path: PathBuf,
    /// The verdict the facade gave these bytes.
    pub clean: bool,
    /// `code · message`, the first eight.
    pub findings: Vec<String>,
    /// `kind · advice`, the first four.
    pub hints: Vec<String>,
    /// What the workflow reaches when it runs, from the report's own
    /// permits and requirements (reads · writes · network · programs ·
    /// tools · models · secrets · spend · human gates).
    pub effects: Vec<String>,
}

/// What a set could not become.
#[derive(Debug, thiserror::Error)]
pub enum ChangeError {
    /// The path leaves the root (absolute · `..` · empty).
    #[error("`{0}` is not a path inside the project root — a change lands only under the root")]
    OutsideRoot(String),
    /// The path is neither a workflow, the project file, nor a file the
    /// human named.
    #[error(
        "`{0}` is not a workflow (`*.nika`), the project file (`nika.yaml`) or a file you named — name it, and the session may write it"
    )]
    Unnamed(String),
    /// The bytes changed since the preview.
    #[error(
        "`{0}` changed since this preview — nothing was applied · ask again to rebuild the preview"
    )]
    Stale(String),
    /// The file system refused. The second field is the OS error plus
    /// the earlier writes whose completion was confirmed. The failed
    /// target may have changed before the error was reported.
    #[error("`{0}`: {1}")]
    Io(String, String),
}

/// The typed change set: built once from the reply, consumed by both
/// the preview and the apply.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectChangeSet {
    /// The proven root every path is relative to.
    pub root: PathBuf,
    /// The goal as the human stated it.
    pub goal: String,
    /// The changes, in reply order.
    pub changes: Vec<ProjectChange>,
    /// The one-time run the human asked for, when they did.
    pub run: Option<RunRequest>,
    /// The fix ladder's mechanical repairs applied to workflow bytes
    /// before the preview (`old → new (kind)` · listed, never hidden).
    pub repairs: Vec<String>,
    /// The audit of every workflow's exact bytes.
    pub audits: Vec<WorkflowAudit>,
}

/// A human gate a run paused on (exit 4): read from the trace's own
/// pause event, answered by the human in the session, resumed by the door.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingGate {
    /// The workflow, relative to the root.
    pub workflow: PathBuf,
    /// The paused trace (the resume handle).
    pub trace: PathBuf,
    /// The gate's task id.
    pub task: String,
    /// The question the gate asked.
    pub message: String,
    /// The prompt's mode (`confirm` · `text` · `choice` …).
    pub mode: String,
}

impl PendingGate {
    /// The gate a paused trace carries, when it carries one.
    #[must_use]
    pub fn from_trace(workflow: &Path, trace: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(trace).ok()?;
        for line in text.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v.get("kind").and_then(|k| k.as_str()) != Some("workflow_paused") {
                continue;
            }
            let field = |key: &str| -> Option<String> {
                v.get("fields")?
                    .as_array()?
                    .iter()
                    .find(|r| r.get("key").and_then(|k| k.as_str()) == Some(key))?
                    .get("value")?
                    .as_str()
                    .map(str::to_owned)
            };
            return Some(Self {
                workflow: workflow.to_path_buf(),
                trace: trace.to_path_buf(),
                task: field("task")?,
                message: field("message")
                    .unwrap_or_else(|| "the run awaits your answer".to_owned()),
                mode: field("mode").unwrap_or_else(|| "text".to_owned()),
            });
        }
        None
    }

    /// The question as the session asks it.
    #[must_use]
    pub fn question(&self) -> String {
        let how = match self.mode.as_str() {
            "confirm" => "yes or no",
            "choice" => "one of the choices, as written",
            _ => "in words",
        };
        format!(
            "the run paused at `{}` and asks you:\n  {}\n  (answer {how} · the answer resumes the run · nothing answers for you)",
            self.task, self.message
        )
    }

    /// The `--answer task=value` the human's line becomes.
    #[must_use]
    pub fn answer_arg(&self, line: &str) -> String {
        let value = match self.mode.as_str() {
            "confirm" => match line.trim().to_lowercase().as_str() {
                "yes" | "y" | "true" | "ok" | "oui" => "true".to_owned(),
                "no" | "n" | "false" | "non" => "false".to_owned(),
                other => other.to_owned(),
            },
            _ => line.trim().to_owned(),
        };
        format!("{}={value}", self.task)
    }
}

/// What apply landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    /// The paths written, relative to the root, in set order.
    pub written: Vec<PathBuf>,
}

/// A failed apply, together with the paths whose writes returned success
/// before the refusal. The failed target may also have changed; the list
/// is the write loop's own record, never inferred from the tree.
pub(crate) struct ApplyAttempt {
    /// Paths this call wrote, in set order, before the refusal.
    pub written: Vec<PathBuf>,
    /// The refusal (stale before any write, or Io at a write).
    pub error: ChangeError,
}

impl ApplyAttempt {
    fn stale(path: &Path) -> Self {
        Self {
            written: Vec::new(),
            error: ChangeError::Stale(path.display().to_string()),
        }
    }

    /// The human-facing account: the error, then the on-disk check of
    /// workflows this call itself wrote (only those; never a tree scan).
    pub(crate) fn refusal_text(&self, set: &ProjectChangeSet) -> String {
        let mut text = self.error.to_string();
        for path in &self.written {
            let Some(change) = set.changes.iter().find(|c| c.path() == *path) else {
                continue;
            };
            if !change.is_workflow() {
                continue;
            }
            let audit = check_on_disk(&set.root, path);
            let _ = write!(
                text,
                "\n  check · `{}` · {}",
                path.display(),
                if audit.clean {
                    "clean ✔"
                } else {
                    "findings ✖"
                }
            );
            for f in &audit.findings {
                let _ = write!(text, "\n    · {f}");
            }
            for h in &audit.hints {
                let _ = write!(text, "\n    · hint · {h}");
            }
        }
        text
    }
}

impl ProjectChangeSet {
    /// A workflow at a path under the root, from bytes the caller owns —
    /// the compiler's candidate through [`crate::review`], or a host's.
    /// The destination is contained (no `..`, no absolute path, a
    /// canonical program name) and witnessed NOW with the no-follow
    /// primitive the write uses: absent → a create; a regular file → an
    /// update over its witness; a symlink, a directory or an unreadable
    /// file → refused, and nothing outside the root is ever hashed. The
    /// bytes are audited by the same facade `nika check` uses. No repair
    /// pass touches them: the bytes previewed are the bytes written.
    ///
    /// # Errors
    ///
    /// A path outside the root or not a workflow name, or a destination
    /// that exists and cannot be witnessed.
    pub fn workflow_at(
        root: &Path,
        goal: &str,
        path: &str,
        content: String,
    ) -> Result<Self, ChangeError> {
        let rel = relative_inside_root(path)?;
        if !rel
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(nika_source::is_canonical_program_file_name)
        {
            return Err(ChangeError::Unnamed(path.to_owned()));
        }
        let audits = vec![audit_bytes(&rel, &content)];
        let change = match witness_now(root, &rel)? {
            None => ProjectChange::CreateWorkflow { path: rel, content },
            Some(before) => ProjectChange::UpdateWorkflow {
                path: rel,
                before,
                content,
            },
        };
        Ok(Self {
            root: root.to_path_buf(),
            goal: goal.to_owned(),
            changes: vec![change],
            run: None,
            repairs: Vec::new(),
            audits,
        })
    }

    /// A set of one project-file change (`nika.yaml` created or replaced
    /// whole, witnessed by the caller): no workflow bytes to audit; the
    /// exact bytes previewed are the bytes written.
    #[must_use]
    pub fn project_change(root: &Path, goal: &str, change: ProjectChange) -> Self {
        Self {
            root: root.to_path_buf(),
            goal: goal.to_owned(),
            changes: vec![change],
            run: None,
            repairs: Vec::new(),
            audits: Vec::new(),
        }
    }

    /// The preview: the exact bytes of every change, the repairs the
    /// ladder applied, the audit of every workflow, the run the consent
    /// would cover. Rendered from the set the apply consumes.
    #[must_use]
    pub fn preview(&self) -> String {
        self.preview_with(true)
    }

    /// The preview a human reads at the consent prompt: the same header and
    /// audits as [`Self::preview`], but each file's bytes reduced to its
    /// BOUNDARY (everything before `tasks:` — the name, the model, the
    /// constants, the inputs, the permits, the outputs) and one line for the
    /// tasks; `/show` prints the exact bytes. The identity the consent
    /// answers is still [`crate::ProposalId::of`] the full preview.
    #[must_use]
    pub fn preview_condensed(&self) -> String {
        self.preview_with(false)
    }

    fn preview_with(&self, full: bool) -> String {
        let mut out = format!("proposed change · {}\n", self.goal);
        for c in &self.changes {
            let path = c.path();
            let lines = c.content().lines().count();
            match c.witness() {
                None => {
                    let _ = writeln!(out, "  creates `{}` ({lines} lines)", path.display());
                }
                Some(w) => {
                    let _ = writeln!(
                        out,
                        "  replaces `{}` whole ({lines} lines · the file as it is now is witnessed {})",
                        path.display(),
                        w.short()
                    );
                }
            }
            let _ = writeln!(out, "  ┌─ `{}`", path.display());
            if full {
                for line in c.content().lines() {
                    let _ = writeln!(out, "  │ {line}");
                }
                let _ = writeln!(out, "  └─");
            } else {
                let mut tasks = 0usize;
                let mut in_tasks = false;
                for line in c.content().lines() {
                    if in_tasks {
                        // A task id is the only thing at exactly two spaces of indent.
                        if line.starts_with("  ") && !line.starts_with("   ") && line.ends_with(':')
                        {
                            tasks += 1;
                        }
                        continue;
                    }
                    if line == "tasks:" {
                        in_tasks = true;
                        continue;
                    }
                    let _ = writeln!(out, "  │ {line}");
                }
                if in_tasks {
                    let _ = writeln!(out, "  │ tasks: {tasks} (in run order above)");
                }
                let _ = writeln!(out, "  └─ `/show` prints the exact {lines} lines");
            }
        }
        if !self.repairs.is_empty() {
            let _ = writeln!(out, "  repaired before this preview (the fix ladder):");
            for r in &self.repairs {
                let _ = writeln!(out, "    · {r}");
            }
        }
        for a in &self.audits {
            let _ = writeln!(
                out,
                "  check of these bytes · `{}` · {}",
                a.path.display(),
                if a.clean { "clean ✔" } else { "findings ✖" }
            );
            for f in &a.findings {
                let _ = writeln!(out, "    · {f}");
            }
            if let Some(line) = compact_hints(&a.hints, &a.path.display().to_string()) {
                let _ = writeln!(out, "    · {line}");
            }
            if !a.effects.is_empty() {
                let _ = writeln!(out, "  when it runs:");
                for e in &a.effects {
                    let _ = writeln!(out, "    · {e}");
                }
            }
        }
        if let Some(r) = &self.run {
            let _ = writeln!(
                out,
                "  then · run `{}` once (--max-cost-usd {:.2} · say « with a ceiling of 0.05 » to change it) · only if the check on disk is clean",
                r.workflow.display(),
                r.max_cost_usd
            );
        }
        out.push_str(
            "apply this? (yes applies · no discards · questions keep it pending · nothing is written until you say yes)",
        );
        out
    }

    /// Land the set: every witness is checked BEFORE the first write (a
    /// stale target applies nothing); each file is written atomically
    /// under the root; nothing outside the set is touched.
    ///
    /// # Errors
    ///
    /// A stale witness, a create over bytes that appeared since the
    /// preview, a target that exists but cannot be witnessed, or the
    /// file system's refusal at write.
    pub fn apply(&self) -> Result<Applied, ChangeError> {
        self.apply_attempt().map_err(|attempt| attempt.error)
    }

    /// Land the set, keeping the paths this call itself wrote when a
    /// later write is refused. Callers that must name a partial effect
    /// use this; [`apply`](Self::apply) still returns only the error.
    pub(crate) fn apply_attempt(&self) -> Result<Applied, ApplyAttempt> {
        self.apply_attempt_with(write_under)
    }

    /// Shared write loop; the injected operation lets filesystem tests fail
    /// after replacement, where an error no longer proves absence of effect.
    pub(crate) fn apply_attempt_with(
        &self,
        mut write: impl FnMut(&Path, &Path, &str) -> Result<(), ChangeError>,
    ) -> Result<Applied, ApplyAttempt> {
        for c in &self.changes {
            let path = c.path();
            // `None` is absence only. Any other read error means the
            // path is not the preimage the preview witnessed (missing
            // for a create, or the exact bytes for an update) — stale,
            // and no write is attempted.
            let Ok(now) = witness_now(&self.root, &path) else {
                return Err(ApplyAttempt::stale(&path));
            };
            match (c.witness(), now) {
                (None, None) => {}
                (Some(before), Some(now)) if *before == now => {}
                _ => return Err(ApplyAttempt::stale(&path)),
            }
        }
        let mut written = Vec::new();
        for c in &self.changes {
            let path = c.path();
            if let Err(e) = write(&self.root, &path, c.content()) {
                let error = match e {
                    ChangeError::Io(failed, os) => {
                        ChangeError::Io(failed, io_write_account(&os, &written))
                    }
                    other => other,
                };
                return Err(ApplyAttempt { written, error });
            }
            written.push(path);
        }
        Ok(Applied { written })
    }

    /// What the set's workflows reach when they run — the preview's effect
    /// rows, answered again on request while the proposal waits.
    #[must_use]
    pub fn effects_fact(&self) -> String {
        let mut out = String::new();
        for a in &self.audits {
            let _ = writeln!(out, "`{}` when it runs:", a.path.display());
            if a.effects.is_empty() {
                out.push_str(
                    "  · nothing outside the process — no read, write, network, program or model\n",
                );
            }
            for e in &a.effects {
                let _ = writeln!(out, "  · {e}");
            }
        }
        if out.is_empty() {
            out.push_str("no workflow in this proposal — a supporting file runs nothing\n");
        }
        out.trim_end().to_owned()
    }

    /// The workflows the set lands, relative to the root.
    #[must_use]
    pub fn workflows(&self) -> Vec<PathBuf> {
        self.changes
            .iter()
            .filter(|c| c.is_workflow())
            .map(ProjectChange::path)
            .collect()
    }
}

/// The real check of a workflow as it now sits on disk (after apply) —
/// the SAME judgment `nika check` makes (the freeze audit): the composed
/// lane resolves a child the workflow invokes against the file itself, the
/// skills lane against its directory. The preview stays child-blind on
/// purpose: a proposal may create the child in the same set.
#[must_use]
pub fn check_on_disk(root: &Path, path: &Path) -> WorkflowAudit {
    let on_disk = root.join(path);
    match std::fs::read_to_string(&on_disk) {
        Ok(source) => {
            let base = on_disk.parent().map(Path::to_path_buf);
            let mut read = |p: &str| std::fs::read_to_string(p).map_err(|e| e.to_string());
            let judged = audit_source(
                &source,
                &on_disk.display().to_string(),
                Some(&mut read),
                base.as_deref(),
                AuditOptions::default(),
            );
            fold_audit(path, judged)
        }
        Err(e) => WorkflowAudit {
            path: path.to_path_buf(),
            clean: false,
            findings: vec![format!("unreadable after apply: {e}")],
            hints: Vec::new(),
            effects: Vec::new(),
        },
    }
}

/// The facade's audit of exact bytes (the preview · child-blind), folded to
/// the preview's rows.
fn audit_bytes(path: &Path, source: &str) -> WorkflowAudit {
    let logical = path.display().to_string();
    fold_audit(
        path,
        audit_source(source, &logical, None, None, AuditOptions::default()),
    )
}

/// The ONE fold of the facade's verdict to the preview's rows.
fn fold_audit<E: std::fmt::Display>(
    path: &Path,
    judged: Result<nika_cli_host::oracle::Audit, E>,
) -> WorkflowAudit {
    match judged {
        Ok(audit) => {
            let findings = audit
                .report
                .findings
                .iter()
                .take(8)
                .map(|f| format!("{} · {}", f.code.as_deref().unwrap_or("-"), f.message))
                .collect();
            let hints = audit
                .report
                .hints
                .iter()
                .take(4)
                .map(|h| format!("{} · {}", h.kind, h.advice))
                .collect();
            WorkflowAudit {
                path: path.to_path_buf(),
                clean: audit.verdict.clean,
                findings,
                hints,
                effects: effect_rows(&audit.report),
            }
        }
        Err(e) => WorkflowAudit {
            path: path.to_path_buf(),
            clean: false,
            findings: vec![format!("NIKA-PARSE · {e}")],
            hints: Vec::new(),
            effects: Vec::new(),
        },
    }
}

/// What the workflow reaches when it runs, from the report's own
/// permits (needed) and requirements: one row per effect class present.
fn effect_rows(report: &nika_check::CheckReport) -> Vec<String> {
    let mut rows = Vec::new();
    let needed = &report.permits.needed;
    if let Some(fs) = &needed.fs {
        if !fs.read.is_empty() {
            rows.push(format!("reads {}", fs.read.join(" · ")));
        }
        if !fs.write.is_empty() {
            rows.push(format!("writes {}", fs.write.join(" · ")));
        }
    }
    if let Some(net) = &needed.net
        && !net.http.is_empty()
    {
        rows.push(format!("network {}", net.http.join(" · ")));
    }
    match &needed.exec {
        Some(nika_cap::ExecPermit::Any) => rows.push("runs any program".to_owned()),
        Some(nika_cap::ExecPermit::Programs(p)) if !p.is_empty() => {
            rows.push(format!("runs {}", p.join(" · ")));
        }
        _ => {}
    }
    if let Some(tools) = &needed.tools {
        if !tools.is_empty() {
            rows.push(format!("tools {}", tools.join(" · ")));
        }
        if tools.iter().any(|t| t == "nika:prompt") {
            rows.push("pauses for a human answer (`nika:prompt`)".to_owned());
        }
    }
    if let Some(env) = &needed.env
        && !env.is_empty()
    {
        rows.push(format!("environment {}", env.join(" · ")));
    }
    for m in &report.requirements.models {
        rows.push(format!("model {} (tasks {})", m.model, m.tasks.join(" · ")));
    }
    for s in &report.requirements.secrets {
        rows.push(format!(
            "secret {} (key {} · a reference, never a value)",
            s.name, s.key
        ));
    }
    rows.extend(spend_rows(&report.cost));
    rows
}

/// The spend a run can reach, from the check's own cost envelope, never
/// from a total read alone. A workflow with no model call spends nothing
/// on inference. A mock model is a proven zero. A model with no catalog
/// price is its own row: unknown, never counted as free. A missing token
/// bound or an unknown fan-out stays unbounded.
fn spend_rows(cost: &nika_check::CostCeiling) -> Vec<String> {
    if cost.tasks.is_empty() && cost.composed.is_empty() {
        return vec![
            "model output estimate · $0 · no direct model task in these checked bytes".to_owned(),
        ];
    }
    let no_price = cost
        .tasks
        .iter()
        .filter(|t| t.unbounded_reason == Some(nika_check::UnboundedReason::NoPrice))
        .count();
    let unbounded = cost
        .tasks
        .iter()
        .filter(|t| {
            t.usd.is_none() && t.unbounded_reason != Some(nika_check::UnboundedReason::NoPrice)
        })
        .count()
        + cost.composed.iter().filter(|c| c.has_unbounded).count();
    let mut rows = Vec::new();
    if no_price > 0 {
        rows.push(format!(
            "model output estimate · unknown · {no_price} model task(s) with no catalog price — never counted as free"
        ));
    }
    if unbounded > 0 {
        rows.push(
            "model output estimate · unbounded: a token or iteration bound is missing; Run admission is separate"
                .to_owned(),
        );
    }
    if no_price == 0 && unbounded == 0 {
        let all_mock = cost.composed.is_empty()
            && cost.tasks.iter().all(|t| {
                t.model
                    .as_deref()
                    .is_some_and(|m| m == "mock" || m.starts_with("mock/"))
            });
        rows.push(if all_mock {
            "model output estimate · $0 · mock model tasks".to_owned()
        } else {
            format!(
                "model output estimate ≤ ${:.4} at catalog prices · input tokens and other charges excluded",
                cost.bounded_total_usd
            )
        });
    }
    rows
}

/// A relative path with no `..`, no root, no empty component.
fn relative_inside_root(path: &str) -> Result<PathBuf, ChangeError> {
    let p = Path::new(path.trim());
    if path.trim().is_empty() || p.is_absolute() || path.contains('\\') {
        return Err(ChangeError::OutsideRoot(path.to_owned()));
    }
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(n) => out.push(n),
            Component::CurDir => {}
            _ => return Err(ChangeError::OutsideRoot(path.to_owned())),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(ChangeError::OutsideRoot(path.to_owned()));
    }
    Ok(out)
}

/// Confirmed earlier writes and the uncertainty of the failing target.
/// `write_atomic` can replace the target before directory sync fails.
fn io_write_account(os: &str, written: &[PathBuf]) -> String {
    let earlier = if written.is_empty() {
        "no earlier file was confirmed written".to_owned()
    } else {
        let kept = written
            .iter()
            .map(|p| format!("`{}`", p.display()))
            .collect::<Vec<_>>()
            .join(" · ");
        format!("written before it and kept: {kept}")
    };
    format!(
        "{os} — {earlier} · the failed target may have changed; inspect it before retrying · later files were not attempted"
    )
}

/// The witness of the bytes at `rel` under `root`.
///
/// `None` only when the path is absent (`NotFound`). Any other error
/// (EACCES, EISDIR, …) is a refusal: the target exists in some form
/// and was not seen. Collapsing those into `None` would let a preview
/// promise `creates` over bytes the session never read.
fn witness_now(root: &Path, rel: &Path) -> Result<Option<Witness>, ChangeError> {
    let shown = rel.display().to_string();
    let io = |e: std::io::Error| {
        ChangeError::Io(
            shown.clone(),
            format!("exists but cannot be witnessed: {e}"),
        )
    };
    // Contained, no-follow: the same primitive the write path uses.
    // `std::fs::read` follows a final or parent symlink and would hash
    // bytes that sit outside the root.
    let dir = OwnedDir::open(root).map_err(io)?;
    let mut file = match dir.open_relative(rel) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(io)?;
    Ok(Some(Witness::of(&bytes)))
}

/// Write one file atomically under the root: the parents are created
/// below the root's own descriptor, the file lands by temp + rename.
fn write_under(root: &Path, rel: &Path, content: &str) -> Result<(), ChangeError> {
    let shown = rel.display().to_string();
    let io = |e: std::io::Error| ChangeError::Io(shown.clone(), e.to_string());
    let name = rel
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ChangeError::OutsideRoot(shown.clone()))?;
    let parents: Vec<&str> = rel
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| c.as_os_str().to_str())
                .collect()
        })
        .unwrap_or_default();
    let dir = OwnedDir::open(root).map_err(io)?;
    let dir = if parents.is_empty() {
        dir
    } else {
        dir.create_below(&parents).map_err(io)?
    };
    dir.write_atomic(name, content).map_err(io)?;
    // A project file a human edits and commits: the usual mode, not the
    // private-state mode the atomic writer defaults to.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(root.join(rel), std::fs::Permissions::from_mode(0o644))
            .map_err(io)?;
    }
    Ok(())
}

/// The check's hints in one line: their names, and where the full text is.
/// A hint's text opens with its name (`run-clock · …`); three paragraphs of
/// teaching after a consent hide the one line that matters (the run).
#[must_use]
pub fn compact_hints(hints: &[String], file: &str) -> Option<String> {
    if hints.is_empty() {
        return None;
    }
    let names: Vec<&str> = hints
        .iter()
        .map(|h| h.split(" · ").next().unwrap_or(h).trim())
        .collect();
    Some(format!(
        "{} hint{} · {} · `nika check {file}` prints them",
        hints.len(),
        if hints.len() > 1 { "s" } else { "" },
        names.join(" · ")
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const WORKFLOW: &str = "nika: daily\nmodel: mock/echo\npermits: { fs: { read: [\"./notes/**\"] }, tools: [\"nika:read\"] }\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./notes/today.md\" } }\n  sum:\n    with: { text: \"${{ tasks.read.output }}\" }\n    infer: { prompt: \"Summarize: ${{ with.text }}\", max_tokens: 40 }\noutputs:\n  digest: ${{ tasks.sum.output }}\n";

    /// The spend row reads the check's cost envelope, never a zero alone:
    /// no model at all, a local model with no catalog price (unknown, never
    /// free), and an inference with no token bound are three different rows.
    #[test]
    fn the_spend_row_never_reads_unpriced_from_a_zero() {
        let dir = tempfile::tempdir().expect("tmp");
        let no_model = "nika: copy\npermits: { fs: { read: [\"./a.md\"], write: [\"./b.md\"] }, tools: [\"nika:read\", \"nika:write\"] }\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./a.md\" } }\n  write:\n    with: { text: \"${{ tasks.read.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./b.md\", content: \"${{ with.text }}\" } }\n";
        let unpriced = "nika: local\nmodel: ollama/llama3.2\npermits: {}\ntasks:\n  draft:\n    infer: { prompt: \"Say hello\", max_tokens: 64 }\n";
        let open = "nika: open\nmodel: mock/echo\npermits: {}\ntasks:\n  draft:\n    infer: { prompt: \"Say hello\" }\n";
        for (name, workflow, row) in [
            (
                "copy.nika",
                no_model,
                "model output estimate · $0 · no direct model task in these checked bytes",
            ),
            (
                "local.nika",
                unpriced,
                "model output estimate · unknown · 1 model task(s) with no catalog price — never counted as free",
            ),
            ("open.nika", open, "model output estimate · unbounded"),
        ] {
            let set = ProjectChangeSet::workflow_at(dir.path(), "spend", name, workflow.to_owned())
                .expect("legal");
            let preview = set.preview();
            assert!(preview.contains(row), "{name}: {preview}");
            assert!(!preview.contains("mock or unpriced"), "{name}: {preview}");
            if name != "copy.nika" {
                assert!(
                    !preview.contains("model output estimate · $0"),
                    "no zero claimed: {preview}"
                );
                assert!(
                    !preview.contains("model output estimate ≤"),
                    "no ceiling claimed: {preview}"
                );
            }
        }
    }

    /// The preview prints the exact bytes the apply lands; the audit of
    /// those bytes rides the preview; the effect rows come from the
    /// report's own permits and requirements.
    #[test]
    fn preview_equals_apply_for_a_create() {
        let dir = tempfile::tempdir().expect("tmp");
        let set = ProjectChangeSet::workflow_at(
            dir.path(),
            "a daily digest",
            "daily.nika",
            WORKFLOW.to_owned(),
        )
        .expect("legal");
        assert_eq!(set.changes.len(), 1);
        assert!(
            matches!(&set.changes[0], ProjectChange::CreateWorkflow { path, content } if path == Path::new("daily.nika") && content == WORKFLOW)
        );
        let preview = set.preview();
        for line in WORKFLOW.lines() {
            assert!(
                preview.contains(&format!("│ {line}")),
                "exact bytes in the preview: {line}"
            );
        }
        assert!(preview.contains("creates `daily.nika`"), "{preview}");
        assert!(
            preview.contains("check of these bytes · `daily.nika` · clean ✔"),
            "{preview}"
        );
        assert!(
            preview.contains("reads ./notes/today.md"),
            "the effect rows: {preview}"
        );
        assert!(preview.contains("model mock/echo"), "{preview}");
        assert!(
            preview.contains("model output estimate · $0 · mock model tasks"),
            "the spend row is always there: {preview}"
        );
        assert!(
            set.effects_fact().contains("reads ./notes/today.md"),
            "{}",
            set.effects_fact()
        );
        assert!(
            preview.contains("nothing is written until you say yes"),
            "{preview}"
        );
        let applied = set.apply().expect("applied");
        assert_eq!(applied.written, vec![PathBuf::from("daily.nika")]);
        let on_disk = std::fs::read_to_string(dir.path().join("daily.nika")).expect("landed");
        assert_eq!(on_disk, WORKFLOW, "byte for byte");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(dir.path().join("daily.nika"))
                .expect("meta")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o644, "a project file, not private state");
        }
        assert!(check_on_disk(dir.path(), Path::new("daily.nika")).clean);
    }

    /// The freeze audit · the check after apply is the one `nika check`
    /// makes: a child the workflow invokes is judged (composed lane), so a
    /// missing child stops the run here as it stops it at the terminal —
    /// and the same child, present, reads clean.
    #[test]
    fn the_check_after_apply_judges_the_composed_world() {
        let dir = tempfile::tempdir().expect("tmp");
        let parent = "nika: parent\nmodel: mock/echo\npermits: {}\ntasks:\n  child:\n    invoke: { workflow: ./child.nika }\noutputs:\n  out: ${{ tasks.child.output }}\n";
        std::fs::write(dir.path().join("parent.nika"), parent).expect("seed");
        let missing = check_on_disk(dir.path(), Path::new("parent.nika"));
        assert!(
            !missing.clean,
            "a missing child is a finding here as at the terminal: {:?}",
            missing.findings
        );
        std::fs::write(
            dir.path().join("child.nika"),
            "nika: child\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\noutputs:\n  said: ${{ tasks.t.output }}\n",
        )
        .expect("the child");
        let present = check_on_disk(dir.path(), Path::new("parent.nika"));
        assert!(present.clean, "{:?}", present.findings);
    }

    /// An update is witnessed: the bytes the preview was built over must
    /// be the bytes on disk at apply, or nothing is applied.
    #[test]
    fn a_stale_witness_applies_nothing() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(dir.path().join("daily.nika"), "nika: old\n").expect("seed");
        let set =
            ProjectChangeSet::workflow_at(dir.path(), "update", "daily.nika", WORKFLOW.to_owned())
                .expect("legal");
        assert!(
            matches!(&set.changes[0], ProjectChange::UpdateWorkflow { before, .. } if *before == Witness::of(b"nika: old\n"))
        );
        assert!(set.preview().contains("replaces `daily.nika` whole"));
        std::fs::write(dir.path().join("daily.nika"), "nika: changed-meanwhile\n").expect("race");
        let err = set.apply().expect_err("stale");
        assert!(
            matches!(err, ChangeError::Stale(ref p) if p == "daily.nika"),
            "{err}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("daily.nika")).expect("still"),
            "nika: changed-meanwhile\n",
            "nothing was applied"
        );
    }

    /// A path that leaves the root, an absolute path, or a name that is
    /// not a workflow's is refused before any preview; a contained
    /// workflow name under a fresh parent lands.
    #[test]
    fn paths_outside_the_root_and_non_workflow_names_refuse() {
        let dir = tempfile::tempdir().expect("tmp");
        let outside =
            ProjectChangeSet::workflow_at(dir.path(), "g", "../evil.nika", "nika: x\n".to_owned());
        assert!(matches!(outside, Err(ChangeError::OutsideRoot(_))));
        let absolute = ProjectChangeSet::workflow_at(
            dir.path(),
            "g",
            "/etc/nika.yaml",
            "nika: x\n".to_owned(),
        );
        assert!(matches!(absolute, Err(ChangeError::OutsideRoot(_))));
        let not_a_workflow =
            ProjectChangeSet::workflow_at(dir.path(), "g", "notes/today.md", "hello\n".to_owned());
        assert!(matches!(not_a_workflow, Err(ChangeError::Unnamed(_))));
        let nested =
            ProjectChangeSet::workflow_at(dir.path(), "g", "notes/daily.nika", WORKFLOW.to_owned())
                .expect("legal");
        assert!(matches!(
            &nested.changes[0],
            ProjectChange::CreateWorkflow { path, .. } if path == Path::new("notes/daily.nika")
        ));
        nested.apply().expect("lands under a created parent");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("notes/daily.nika")).expect("landed"),
            WORKFLOW
        );
    }

    /// A paused trace yields the gate the human must answer; the answer
    /// becomes the resume argument in the gate's own mode.
    #[test]
    fn a_paused_trace_yields_the_pending_gate() {
        let dir = tempfile::tempdir().expect("tmp");
        let trace = dir.path().join("paused.ndjson");
        std::fs::write(
            &trace,
            "{\"kind\":\"workflow_started\",\"fields\":[{\"key\":\"workflow\",\"value\":\"gated\"}]}\n{\"kind\":\"workflow_paused\",\"fields\":[{\"key\":\"workflow\",\"value\":\"gated\"},{\"key\":\"task\",\"value\":\"gate\"},{\"key\":\"mode\",\"value\":\"confirm\"},{\"key\":\"message\",\"value\":\"Ship the digest to the team?\"}]}\n",
        )
        .expect("trace");
        let gate = PendingGate::from_trace(Path::new("gated.nika"), &trace).expect("a gate");
        assert_eq!(gate.task, "gate");
        assert!(
            gate.question().contains("Ship the digest to the team?")
                && gate.question().contains("yes or no")
        );
        assert_eq!(gate.answer_arg("yes"), "gate=true");
        assert_eq!(gate.answer_arg("No"), "gate=false");
        std::fs::write(&trace, "{\"kind\":\"workflow_completed\",\"fields\":[]}\n").expect("trace");
        assert!(
            PendingGate::from_trace(Path::new("gated.nika"), &trace).is_none(),
            "no pause, no gate"
        );
    }

    /// Restore a mode even if the test panics, so the tempdir can drop.
    #[cfg(unix)]
    struct RestorePerms {
        path: PathBuf,
        mode: u32,
    }

    #[cfg(unix)]
    impl Drop for RestorePerms {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt as _;
            let _ =
                std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(self.mode));
        }
    }

    /// Visible when a unix permission law cannot be proven on this
    /// process (root, or a 0o555 directory that still accepts a write).
    /// stderr, not a panic: the suite stays green and the limitation is
    /// named.
    #[cfg(unix)]
    #[allow(clippy::disallowed_macros, clippy::print_stderr)]
    fn note_coverage_limit(why: &str) {
        eprintln!("{why}");
    }

    /// `Some` when this process cannot prove EACCES on a 0o000 file
    /// (root, a capability that ignores mode bits, or an unexpected
    /// error). The caller names the reason and returns; it must not
    /// panic, and it must not invent a pass of the EACCES law.
    #[cfg(unix)]
    fn eacces_unproven(path: &Path) -> Option<String> {
        match std::fs::read(path) {
            Ok(_) => Some(format!(
                "coverage limitation: this process can still read 0o000 at {}; EACCES law not proven",
                path.display()
            )),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => None,
            Err(e) => Some(format!(
                "coverage limitation: 0o000 at {} produced {e}; EACCES law not proven",
                path.display()
            )),
        }
    }

    /// An existing target the process cannot read is not a create:
    /// `std::fs::read(..).ok()` would treat EACCES like absence and the
    /// preview would promise `creates` over bytes that were never
    /// witnessed. Refused before any preview; the unreadable preimage
    /// is left untouched. Skipped when this process can still read a
    /// 0o000 file (root).
    #[cfg(unix)]
    #[test]
    fn an_unreadable_existing_target_is_refused_before_the_preview() {
        use std::os::unix::fs::PermissionsExt as _;
        const SECRET: &str = "nika: secret-on-disk\n";
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("secret.nika");
        std::fs::write(&path, SECRET).expect("seed");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let restore = RestorePerms {
            path: path.clone(),
            mode: 0o644,
        };
        if let Some(why) = eacces_unproven(&path) {
            note_coverage_limit(&why);
            return;
        }
        let err =
            ProjectChangeSet::workflow_at(dir.path(), "g", "secret.nika", WORKFLOW.to_owned())
                .expect_err("an unreadable existing target is not a create");
        assert!(
            matches!(err, ChangeError::Io(..)),
            "the class is the file system's, not unnamed/stale: {err}"
        );
        let text = err.to_string();
        assert!(
            text.contains("secret.nika")
                && (text.contains("cannot be witnessed") || text.contains("unreadable")),
            "the refusal names that the target exists and was not seen: {text}"
        );
        assert!(!text.contains("creates"), "no create is promised: {text}");
        drop(restore);
        assert_eq!(
            std::fs::read_to_string(&path).expect("untouched"),
            SECRET,
            "the unreadable preimage is not replaced"
        );
    }

    /// A create whose destination becomes unreadable after the preview
    /// is stale: the file is not absent, it cannot be witnessed, and
    /// apply must not try the write. Distinct from a permission error
    /// mid-write. Skipped when this process can still read a 0o000 file.
    #[cfg(unix)]
    #[test]
    fn a_create_over_an_unreadable_now_target_is_stale_and_writes_nothing() {
        use std::os::unix::fs::PermissionsExt as _;
        const SECRET: &str = "nika: appeared-unreadable\n";
        let dir = tempfile::tempdir().expect("tmp");
        let set = ProjectChangeSet::workflow_at(dir.path(), "g", "daily.nika", WORKFLOW.to_owned())
            .expect("legal");
        assert!(matches!(
            &set.changes[0],
            ProjectChange::CreateWorkflow { .. }
        ));
        let path = dir.path().join("daily.nika");
        std::fs::write(&path, SECRET).expect("appeared");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let restore = RestorePerms {
            path: path.clone(),
            mode: 0o644,
        };
        if let Some(why) = eacces_unproven(&path) {
            note_coverage_limit(&why);
            return;
        }
        let err = set.apply().expect_err("stale");
        assert!(
            matches!(err, ChangeError::Stale(ref p) if p == "daily.nika"),
            "unreadable-now is stale, not a silent create: {err}"
        );
        drop(restore);
        assert_eq!(
            std::fs::read_to_string(&path).expect("untouched"),
            SECRET,
            "nothing was written over the unreadable preimage"
        );
    }

    /// Two creates: the first write lands, the second parent is then
    /// not writable. The refusal names the file that landed and the
    /// file that did not — it does not say « nothing else was written »
    /// as if the set were empty. Distinct from a stale preflight, which
    /// writes nothing at all.
    #[cfg(unix)]
    #[test]
    fn a_write_refused_mid_set_names_what_already_landed() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("tmp");
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).expect("locked");
        let set = ProjectChangeSet {
            root: dir.path().to_path_buf(),
            goal: "two files".to_owned(),
            changes: vec![
                ProjectChange::CreateWorkflow {
                    path: PathBuf::from("brief.nika"),
                    content: WORKFLOW.to_owned(),
                },
                ProjectChange::CreateWorkflow {
                    path: PathBuf::from("locked/note.nika"),
                    content: WORKFLOW.to_owned(),
                },
            ],
            run: None,
            repairs: Vec::new(),
            audits: Vec::new(),
        };
        assert_eq!(set.changes.len(), 2);
        assert!(set.changes.iter().all(|c| c.witness().is_none()));
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("ro");
        let restore = RestorePerms {
            path: locked.clone(),
            mode: 0o755,
        };
        let Err(attempt) = set.apply_attempt() else {
            note_coverage_limit(
                "coverage limitation: this process wrote into a 0o555 directory; mid-set Io law not proven",
            );
            return;
        };
        assert_eq!(
            attempt.written,
            vec![PathBuf::from("brief.nika")],
            "the write loop's own record, not a tree scan"
        );
        let err = attempt.error;
        let brief = dir.path().join("brief.nika");
        let note = locked.join("note.nika");
        assert_eq!(
            std::fs::read_to_string(&brief).expect("first landed"),
            WORKFLOW,
            "the first write is on disk"
        );
        assert!(!note.exists(), "the second write did not land");
        let text = err.to_string();
        assert!(
            matches!(err, ChangeError::Io(..)),
            "a write-time refusal, not a stale preflight: {err}"
        );
        assert!(
            text.contains("brief.nika")
                && (text.contains("written before") || text.contains("kept")),
            "the refusal names what landed: {text}"
        );
        assert!(
            !text.contains("nothing else was written"),
            "the baked suffix claims a total no-write: {text}"
        );
        drop(restore);
    }
}
