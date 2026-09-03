//! Project changes from the session (ADR-126): one typed change set,
//! consumed by BOTH the preview and the apply so the two cannot diverge;
//! witnesses against stale bytes; the engine's own audit of the exact
//! bytes as the preview's effects; consent a session event, never a
//! reasoner's tool.

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};

use nika_cli_host::fix_ladder::{StopNotes, apply_prepass};
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
        "`{0}` is not a workflow (`*.nika.yaml`), the project file (`nika.yaml`) or a file you named — name it, and the session may write it"
    )]
    Unnamed(String),
    /// The bytes changed since the preview.
    #[error(
        "`{0}` changed since this preview — nothing was applied · ask again to rebuild the preview"
    )]
    Stale(String),
    /// The file system refused.
    #[error("`{0}`: {1} — nothing else was written")]
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

impl ProjectChangeSet {
    /// Build the set from a reply: every fenced block that names a path
    /// (`path=<p>` on the fence, or `# path: <p>` as its first line)
    /// proposes the bytes at that path. `None` when the reply carries no
    /// such block (prose stays prose). `named` are the files the human
    /// named in the conversation: the only supporting files a set may
    /// touch.
    ///
    /// # Errors
    ///
    /// A path outside the root, or a path that is neither a workflow,
    /// the project file nor a named file.
    pub fn from_reply(
        root: &Path,
        goal: &str,
        reply: &str,
        named: &[String],
        run: Option<RunRequest>,
    ) -> Result<Option<Self>, ChangeError> {
        let blocks = fenced_blocks(reply);
        if blocks.is_empty() {
            return Ok(None);
        }
        let mut changes = Vec::new();
        let mut repairs = Vec::new();
        let mut audits = Vec::new();
        for (path, body) in blocks {
            let rel = relative_inside_root(&path)?;
            let is_workflow = rel
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".nika.yaml"));
            let is_project = rel == Path::new(PROJECT_FILE);
            let is_named = named.iter().any(|n| Path::new(n) == rel);
            if !(is_workflow || is_project || is_named) {
                return Err(ChangeError::Unnamed(path));
            }
            let mut content = body;
            if is_workflow {
                let mut notes = StopNotes(Vec::new());
                let mut landed = Vec::new();
                apply_prepass(&mut content, &mut landed, &mut notes);
                repairs.extend(
                    landed
                        .iter()
                        .filter(|r| r.applied)
                        .map(|r| format!("{} → {} ({})", r.old, r.new, r.kind)),
                );
                audits.push(audit_bytes(&rel, &content));
            }
            let before = std::fs::read(root.join(&rel)).ok().map(|b| Witness::of(&b));
            changes.push(match (is_workflow, is_project, before) {
                (true, _, None) => ProjectChange::CreateWorkflow { path: rel, content },
                (true, _, Some(before)) => ProjectChange::UpdateWorkflow {
                    path: rel,
                    before,
                    content,
                },
                (_, true, None) => ProjectChange::CreateProjectFile { content },
                (_, true, Some(before)) => ProjectChange::UpdateProjectFile { before, content },
                (_, _, None) => ProjectChange::CreateSupportingFile { path: rel, content },
                (_, _, Some(before)) => ProjectChange::UpdateSupportingFile {
                    path: rel,
                    before,
                    content,
                },
            });
        }
        let run = run.map(|mut r| {
            if r.workflow.as_os_str().is_empty()
                && let Some(first) = changes.iter().find(|c| c.is_workflow())
            {
                r.workflow = first.path();
            }
            r
        });
        Ok(Some(Self {
            root: root.to_path_buf(),
            goal: goal.to_owned(),
            changes,
            run,
            repairs,
            audits,
        }))
    }

    /// The preview: the exact bytes of every change, the repairs the
    /// ladder applied, the audit of every workflow, the run the consent
    /// would cover. Rendered from the set the apply consumes.
    #[must_use]
    pub fn preview(&self) -> String {
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
            for line in c.content().lines() {
                let _ = writeln!(out, "  │ {line}");
            }
            let _ = writeln!(out, "  └─");
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
            for h in &a.hints {
                let _ = writeln!(out, "    · hint · {h}");
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
            "apply this? (yes · anything else discards it · nothing is written until you say yes)",
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
    /// preview, or the file system's refusal.
    pub fn apply(&self) -> Result<Applied, ChangeError> {
        for c in &self.changes {
            let path = c.path();
            let now = std::fs::read(self.root.join(&path))
                .ok()
                .map(|b| Witness::of(&b));
            match (c.witness(), now) {
                (None, None) => {}
                (Some(before), Some(now)) if *before == now => {}
                _ => return Err(ChangeError::Stale(path.display().to_string())),
            }
        }
        let mut written = Vec::new();
        for c in &self.changes {
            let path = c.path();
            write_under(&self.root, &path, c.content())?;
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

/// The real check of a workflow as it now sits on disk (after apply).
#[must_use]
pub fn check_on_disk(root: &Path, path: &Path) -> WorkflowAudit {
    match std::fs::read_to_string(root.join(path)) {
        Ok(source) => audit_bytes(path, &source),
        Err(e) => WorkflowAudit {
            path: path.to_path_buf(),
            clean: false,
            findings: vec![format!("unreadable after apply: {e}")],
            hints: Vec::new(),
            effects: Vec::new(),
        },
    }
}

/// The facade's audit of exact bytes, folded to the preview's rows.
fn audit_bytes(path: &Path, source: &str) -> WorkflowAudit {
    let logical = path.display().to_string();
    match audit_source(source, &logical, None, None, AuditOptions::default()) {
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
    if report.cost.has_unbounded {
        rows.push(
            "spend unbounded — no cap declared (the run's --max-cost-usd is the ceiling)"
                .to_owned(),
        );
    } else {
        rows.push(format!(
            "spend ≤ ${:.4} worst case{}",
            report.cost.bounded_total_usd,
            if report.cost.bounded_total_usd > 0.0 {
                ""
            } else {
                " (mock or unpriced)"
            }
        ));
    }
    rows
}

/// The reply's prose with every fenced block removed (what the human
/// reads above the preview), ending in a blank line when non-empty.
#[must_use]
pub fn prose_outside_blocks(reply: &str) -> String {
    let mut out = Vec::new();
    let mut inside = false;
    for line in reply.lines() {
        if line.trim_start().starts_with("```") {
            inside = !inside;
            continue;
        }
        let doubled_blank =
            line.trim().is_empty() && out.last().is_some_and(|l: &&str| l.trim().is_empty());
        if !(inside || doubled_blank) {
            out.push(line);
        }
    }
    let text = out.join("\n").trim().to_owned();
    if text.is_empty() {
        text
    } else {
        format!("{text}\n\n")
    }
}

/// Every fenced block that names a path: `path=<p>` on the fence line
/// or `# path: <p>` (or `# path=<p>`) as the block's first line.
fn fenced_blocks(reply: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut open: Option<(Option<String>, Vec<String>)> = None;
    for raw in reply.lines() {
        let line = raw.trim_end();
        if let Some(rest) = line.trim_start().strip_prefix("```") {
            match open.take() {
                Some((Some(path), body)) => {
                    let mut content = body.join("\n");
                    content.push('\n');
                    blocks.push((path, content));
                }
                Some((None, _)) => {}
                None => {
                    let path = rest
                        .split_whitespace()
                        .find_map(|t| t.strip_prefix("path="))
                        .map(|p| {
                            p.trim_matches(|c| c == '"' || c == '\'' || c == '`')
                                .to_owned()
                        });
                    open = Some((path, Vec::new()));
                }
            }
            continue;
        }
        if let Some((path, body)) = &mut open {
            if body.is_empty()
                && path.is_none()
                && let Some(p) = first_line_path(line)
            {
                *path = Some(p);
                continue;
            }
            body.push(line.to_owned());
        }
    }
    blocks
}

fn first_line_path(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix('#')?.trim_start();
    let rest = rest.strip_prefix("path")?.trim_start();
    let rest = rest.strip_prefix(':').or_else(|| rest.strip_prefix('='))?;
    let p = rest
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == '`');
    (!p.is_empty()).then(|| p.to_owned())
}

/// A relative path with no `..`, no root, no empty component.
fn relative_inside_root(path: &str) -> Result<PathBuf, ChangeError> {
    let p = Path::new(path.trim());
    if path.trim().is_empty() || p.is_absolute() {
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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const WORKFLOW: &str = "nika: daily\nmodel: mock/echo\npermits: { fs: { read: [\"./notes/**\"] }, tools: [\"nika:read\"] }\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./notes/today.md\" } }\n  sum:\n    with: { text: \"${{ tasks.read.output }}\" }\n    infer: { prompt: \"Summarize: ${{ with.text }}\", max_tokens: 40 }\noutputs:\n  digest: ${{ tasks.sum.output }}\n";

    fn reply_with(path: &str, body: &str) -> String {
        format!(
            "Here is the workflow.\n\n```yaml path={path}\n{body}```\n\nRun it with `nika run`."
        )
    }

    /// The preview prints the exact bytes the apply lands; the audit of
    /// those bytes rides the preview; the effect rows come from the
    /// report's own permits and requirements.
    #[test]
    fn preview_equals_apply_for_a_create() {
        let dir = tempfile::tempdir().expect("tmp");
        let set = ProjectChangeSet::from_reply(
            dir.path(),
            "a daily digest",
            &reply_with("daily.nika.yaml", WORKFLOW),
            &[],
            None,
        )
        .expect("legal")
        .expect("a block");
        assert_eq!(set.changes.len(), 1);
        assert!(
            matches!(&set.changes[0], ProjectChange::CreateWorkflow { path, content } if path == Path::new("daily.nika.yaml") && content == WORKFLOW)
        );
        let preview = set.preview();
        for line in WORKFLOW.lines() {
            assert!(
                preview.contains(&format!("│ {line}")),
                "exact bytes in the preview: {line}"
            );
        }
        assert!(preview.contains("creates `daily.nika.yaml`"), "{preview}");
        assert!(
            preview.contains("check of these bytes · `daily.nika.yaml` · clean ✔"),
            "{preview}"
        );
        assert!(
            preview.contains("reads ./notes/today.md"),
            "the effect rows: {preview}"
        );
        assert!(preview.contains("model mock/echo"), "{preview}");
        assert!(
            preview.contains("spend ≤ $0.0000 worst case (mock or unpriced)"),
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
        assert_eq!(applied.written, vec![PathBuf::from("daily.nika.yaml")]);
        let on_disk = std::fs::read_to_string(dir.path().join("daily.nika.yaml")).expect("landed");
        assert_eq!(on_disk, WORKFLOW, "byte for byte");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(dir.path().join("daily.nika.yaml"))
                .expect("meta")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o644, "a project file, not private state");
        }
        assert!(check_on_disk(dir.path(), Path::new("daily.nika.yaml")).clean);
    }

    /// An update is witnessed: the bytes the preview was built over must
    /// be the bytes on disk at apply, or nothing is applied.
    #[test]
    fn a_stale_witness_applies_nothing() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(dir.path().join("daily.nika.yaml"), "nika: old\n").expect("seed");
        let set = ProjectChangeSet::from_reply(
            dir.path(),
            "update",
            &reply_with("daily.nika.yaml", WORKFLOW),
            &[],
            None,
        )
        .expect("legal")
        .expect("a block");
        assert!(
            matches!(&set.changes[0], ProjectChange::UpdateWorkflow { before, .. } if *before == Witness::of(b"nika: old\n"))
        );
        assert!(set.preview().contains("replaces `daily.nika.yaml` whole"));
        std::fs::write(
            dir.path().join("daily.nika.yaml"),
            "nika: changed-meanwhile\n",
        )
        .expect("race");
        let err = set.apply().expect_err("stale");
        assert!(
            matches!(err, ChangeError::Stale(ref p) if p == "daily.nika.yaml"),
            "{err}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("daily.nika.yaml")).expect("still"),
            "nika: changed-meanwhile\n",
            "nothing was applied"
        );
    }

    /// A path that leaves the root, an absolute path, or a file the human
    /// never named is refused before any preview.
    #[test]
    fn paths_outside_the_root_and_unnamed_files_refuse() {
        let dir = tempfile::tempdir().expect("tmp");
        let outside = ProjectChangeSet::from_reply(
            dir.path(),
            "g",
            &reply_with("../evil.nika.yaml", "nika: x\n"),
            &[],
            None,
        );
        assert!(matches!(outside, Err(ChangeError::OutsideRoot(_))));
        let absolute = ProjectChangeSet::from_reply(
            dir.path(),
            "g",
            &reply_with("/etc/nika.yaml", "nika: x\n"),
            &[],
            None,
        );
        assert!(matches!(absolute, Err(ChangeError::OutsideRoot(_))));
        let unnamed = ProjectChangeSet::from_reply(
            dir.path(),
            "g",
            &reply_with("notes/today.md", "hello\n"),
            &[],
            None,
        );
        assert!(matches!(unnamed, Err(ChangeError::Unnamed(_))));
        let named = ProjectChangeSet::from_reply(
            dir.path(),
            "g",
            &reply_with("notes/today.md", "hello\n"),
            &["notes/today.md".to_owned()],
            None,
        )
        .expect("legal")
        .expect("a block");
        assert!(matches!(
            &named.changes[0],
            ProjectChange::CreateSupportingFile { .. }
        ));
        named.apply().expect("lands under a created parent");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("notes/today.md")).expect("landed"),
            "hello\n"
        );
    }

    /// The prose above the preview is the reply without its fences.
    #[test]
    fn the_prose_outside_the_blocks_is_kept() {
        let reply = reply_with("daily.nika.yaml", WORKFLOW);
        let prose = prose_outside_blocks(&reply);
        assert_eq!(
            prose,
            "Here is the workflow.\n\nRun it with `nika run`.\n\n"
        );
        assert_eq!(prose_outside_blocks("```yaml\nnika: x\n```"), "");
    }

    /// Prose is prose: a reply without a path-bearing block proposes nothing;
    /// a `# path:` first line names the block too.
    #[test]
    fn prose_proposes_nothing_and_the_first_line_may_name_the_path() {
        let dir = tempfile::tempdir().expect("tmp");
        assert!(
            ProjectChangeSet::from_reply(
                dir.path(),
                "g",
                "Use a `nika:write` task.\n```yaml\nnika: unnamed\n```\n",
                &[],
                None
            )
            .expect("legal")
            .is_none()
        );
        let set = ProjectChangeSet::from_reply(dir.path(), "g", "```yaml\n# path: out/daily.nika.yaml\nnika: daily\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: hi, max_tokens: 10 }\n```\n", &[], None)
            .expect("legal")
            .expect("named on the first line");
        assert_eq!(set.changes[0].path(), PathBuf::from("out/daily.nika.yaml"));
        assert!(
            !set.changes[0].content().contains("# path:"),
            "the naming line is not part of the bytes"
        );
    }

    /// The fix ladder's mechanical prepass repairs the reasoner's dead forms
    /// before the preview, and the preview lists the repair.
    #[test]
    fn the_ladder_repairs_before_the_preview_and_says_so() {
        let dir = tempfile::tempdir().expect("tmp");
        let dead = "nika: d\nmodel: mock/echo\npermits: { exec: [\"echo\"] }\ntasks:\n  say:\n    exec: \"echo hi\"\n";
        let set = ProjectChangeSet::from_reply(
            dir.path(),
            "g",
            &reply_with("d.nika.yaml", dead),
            &[],
            None,
        )
        .expect("legal")
        .expect("a block");
        assert!(
            !set.repairs.is_empty(),
            "a repair landed: {:?}",
            set.repairs
        );
        assert!(
            set.preview().contains("repaired before this preview"),
            "{}",
            set.preview()
        );
        assert!(
            !set.changes[0].content().contains("exec: \"echo hi\""),
            "the dead form is gone"
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
        let gate = PendingGate::from_trace(Path::new("gated.nika.yaml"), &trace).expect("a gate");
        assert_eq!(gate.task, "gate");
        assert!(
            gate.question().contains("Ship the digest to the team?")
                && gate.question().contains("yes or no")
        );
        assert_eq!(gate.answer_arg("yes"), "gate=true");
        assert_eq!(gate.answer_arg("No"), "gate=false");
        std::fs::write(&trace, "{\"kind\":\"workflow_completed\",\"fields\":[]}\n").expect("trace");
        assert!(
            PendingGate::from_trace(Path::new("gated.nika.yaml"), &trace).is_none(),
            "no pause, no gate"
        );
    }

    /// A run request with no workflow named binds to the first workflow the
    /// set lands; the preview announces the ceiling and the clean-check law.
    #[test]
    fn a_run_request_binds_to_the_first_workflow() {
        let dir = tempfile::tempdir().expect("tmp");
        let run = RunRequest {
            workflow: PathBuf::new(),
            vars: vec![],
            max_cost_usd: 0.05,
        };
        let set = ProjectChangeSet::from_reply(
            dir.path(),
            "g",
            &reply_with("daily.nika.yaml", WORKFLOW),
            &[],
            Some(run),
        )
        .expect("legal")
        .expect("a block");
        assert_eq!(
            set.run.as_ref().map(|r| r.workflow.clone()),
            Some(PathBuf::from("daily.nika.yaml"))
        );
        assert!(
            set.preview()
                .contains("run `daily.nika.yaml` once (--max-cost-usd 0.05 ·")
        );
    }
}
