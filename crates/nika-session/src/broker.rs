// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The context broker — the session never hands a model a repository
//! handle. It asks the broker for a minimal, typed bundle: the goal, the
//! project facts, the snippets the HUMAN named (inside the proven root ·
//! bounded · obvious secrets redacted · provenance kept), the grounding,
//! the data locus. The environment is never injected; the model does not
//! decide its own read boundary.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::identity::{IDENTITY_CORE, language_digest};
use crate::snapshot::ProjectSnapshot;

/// One snippet the human named, with its provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Snippet {
    /// The path relative to the root.
    pub path: String,
    /// Where it came from and how it was cut (`file · N bytes · redacted K`).
    pub provenance: String,
    /// The text, redacted and bounded.
    pub text: String,
}

/// The bundle a reasoner receives — and nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SessionContextBundle {
    /// The goal the intent draft holds, when any.
    pub goal: Option<String>,
    /// The project facts (the snapshot's compact lines).
    pub project_facts: Vec<String>,
    /// The snippets the human named.
    pub selected_snippets: Vec<Snippet>,
    /// Diagnostics worth the model's attention (findings on named files).
    pub diagnostics: Vec<String>,
    /// The identity core and the language digest.
    pub canonical_grounding: String,
    /// Where this bundle goes.
    pub data_locus: String,
    /// What was redacted before it left (kinds, never values).
    pub redactions: Vec<String>,
}

/// The broker over a proven root.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContextBroker {
    /// The proven root every retrieval stays inside.
    pub root: PathBuf,
    /// The byte cap per snippet.
    pub max_snippet_bytes: usize,
}

impl ContextBroker {
    /// A broker rooted at `root`, snippets capped at 8 KiB.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            max_snippet_bytes: 8 * 1024,
        }
    }

    /// Build the bundle for one turn: the facts, the named files (only
    /// `.nika.yaml` and `nika.yaml`, only inside the root), the grounding.
    #[must_use]
    pub fn bundle(
        &self,
        snapshot: &ProjectSnapshot,
        goal: Option<&str>,
        named: &[String],
        data_locus: &str,
    ) -> SessionContextBundle {
        let mut snippets = Vec::new();
        let mut diagnostics = Vec::new();
        let mut redactions = Vec::new();
        for name in named {
            let Some(path) = self.admit(name) else {
                diagnostics.push(format!(
                    "`{name}` is outside the root or not a workflow file · not read"
                ));
                continue;
            };
            let Ok(raw) = std::fs::read_to_string(&path) else {
                diagnostics.push(format!("`{name}` could not be read"));
                continue;
            };
            let (text, kinds) = redact(&raw);
            let cut = text.len() > self.max_snippet_bytes;
            let text = if cut {
                let mut end = self.max_snippet_bytes;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text[..end].to_owned()
            } else {
                text
            };
            let canonical_root = self
                .root
                .canonicalize()
                .unwrap_or_else(|_| self.root.clone());
            let rel = path
                .strip_prefix(&canonical_root)
                .unwrap_or(&path)
                .display()
                .to_string();
            if let Some(seen) = snapshot.find(&rel)
                && !seen.clean
            {
                diagnostics.push(format!("`{rel}` has {} finding(s) at check", seen.findings));
            }
            redactions.extend(kinds.iter().cloned());
            snippets.push(Snippet {
                provenance: format!(
                    "file · {} bytes{}{}",
                    raw.len(),
                    if cut { " · cut at the cap" } else { "" },
                    if kinds.is_empty() {
                        String::new()
                    } else {
                        format!(" · redacted {}", kinds.join(", "))
                    }
                ),
                path: rel,
                text,
            });
        }
        SessionContextBundle {
            goal: goal.map(str::to_owned),
            project_facts: snapshot.facts_lines(),
            selected_snippets: snippets,
            diagnostics,
            canonical_grounding: format!("{IDENTITY_CORE}\n\n{}", language_digest()),
            data_locus: data_locus.to_owned(),
            redactions,
        }
    }

    /// The path a name may be read from: inside the root, a workflow or
    /// project file, existing.
    fn admit(&self, name: &str) -> Option<PathBuf> {
        let candidate = self.root.join(name.trim());
        let canonical = candidate.canonicalize().ok()?;
        let root = self.root.canonicalize().ok()?;
        if !canonical.starts_with(&root) {
            return None;
        }
        let file = canonical.file_name()?.to_str()?;
        if !(file.ends_with(".nika.yaml") || file == "nika.yaml") {
            return None;
        }
        Some(canonical)
    }

    /// The prompt a reasoner receives: the grounding, the facts, the
    /// snippets, the recent dialogue, the turn — compact, never the docs.
    #[must_use]
    pub fn prompt(
        bundle: &SessionContextBundle,
        recent: &[(String, String)],
        turn: &str,
    ) -> String {
        let mut p = String::new();
        p.push_str(&bundle.canonical_grounding);
        p.push_str("\n\nProject facts:\n");
        for line in &bundle.project_facts {
            p.push_str("- ");
            p.push_str(line);
            p.push('\n');
        }
        if let Some(goal) = &bundle.goal {
            let _ = write!(p, "\nGoal so far: {goal}\n");
        }
        for s in &bundle.selected_snippets {
            let _ = write!(
                p,
                "\nFile `{}` ({}):\n```yaml\n{}\n```\n",
                s.path, s.provenance, s.text
            );
        }
        for d in &bundle.diagnostics {
            let _ = write!(p, "\nDiagnostic: {d}\n");
        }
        if !recent.is_empty() {
            p.push_str("\nRecent dialogue:\n");
            for (user, assistant) in recent {
                let _ = write!(p, "user: {user}\nassistant: {assistant}\n");
            }
        }
        let _ = write!(p, "\nuser: {turn}\nassistant:");
        p
    }
}

/// Redact obvious secrets before anything leaves: API keys, private key
/// blocks, `password=`/`token=` values. Returns the text and the KINDS
/// found (never a value).
#[must_use]
pub fn redact(text: &str) -> (String, Vec<String>) {
    let mut kinds = Vec::new();
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut l = line.to_owned();
        for (marker, kind) in [
            ("sk-", "api key"),
            ("AKIA", "aws key"),
            ("ghp_", "github token"),
            ("xoxb-", "slack token"),
        ] {
            if let Some(i) = l.find(marker)
                && l[i..].len() >= marker.len() + 8
                && l[i + marker.len()..]
                    .chars()
                    .take(8)
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                let end = l[i..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',')
                    .map_or(l.len(), |e| i + e);
                l.replace_range(i..end, "[redacted]");
                kinds.push(kind.to_owned());
            }
        }
        if l.contains("-----BEGIN") {
            "[redacted private key block]".clone_into(&mut l);
            kinds.push("private key".to_owned());
        }
        for key in ["password", "token", "secret"] {
            let lower = l.to_ascii_lowercase();
            let hit = lower
                .find(&format!("{key}="))
                .or_else(|| lower.find(&format!("{key}: ")));
            if let Some(i) = hit {
                let start = i + key.len() + 1;
                let start = if l[start..].starts_with(' ') {
                    start + 1
                } else {
                    start
                };
                if start < l.len()
                    && !l[start..].trim().is_empty()
                    && !l[start..].starts_with("${{")
                {
                    l.replace_range(start.., "[redacted]");
                    kinds.push(key.to_owned());
                }
            }
        }
        out.push_str(&l);
        out.push('\n');
    }
    if !text.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    kinds.sort();
    kinds.dedup();
    (out, kinds)
}

/// Whether a path is inside `root` (a helper for the runtime's own doors).
#[must_use]
pub fn inside(root: &Path, path: &Path) -> bool {
    match (root.canonicalize(), path.canonicalize()) {
        (Ok(r), Ok(p)) => p.starts_with(r),
        _ => false,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("a.nika.yaml"),
            "nika: alpha\nmodel: mock/echo\nsecrets:\n  k: { source: env, key: OPENAI_API_KEY }\ntasks:\n  t:\n    infer: { prompt: \"key sk-live-ABCDEFGH123456 here\", max_tokens: 10 }\n",
        )
        .expect("a");
        std::fs::write(dir.path().join("notes.txt"), "token=abc123\n").expect("notes");
        std::fs::create_dir_all(dir.path().join("sub")).expect("sub");
        dir
    }

    /// Only what the human named, only inside the root, only workflow
    /// files — and the environment never rides.
    #[test]
    fn the_bundle_carries_only_the_named_files_inside_the_root() {
        let dir = tree();
        let snap = ProjectSnapshot::observe(dir.path());
        let broker = ContextBroker::new(dir.path().to_path_buf());
        let named = vec![
            "a.nika.yaml".to_owned(),
            "notes.txt".to_owned(),
            "../outside.nika.yaml".to_owned(),
            "missing.nika.yaml".to_owned(),
        ];
        let bundle = broker.bundle(&snap, Some("summarize"), &named, "local · private");
        assert_eq!(bundle.selected_snippets.len(), 1, "{bundle:?}");
        assert_eq!(bundle.selected_snippets[0].path, "a.nika.yaml");
        assert!(
            bundle
                .diagnostics
                .iter()
                .any(|d| d.contains("notes.txt") && d.contains("not read")),
            "{bundle:?}"
        );
        assert!(
            bundle
                .diagnostics
                .iter()
                .any(|d| d.contains("outside.nika.yaml")),
            "{bundle:?}"
        );
        let prompt = ContextBroker::prompt(&bundle, &[], "what does alpha do?");
        assert!(prompt.contains("Never invent Nika syntax"));
        assert!(prompt.contains("root: "));
        assert!(
            !prompt.contains("sk-live-ABCDEFGH123456"),
            "the key value never leaves: {prompt}"
        );
        assert!(prompt.contains("[redacted]"));
        assert!(
            bundle.redactions.contains(&"api key".to_owned()),
            "{:?}",
            bundle.redactions
        );
        assert!(
            prompt.contains("key: OPENAI_API_KEY"),
            "a secret's NAME is a fact, its value is not"
        );
        assert!(prompt.ends_with("user: what does alpha do?\nassistant:"));
    }

    /// The redactor catches the obvious shapes and names the kinds.
    #[test]
    fn the_redactor_names_the_kinds_never_the_values() {
        let (text, kinds) = redact(
            "password: hunter2\nx: AKIAABCDEFGHIJKLMNOP\n-----BEGIN RSA PRIVATE KEY-----\nplain: ${{ secrets.k }}\ntoken: ${{ secrets.t }}\n",
        );
        assert!(
            !text.contains("hunter2") && !text.contains("AKIAABCD") && !text.contains("BEGIN RSA"),
            "{text}"
        );
        assert!(
            text.contains("plain: ${{ secrets.k }}") && text.contains("token: ${{ secrets.t }}"),
            "a reference is not a secret: {text}"
        );
        assert_eq!(kinds, vec!["aws key", "password", "private key"]);
    }
}
