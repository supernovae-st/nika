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
    /// `.nika` and `nika.yaml`, only inside the root), the grounding.
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
        if name.contains('\\') {
            return None;
        }
        let file = canonical.file_name()?.to_str()?;
        if !(nika_source::is_canonical_program_file_name(file) || file == "nika.yaml") {
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

fn read_bounded_context(mut reader: impl std::io::Read) -> Result<String, &'static str> {
    let read = nika_fs::read_capped(&mut reader, MAX_CONTEXT_FILE_BYTES as u64);
    let capped = read.map_err(|_| "could not be read")?;
    if capped.over {
        return Err("exceeds the 256 KiB context read limit");
    }
    String::from_utf8(capped.bytes.to_vec()).map_err(|_| "is not valid UTF-8")
}

/// The shared outgoing-context redactor, kept at its historical Session path.
pub use nika_cli_host::context_redaction::redact;

/// What the record store keeps of `text`: `text` exactly as it was when the redactor changed
/// nothing but its line ends (it rebuilds lines with `\n`), else the redacted text.
#[must_use]
pub(crate) fn redact_for_record(text: &str) -> String {
    record_view(text, &redact)
}

/// `text` when `redact` only rebuilt its line ends as `str::lines` reads them (`\r\n` becomes
/// `\n`), else what `redact` wrote. The output decides, never the kinds reported: a text
/// changed without a kind is kept changed.
fn record_view(text: &str, redact: &dyn Fn(&str) -> (String, Vec<String>)) -> String {
    let (out, _) = redact(text);
    let mut rejoined = text.lines().collect::<Vec<_>>().join("\n");
    if text.ends_with('\n') {
        rejoined.push('\n');
    }
    if out == rejoined {
        text.to_owned()
    } else {
        out
    }
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
            dir.path().join("a.nika"),
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
            "a.nika".to_owned(),
            "notes.txt".to_owned(),
            "../outside.nika".to_owned(),
            "missing.nika".to_owned(),
        ];
        let bundle = broker.bundle(&snap, Some("summarize"), &named, "local · private");
        assert_eq!(bundle.selected_snippets.len(), 1, "{bundle:?}");
        assert_eq!(bundle.selected_snippets[0].path, "a.nika");
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
                .any(|d| d.contains("outside.nika")),
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
        std::fs::write(dir.path().join("a.nika"), input).expect("fixture");
        let snapshot = ProjectSnapshot::observe(dir.path());
        for cap in [8, 8192] {
            let mut broker = ContextBroker::new(dir.path().to_path_buf());
            broker.max_snippet_bytes = cap;
            let bundle = broker.bundle(&snapshot, None, &["a.nika".to_owned()], "local");
            let prompt = ContextBroker::prompt(&bundle, &[], "inspect the named workflow");
            assert_eq!(bundle.selected_snippets.len(), 1);
            assert!(bundle.redactions.contains(&"private key".to_owned()));
            assert!(!prompt.contains("FAKE_PRIVATE_MATERIAL"));
            assert!(bundle.selected_snippets[0].text.len() <= cap);
        }
    }

    /// The plan a compiler records for `source`: one composed component, bound to its bytes.
    fn record_of(source: &str) -> serde_json::Value {
        serde_json::json!({"strategy": "native", "source": source,
            "document": {"version": 1, "candidate_sha256": nika_compile::surface::sha256(source),
                "request": "the request", "base_sha256": null, "mode": "composed",
                "components": [{"component": {"id": "block:json-filter-records"},
                    "nodes": {"tasks": {"filter": "0".repeat(64)}}}]}})
    }

    /// `record` kept through the record store with `redact`: remembered for a proposal, then
    /// moved by its Save. Returns the record store, and the plan a later round would replay
    /// from the saved file.
    fn kept_through_save_with(
        source: &str,
        record: &serde_json::Value,
        redact: &dyn Fn(&str) -> String,
    ) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
        use nika_onboard::compile::program_records::{Place, plan, remember, saved};
        let mut raw = None;
        remember(&mut raw, Place::Proposal("p"), source, Some(record), redact);
        saved(&mut raw, "workflow.nika", source, "p", redact);
        let replayed = plan(raw.as_ref(), Place::Saved("workflow.nika"), source);
        assert!(
            replayed.is_none() || replayed.as_ref() == Some(record),
            "a replayable record is the plan as compiled"
        );
        (raw, replayed)
    }

    /// The plan of `source` kept through the record store with the Session's redactor.
    fn kept_through_save(source: &str) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
        kept_through_save_with(source, &record_of(source), &redact_for_record)
    }

    /// A workflow name that holds `sk-` inside a word keeps the record of its program.
    #[test]
    fn a_workflow_named_weekly_task_scheduler_keeps_its_record() {
        let (raw, replayed) =
            kept_through_save("nika: weekly-task-scheduler\npermits: {}\ntasks: {}\n");
        assert!(raw.is_some() && replayed.is_some(), "{raw:?}");
    }

    /// The same for `ask-questions-intake`.
    #[test]
    fn a_workflow_named_ask_questions_intake_keeps_its_record() {
        let (raw, replayed) =
            kept_through_save("nika: ask-questions-intake\npermits: {}\ntasks: {}\n");
        assert!(raw.is_some() && replayed.is_some(), "{raw:?}");
    }

    /// A prompt that says `risk-assessment` keeps the record of its program.
    #[test]
    fn a_prompt_saying_risk_assessment_keeps_its_record() {
        let (raw, replayed) = kept_through_save(
            "nika: review\ntasks:\n  t:\n    infer: { prompt: \"Write the risk-assessment summary\" }\n",
        );
        assert!(raw.is_some() && replayed.is_some(), "{raw:?}");
    }

    /// A text the redactor only re-joins holds no credential: a source with `\r\n` line ends, or a
    /// lone trailing `\r`, keeps its record replayable, with its bytes and no redaction listed.
    #[test]
    fn a_crlf_source_keeps_its_record_replayable_and_unredacted() {
        for source in [
            "nika: crlf\r\npermits: {}\r\ntasks: {}\r\n",
            "nika: crlf\npermits: {}\ntasks: {}\r",
        ] {
            let (raw, replayed) = kept_through_save(source);
            let text = raw
                .as_ref()
                .map(serde_json::Value::to_string)
                .unwrap_or_default();
            assert!(replayed.is_some(), "{source:?}: {text}");
            assert!(!text.contains("\"redactions\""), "{source:?}: {text}");
        }
    }

    /// A token in a URL is a credential: its value is never stored and the plan is never
    /// replayed, yet the record of the program and its components is kept through the Save, and
    /// the saved file keeps it.
    #[test]
    fn a_token_in_a_url_is_redacted_and_the_record_kept_through_save() {
        let source = "nika: pull\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://x.test/feed?token=abc123&page=2\" } }\n";
        let (raw, replayed) = kept_through_save(source);
        let text = raw
            .as_ref()
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        assert!(
            !text.contains("abc123"),
            "the credential is never stored: {text}"
        );
        assert!(text.contains("block:json-filter-records"), "{text}");
        assert!(text.contains("\"replayable\":false"), "{text}");
        assert_eq!(
            nika_onboard::compile::program_records::last_saved(raw.as_ref()),
            Some("workflow.nika")
        );
        assert!(replayed.is_none(), "a redacted plan is never replayed");
    }

    /// The store decides from what the redactor wrote, never from the kinds it reports: a text
    /// masked without a kind is stored masked, and its plan is never replayed.
    #[test]
    fn a_change_without_a_reported_kind_still_keeps_the_redacted_text() {
        let masks = |text: &str| (text.replace("SECRET", "[masked]"), Vec::<String>::new());
        let source = "nika: pull\npermits: {}\ntasks: {}\n# SECRET\n";
        let masked = |text: &str| record_view(text, &masks);
        let (raw, replayed) = kept_through_save_with(source, &record_of(source), &masked);
        let text = raw
            .as_ref()
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        assert!(!text.contains("SECRET"), "{text}");
        assert!(text.contains("\"replayable\":false"), "{text}");
        assert!(replayed.is_none());
    }

    /// A key glued to an encoded separator is a key in the record store too: its value is never
    /// stored, and the record of the program is kept without it.
    #[test]
    fn a_key_after_an_encoded_separator_is_never_stored() {
        let source = "nika: pull\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://x.test/feed?token%3Dsk-live-ABCDEFGH123456\" } }\n";
        let (raw, replayed) = kept_through_save(source);
        let text = raw
            .as_ref()
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        assert!(!text.contains("ABCDEFGH123456"), "{text}");
        assert!(text.contains("block:json-filter-records"), "{text}");
        assert!(replayed.is_none());
    }

    /// A credential split across texts of a plan, which the plan's single compact line showed
    /// whole: a private key given line by line, a value after the text that opens it, a key
    /// named `token: `. No text alone shows it, so such a plan is not stored.
    #[test]
    fn a_credential_split_across_texts_is_never_stored() {
        let source = "nika: pull\npermits: {}\ntasks: {}\n";
        for (split, material) in [
            (
                serde_json::json!([
                    "-----BEGIN RSA PRIVATE KEY-----",
                    "FAKE_PRIVATE_MATERIAL_LINE",
                    "-----END RSA PRIVATE KEY-----"
                ]),
                "FAKE_PRIVATE_MATERIAL_LINE",
            ),
            (
                serde_json::json!(["--password=", "FAKE_PASSWORD_VALUE"]),
                "FAKE_PASSWORD_VALUE",
            ),
            (
                serde_json::json!({"token: ": "FAKE_TOKEN_VALUE"}),
                "FAKE_TOKEN_VALUE",
            ),
        ] {
            let mut record = record_of(source);
            record["document"]["observed"] = split;
            let (raw, _) = kept_through_save_with(source, &record, &redact_for_record);
            let text = raw
                .as_ref()
                .map(serde_json::Value::to_string)
                .unwrap_or_default();
            assert!(!text.contains(material), "{text}");
        }
    }

    /// A YAML block value split across texts of a plan: the header that opens it (`token: |`,
    /// literal or folded, with chomping, indentation or a comment) in one text, its lines in the
    /// next. No text alone shows it, so such a plan is not stored; an ordinary block keeps it.
    #[test]
    fn a_yaml_block_value_split_across_texts_is_never_stored() {
        let source = "nika: pull\npermits: {}\ntasks: {}\n";
        for opened in [
            "token: |",
            "token: >",
            "password: |-",
            "secret: >+2",
            "token: | # the deploy token",
        ] {
            let mut record = record_of(source);
            record["document"]["observed"] =
                serde_json::json!(["config:", format!("  {opened}"), "    FAKE_BLOCK_VALUE"]);
            let (raw, _) = kept_through_save_with(source, &record, &redact_for_record);
            let text = raw
                .as_ref()
                .map(serde_json::Value::to_string)
                .unwrap_or_default();
            assert!(!text.contains("FAKE_BLOCK_VALUE"), "{opened}: {text}");
        }
        let mut record = record_of(source);
        record["document"]["observed"] =
            serde_json::json!(["notes:", "  description: |", "    plain text"]);
        let (raw, replayed) = kept_through_save_with(source, &record, &redact_for_record);
        let text = raw
            .as_ref()
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        assert!(
            text.contains("plain text"),
            "an ordinary block keeps its plan: {text}"
        );
        assert!(replayed.is_some(), "{text}");
    }

    #[test]
    fn oversized_named_files_are_omitted_with_a_visible_reason() {
        let dir = tree();
        let snapshot = ProjectSnapshot::observe(dir.path());
        std::fs::write(dir.path().join("a.nika"), "x".repeat(256 * 1024 + 1)).expect("fixture");
        let broker = ContextBroker::new(dir.path().to_path_buf());
        let bundle = broker.bundle(&snapshot, None, &["a.nika".to_owned()], "local");
        assert!(bundle.selected_snippets.is_empty());
        assert!(bundle.diagnostics.iter().any(|d| d.contains("read limit")));
    }

    #[test]
    fn non_regular_workflow_paths_are_not_read() {
        let dir = tree();
        let snapshot = ProjectSnapshot::observe(dir.path());
        std::fs::create_dir(dir.path().join("directory.nika")).expect("fixture");
        let broker = ContextBroker::new(dir.path().to_path_buf());
        let bundle = broker.bundle(&snapshot, None, &["directory.nika".to_owned()], "local");
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
