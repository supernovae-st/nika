// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The context broker — the session never hands a model a repository
//! handle. It asks the broker for a minimal, typed bundle: the goal, the
//! project facts, the snippets the HUMAN named (inside the proven root ·
//! bounded · obvious secrets redacted · provenance kept), the grounding,
//! the data locus. The environment is never injected; the model does not
//! decide its own read boundary.

use std::fmt::Write as _;
use std::io::Read as _;
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
            let raw = match read_context_file(&path) {
                Ok(raw) => raw,
                Err(reason) => {
                    diagnostics.push(format!("`{name}` {reason} · omitted"));
                    continue;
                }
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

// Input reads have a separate ceiling from the redacted snippet output.
// Reject oversized sources rather than exposing an unredacted partial token.
const MAX_CONTEXT_FILE_BYTES: usize = 256 * 1024;

fn read_context_file(path: &Path) -> Result<String, &'static str> {
    let metadata = std::fs::metadata(path).map_err(|_| "could not be read")?;
    if !metadata.is_file() {
        return Err("is not a regular file");
    }
    let file = std::fs::File::open(path).map_err(|_| "could not be read")?;
    if !file.metadata().map_err(|_| "could not be read")?.is_file() {
        return Err("is not a regular file");
    }
    read_bounded_context(file)
}

fn read_bounded_context(reader: impl std::io::Read) -> Result<String, &'static str> {
    let mut bytes = Vec::new();
    // One lookahead byte distinguishes an exact-size file from a larger one.
    reader
        .take(MAX_CONTEXT_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "could not be read")?;
    if bytes.len() > MAX_CONTEXT_FILE_BYTES {
        return Err("exceeds the 256 KiB context read limit");
    }
    String::from_utf8(bytes).map_err(|_| "is not valid UTF-8")
}

/// Redact obvious secrets before anything leaves: API keys, private key
/// blocks, `password=`/`token=` values. Returns the text and the KINDS
/// found (never a value). An unterminated private-key block hides the rest
/// of the input; this shape-based filter is not a general secret detector.
#[must_use]
pub fn redact(text: &str) -> (String, Vec<String>) {
    let mut kinds = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut private_key_end: Option<String> = None;
    for line in text.lines() {
        // A missing or mismatched footer keeps the remaining material hidden.
        if let Some(end) = &private_key_end {
            if line.contains(end) {
                private_key_end = None;
            }
            continue;
        }
        if let Some((_, rest)) = line.split_once("-----BEGIN ")
            && let Some((label, _)) = rest.split_once("-----")
            && label.contains("PRIVATE KEY")
        {
            let end = format!("-----END {label}-----");
            private_key_end = (!rest.contains(&end)).then_some(end);
            out.push_str("[redacted private key block]\n");
            kinds.push("private key".to_owned());
            continue;
        }
        let mut l = line.to_owned();
        for (marker, kind) in [
            ("sk-", "api key"),
            ("AKIA", "aws key"),
            ("ghp_", "github token"),
            ("xoxb-", "slack token"),
        ] {
            let mut scan = 0;
            let mut kept = 0;
            let mut redacted = String::new();
            while let Some(offset) = l[scan..].find(marker) {
                let i = scan + offset;
                scan = i + marker.len();
                if l[i..].len() < marker.len() + 8
                    || !l[scan..]
                        .chars()
                        .take(8)
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    continue;
                }
                let end = l[i..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',')
                    .map_or(l.len(), |e| i + e);
                // Copy each untouched span once instead of repeatedly shifting
                // the tail of a line containing many credentials.
                redacted.push_str(&l[kept..i]);
                redacted.push_str("[redacted]");
                kept = end;
                scan = end;
            }
            if kept != 0 {
                redacted.push_str(&l[kept..]);
                l = redacted;
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
            "password: hunter2\nx: AKIAABCDEFGHIJKLMNOP\n-----BEGIN RSA PRIVATE KEY-----\nFAKE_PRIVATE_MATERIAL\n-----END RSA PRIVATE KEY-----\nplain: ${{ secrets.k }}\ntoken: ${{ secrets.t }}\n",
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
    #[test]
    fn private_key_bodies_are_removed_through_the_matching_footer() {
        for label in [
            "PRIVATE KEY",
            "RSA PRIVATE KEY",
            "EC PRIVATE KEY",
            "ENCRYPTED PRIVATE KEY",
            "OPENSSH PRIVATE KEY",
        ] {
            let input = format!(
                "before\n-----BEGIN {label}-----\nFAKE_PRIVATE_MATERIAL\n-----END {label}-----\nafter\n"
            );
            let (text, kinds) = redact(&input);
            assert!(
                !text.contains("FAKE_PRIVATE_MATERIAL"),
                "private body escaped for {label}"
            );
            assert!(!text.contains("-----END"));
            assert!(text.starts_with("before\n") && text.ends_with("after\n"));
            assert_eq!(kinds, vec!["private key"]);
        }
    }

    #[test]
    fn unterminated_or_mismatched_private_blocks_do_not_release_the_tail() {
        for tail in ["", "\r\n-----END CERTIFICATE-----\r\nMORE_FAKE_MATERIAL"] {
            let input =
                format!("évidence\r\n-----BEGIN PRIVATE KEY-----\r\nFAKE_PRIVATE_MATERIAL{tail}");
            let (text, _) = redact(&input);
            assert!(!text.contains("FAKE_PRIVATE_MATERIAL"));
            assert!(!text.contains("MORE_FAKE_MATERIAL"));
            assert!(text.contains("évidence"));
        }
    }

    #[test]
    fn every_key_on_a_line_is_redacted_even_after_an_invalid_prefix() {
        for marker in ["sk-", "AKIA", "ghp_", "xoxb-"] {
            let first = format!("{marker}FAKEFIRST123");
            let second = format!("{marker}FAKESECOND456");
            let input = format!("é {marker}! short {first}, '{second}' end");
            let (text, _) = redact(&input);
            assert!(
                !text.contains(&first) && !text.contains(&second),
                "repeated key escaped: {marker}"
            );
            assert!(text.contains("short") && text.ends_with("end"));
        }
    }

    #[test]
    fn inline_private_blocks_do_not_consume_following_lines() {
        let (text, _) = redact(
            "x: -----BEGIN PRIVATE KEY-----FAKE_BODY-----END PRIVATE KEY-----\nnext: visible",
        );
        assert!(!text.contains("FAKE_BODY"));
        assert!(text.ends_with("next: visible"));
    }

    #[test]
    fn redaction_is_idempotent_and_keeps_reference_only_text() {
        for prefix in ["", "é", "🦋", "unicode 漢字"] {
            for suffix in ["", "\n", "\r\n"] {
                let input = format!("{prefix} sk-FAKEFIRST123 sk-FAKESECOND456{suffix}");
                let (once, _) = redact(&input);
                let (twice, _) = redact(&once);
                assert_eq!(once, twice);
                assert!(!once.contains("FAKEFIRST123") && !once.contains("FAKESECOND456"));
            }
        }
        let reference = "token: ${{ secrets.k }}\nplain: visible";
        assert_eq!(redact(reference).0, reference);
    }

    #[test]
    fn named_workflow_private_material_never_reaches_the_reasoner_prompt() {
        let dir = tree();
        let input = "nika: private-fixture\nconst:\n  key: |\n    -----BEGIN PRIVATE KEY-----\n    FAKE_PRIVATE_MATERIAL\n    -----END PRIVATE KEY-----\n";
        std::fs::write(dir.path().join("a.nika.yaml"), input).expect("fixture");
        let snapshot = ProjectSnapshot::observe(dir.path());
        for cap in [8, 8192] {
            let mut broker = ContextBroker::new(dir.path().to_path_buf());
            broker.max_snippet_bytes = cap;
            let bundle = broker.bundle(&snapshot, None, &["a.nika.yaml".to_owned()], "local");
            let prompt = ContextBroker::prompt(&bundle, &[], "inspect the named workflow");
            assert_eq!(bundle.selected_snippets.len(), 1);
            assert!(bundle.redactions.contains(&"private key".to_owned()));
            assert!(!prompt.contains("FAKE_PRIVATE_MATERIAL"));
            assert!(bundle.selected_snippets[0].text.len() <= cap);
        }
    }

    #[test]
    fn oversized_named_files_are_omitted_with_a_visible_reason() {
        let dir = tree();
        let snapshot = ProjectSnapshot::observe(dir.path());
        std::fs::write(dir.path().join("a.nika.yaml"), "x".repeat(256 * 1024 + 1))
            .expect("fixture");
        let broker = ContextBroker::new(dir.path().to_path_buf());
        let bundle = broker.bundle(&snapshot, None, &["a.nika.yaml".to_owned()], "local");
        assert!(bundle.selected_snippets.is_empty());
        assert!(bundle.diagnostics.iter().any(|d| d.contains("read limit")));
    }

    #[test]
    fn non_regular_workflow_paths_are_not_read() {
        let dir = tree();
        let snapshot = ProjectSnapshot::observe(dir.path());
        std::fs::create_dir(dir.path().join("directory.nika.yaml")).expect("fixture");
        let broker = ContextBroker::new(dir.path().to_path_buf());
        let bundle = broker.bundle(
            &snapshot,
            None,
            &["directory.nika.yaml".to_owned()],
            "local",
        );
        assert!(bundle.selected_snippets.is_empty());
        assert!(
            bundle
                .diagnostics
                .iter()
                .any(|d| d.contains("not a regular file"))
        );
    }

    #[test]
    fn context_reads_stop_after_one_lookahead_byte() {
        struct Endless {
            read: usize,
        }
        impl std::io::Read for Endless {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                buffer.fill(b'x');
                self.read += buffer.len();
                Ok(buffer.len())
            }
        }
        let mut source = Endless { read: 0 };
        assert!(read_bounded_context(&mut source).is_err());
        assert_eq!(source.read, MAX_CONTEXT_FILE_BYTES + 1);
    }

    #[test]
    fn context_read_boundary_counts_bytes_and_preserves_utf8() {
        let exact = "é".repeat(MAX_CONTEXT_FILE_BYTES / 2);
        assert_eq!(
            read_bounded_context(exact.as_bytes()).expect("exact limit"),
            exact
        );
        assert!(read_bounded_context(format!("{exact}x").as_bytes()).is_err());
        assert_eq!(read_bounded_context(&b""[..]).expect("empty"), "");
        assert_eq!(read_bounded_context(&[0xff][..]), Err("is not valid UTF-8"));
    }

    #[test]
    fn context_read_errors_discard_partial_payloads() {
        struct Broken {
            first: bool,
        }
        impl std::io::Read for Broken {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.first && !buffer.is_empty() {
                    self.first = false;
                    buffer[0] = b'x';
                    return Ok(1);
                }
                Err(std::io::Error::other("fixture failure"))
            }
        }
        assert_eq!(
            read_bounded_context(Broken { first: true }),
            Err("could not be read")
        );
    }
}
