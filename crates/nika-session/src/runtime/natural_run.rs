// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A natural request to run an explicitly named project workflow, or rerun the saved one,
//! before the conversation author mistakes it for new preparation. The semantic router
//! reads the exact line; the existing Run admission checks the actual file and its inputs.

use std::path::{Component, Path, PathBuf};

use nika_onboard::routing::run_options;

use super::{SessionRuntime, TurnOutcome, ceiling_in};
use crate::change::Witness;
use crate::outcome::{Refusal, RefusalClass};
use crate::turn::{SessionPhase, TurnAct};

impl SessionRuntime {
    /// Read a free line after its money and knowledge admission. Only a request to run the
    /// current workflow unchanged reaches Run; every other act remains with the conversation.
    /// An author's question can supply context for the request, but this door binds no answer,
    /// consent or grant. The raw request is recorded in the conversation's tree before Run.
    pub(super) fn natural_run_turn(&mut self, line: &str) -> Option<TurnOutcome> {
        if self.pending.is_some()
            || self.authoring.is_some()
            || self.revising.is_some()
            || self.run_inputs.is_some()
            || self.activation.is_some()
            || self.pending_gate.is_some()
            || self.money.gate.is_some()
            || self.pending_choice
            || self.knowledge.held.is_some()
            || self.waiting_cost_choice()
            || self.waiting_review().is_some()
        {
            return None;
        }
        let options = run_options::parse(line);
        // Keep literal quoted paths exact: the options parser normalizes whitespace.
        let (workflow, named, observed) = self.natural_run_target(line)?;
        let phase = if self.agent_question_id().is_some() {
            SessionPhase::QuestionPending
        } else {
            SessionPhase::Idle
        };
        let decision = if named {
            self.classify_named_run(phase, line, &workflow)
        } else if observed.is_some() {
            self.classify_observed_run(phase, line, &workflow)
        } else {
            self.classify(phase, line)
        };
        if decision.act != TurnAct::RequestRun || !decision.secondary.is_empty() {
            return None;
        }
        // Recheck after cognition; the classifier cannot replace the named target with an
        // outside, missing or linked file. Run still performs its own check and byte binding.
        if (named || observed.is_some())
            && !regular_project_workflow(&self.snapshot.root, &workflow)
        {
            return None;
        }
        let Some((words, access_pin)) = options else {
            return Some(self.refuse_run_money(line, "invalid or duplicate Run option — use --access <pin> and --max-cost-usd <amount> once each"));
        };
        let ceiling = match ceiling_in(line) {
            Ok(ceiling) => ceiling,
            Err(reason) => return Some(self.refuse_run_money(line, reason)),
        };
        if let Err(reason) = self.record_agent_rerun(line) {
            return Some(TurnOutcome::Refusal(Refusal::new(
                RefusalClass::NotAllowed,
                format!("the rerun request could not be recorded; nothing was run · {reason}"),
            )));
        }
        Some(self.admit_bound_run(
            line,
            (workflow, observed),
            ceiling,
            access_pin,
            run_options::inline_vars(&words),
        ))
    }

    /// A named target is an existing regular project file, never a guessed replacement for
    /// the current workflow. Repeated references to the same logical path stay unambiguous.
    fn natural_run_target(&self, line: &str) -> Option<(PathBuf, bool, Option<Witness>)> {
        let root = &self.snapshot.root;
        let mut names = named_workflows(line)?.into_iter();
        let Some(first) = names.next() else {
            // Save may keep an absolute path. Run uses the same project-relative identity.
            return match self.last_workflow.as_deref() {
                Some(saved) => relative_workflow(root, saved).map(|path| (path, false, None)),
                None => self
                    .observed_run_target()
                    .map(|(path, witness)| (path, false, Some(witness))),
            };
        };
        let workflow = relative_workflow(root, first)?;
        if names.any(|other| relative_workflow(root, other).as_ref() != Some(&workflow)) {
            return None;
        }
        regular_project_workflow(root, &workflow).then_some((workflow, true, None))
    }

    /// The last settled Run is context only: no target or consent is restored at open.
    /// Its exact observed bytes must still exist as a regular workflow inside this project.
    pub(super) fn observed_run_target(&self) -> Option<(PathBuf, Witness)> {
        let kept = self.kept_run()?.ok()?;
        if !matches!(kept.exit, Some(0 | 1)) {
            return None;
        }
        let workflow =
            relative_workflow(&self.snapshot.root, Path::new(kept.workflow.as_deref()?))?;
        if !workflow
            .file_name()?
            .to_str()
            .is_some_and(nika_source::is_canonical_program_file_name)
            || !regular_project_workflow(&self.snapshot.root, &workflow)
        {
            return None;
        }
        let bytes = std::fs::read(self.snapshot.root.join(&workflow)).ok()?;
        (nika_event::source_id::sha256_hex(&bytes) == kept.workflow_sha256?)
            .then(|| (workflow, Witness::of(&bytes)))
    }
}

/// Filesystem evidence only; it never decides whether the person requested a Run.
/// No component inside the project may be a symbolic link. Admission checks bytes again.
fn regular_project_workflow(root: &Path, workflow: &Path) -> bool {
    let mut path = root.to_path_buf();
    let mut file = false;
    for part in workflow.components() {
        let Component::Normal(name) = part else {
            return false;
        };
        path.push(name);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return false;
        }
        file = metadata.is_file();
    }
    file
}

/// The project's logical workflow path. Parent traversal is never normalized into a match:
/// it can cross a symlink, and therefore does not designate the saved path by these words.
pub(super) fn relative_workflow(root: &Path, path: &Path) -> Option<PathBuf> {
    let path = if path.is_absolute() {
        path.strip_prefix(root).ok()?
    } else {
        path
    };
    let components = path.components().filter(|part| *part != Component::CurDir);
    let mut relative = PathBuf::new();
    for part in components {
        let Component::Normal(name) = part else {
            return None;
        };
        relative.push(name);
    }
    (!relative.as_os_str().is_empty()).then_some(relative)
}

/// Literal workflow references, including paths. Checking only a canonical file name would
/// miss `./another.nika` and incorrectly let a run label select the saved workflow instead.
fn named_workflows(line: &str) -> Option<Vec<&Path>> {
    let separator = |c: char| c.is_whitespace() || ",?()[]<>".contains(c);
    let mut rest = line;
    let mut paths = Vec::new();
    let mut option_value = false;
    while !rest.is_empty() {
        rest = rest.trim_start_matches(separator);
        let Some(first) = rest.chars().next() else {
            break;
        };
        let quoted = matches!(first, '`' | '"' | '\'');
        let word = if quoted {
            rest = &rest[first.len_utf8()..];
            let end = rest.find(first)?;
            let word = &rest[..end];
            rest = &rest[end + first.len_utf8()..];
            word
        } else {
            let end = rest.find(separator).unwrap_or(rest.len());
            let word = &rest[..end];
            rest = &rest[end..];
            word.trim_end_matches(['.', ';', ':', '!'])
        };
        if option_value {
            option_value = false;
            continue;
        }
        if !quoted && word.starts_with('-') {
            option_value = !word.contains('=');
            continue;
        }
        if !quoted && word.contains('=') {
            continue;
        }
        let path = Path::new(word);
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(nika_source::is_canonical_program_file_name)
        {
            paths.push(path);
        }
    }
    Some(paths)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    mod observed;

    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::change::{ProjectChangeSet, Witness};
    use crate::intelligence::{DataLocus, IntelligenceKind};
    use crate::reasoner::NoReasoner;
    use crate::runtime::tests::ready;
    use crate::turn::{RoutingMethod, TurnClassifier, TurnContext, TurnDecision};

    const WORKFLOW: &str = "nika: saved\npermits:\n  fs: { read: [\"./source.txt\"], write: [\"./copy.txt\"] }\n  tools: [\"nika:read\", \"nika:write\"]\ntasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"./source.txt\" } }\n  copy:\n    with: { source: \"${{ tasks.read.output }}\" }\n    invoke: { tool: \"nika:write\", args: { path: \"./copy.txt\", content: \"${{ with.source }}\" } }\n";
    const INPUT: &str = "nika: greet\ninputs:\n  name: { type: string, required: true }\npermits:\n  fs: { write: [\"./hello.txt\"] }\n  tools: [\"nika:write\"]\ntasks:\n  write:\n    invoke: { tool: \"nika:write\", args: { path: \"./hello.txt\", content: \"hi ${{ inputs.name }}\" } }\n";
    const AGAIN: &str =
        "Le fichier source est rétabli, peux-tu relancer le workflow enregistré tel quel ?";

    type Seen = Arc<Mutex<Vec<(TurnContext, String)>>>;

    /// The tests prove the route's contract with a semantic reading, not a French lexicon or
    /// the accuracy of a particular model. The real classifier receives these same raw words.
    struct Reading {
        decision: TurnDecision,
        seen: Seen,
    }

    impl TurnClassifier for Reading {
        fn classify(&mut self, context: &TurnContext, raw: &str) -> TurnDecision {
            self.seen
                .lock()
                .expect("record")
                .push((context.clone(), raw.to_owned()));
            self.decision.clone()
        }
    }

    fn session(bytes: &str, act: TurnAct) -> (tempfile::TempDir, SessionRuntime, Seen) {
        let dir = tempfile::tempdir().expect("project");
        std::fs::write(dir.path().join("saved.nika"), bytes).expect("saved workflow");
        std::fs::write(dir.path().join("source.txt"), "repaired input\n").expect("source");
        let mut runtime = SessionRuntime::open(
            dir.path(),
            ready(IntelligenceKind::None, DataLocus::None),
            Box::new(NoReasoner),
        );
        runtime.last_workflow = Some(PathBuf::from("saved.nika"));
        runtime.intent.goal = Some("Copy the source into copy.txt".to_owned());
        let seen = Arc::new(Mutex::new(Vec::new()));
        runtime.with_classifier(Box::new(Reading {
            decision: TurnDecision::new(act, RoutingMethod::Model),
            seen: Arc::clone(&seen),
        }));
        (dir, runtime, seen)
    }

    #[test]
    fn french_rerun_requests_the_existing_bytes_without_preparing_a_candidate() {
        let (dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
        let Some(TurnOutcome::RunRequested { run, .. }) = runtime.natural_run_turn(AGAIN) else {
            panic!("the saved workflow reaches Run");
        };
        assert_eq!(run.workflow, PathBuf::from("saved.nika"));
        assert_eq!(
            run.bytes.as_deref(),
            Some(&Witness::of(WORKFLOW.as_bytes()))
        );
        assert!(run.admits(WORKFLOW));
        assert!(run.closure.is_some());
        assert!(run.vars.is_empty());
        assert_eq!(run.max_cost_usd.to_bits(), 0.25_f64.to_bits());
        assert_eq!(run.access_pin, None);
        assert!(runtime.pending.is_none());
        assert!(runtime.authoring.is_none());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("saved.nika")).expect("saved"),
            WORKFLOW
        );
        assert!(!dir.path().join("compiled-workflow.nika").exists());
        assert!(
            !dir.path().join("copy.txt").exists(),
            "admission requests, the host executes"
        );
        let read = seen.lock().expect("record");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].1, AGAIN);
        assert_eq!(read[0].0.phase, SessionPhase::Idle);
        assert!(
            read[0]
                .0
                .automation
                .as_deref()
                .is_some_and(|text| text.contains("saved.nika"))
        );
    }

    #[test]
    fn a_change_condition_no_run_or_question_stays_with_the_conversation() {
        for (line, act) in [
            (
                "Avant de relancer, écris plutôt dans final.txt.",
                TurnAct::Modify,
            ),
            (
                "Tu peux relancer, mais seulement le vendredi.",
                TurnAct::Mixed,
            ),
            (
                "Si je confirme demain, tu pourras relancer.",
                TurnAct::Mixed,
            ),
            ("Ne relance rien, annule cette demande.", TurnAct::Cancel),
            (
                "Le fichier est réparé, mais ne relance pas encore.",
                TurnAct::Discuss,
            ),
            ("Pourquoi le workflow a-t-il échoué ?", TurnAct::Discuss),
            ("Peut-être plus tard.", TurnAct::Unknown),
        ] {
            let (dir, mut runtime, seen) = session(WORKFLOW, act);
            assert_eq!(runtime.natural_run_turn(line), None, "{line}");
            assert!(runtime.requested_run.is_none(), "{line}");
            assert!(runtime.run_inputs.is_none(), "{line}");
            assert_eq!(seen.lock().expect("record")[0].1, line);
            assert_eq!(
                std::fs::read_to_string(dir.path().join("saved.nika")).expect("saved"),
                WORKFLOW
            );
        }
    }

    #[test]
    fn a_run_label_with_a_secondary_change_never_runs() {
        let (_dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
        let mut decision = TurnDecision::new(TurnAct::RequestRun, RoutingMethod::Model);
        decision.secondary.push(TurnAct::Modify);
        runtime.with_classifier(Box::new(Reading { decision, seen }));
        assert_eq!(
            runtime.natural_run_turn("Relance avec une autre destination."),
            None
        );
        assert!(runtime.requested_run.is_none());
    }

    #[test]
    fn the_existing_check_still_refuses_changed_invalid_bytes() {
        let (dir, mut runtime, _) = session(WORKFLOW, TurnAct::RequestRun);
        let changed = "nika: broken\ntasks:\n  task:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n";
        std::fs::write(dir.path().join("saved.nika"), changed).expect("external change");
        let Some(TurnOutcome::Facts(facts)) = runtime.natural_run_turn(AGAIN) else {
            panic!("the latest bytes must fail their check");
        };
        assert!(facts.contains("findings"));
        assert!(runtime.requested_run.is_none());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("saved.nika")).expect("saved"),
            changed
        );
    }

    #[test]
    fn the_same_admission_preserves_run_options_and_inline_inputs() {
        let (_dir, mut runtime, _) = session(INPUT, TurnAct::RequestRun);
        let line = "Peux-tu relancer tel quel name=Thibaut --access mock --max-cost-usd 0.05";
        let Some(TurnOutcome::RunRequested { run, .. }) = runtime.natural_run_turn(line) else {
            panic!("Run with its explicit options");
        };
        assert_eq!(run.vars, ["name=Thibaut"]);
        assert_eq!(run.max_cost_usd.to_bits(), 0.05_f64.to_bits());
        assert_eq!(run.access_pin.as_deref(), Some("mock"));
        assert!(run.admits(INPUT));
    }

    #[test]
    fn a_missing_input_keeps_its_own_question_and_checked_bytes() {
        let (_dir, mut runtime, seen) = session(INPUT, TurnAct::RequestRun);
        let Some(TurnOutcome::Question { key, .. }) = runtime.natural_run_turn(AGAIN) else {
            panic!("the declared input is still required");
        };
        assert_eq!(key, "input.name");
        assert_eq!(runtime.pending_input(), Some("name"));
        assert_eq!(runtime.natural_run_turn(AGAIN), None);
        assert_eq!(
            seen.lock().expect("record").len(),
            1,
            "the input owns the next line"
        );
        let TurnOutcome::RunRequested { run, .. } = runtime.answer_input_unrecorded("Thibaut")
        else {
            panic!("the input completes the existing admission");
        };
        assert_eq!(run.vars, ["name=Thibaut"]);
        assert!(run.admits(INPUT));
    }

    #[test]
    fn no_saved_workflow_or_a_waiting_proposal_cannot_reach_the_classifier() {
        let (dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
        runtime.last_workflow = None;
        assert_eq!(runtime.natural_run_turn(AGAIN), None);
        runtime.last_workflow = Some(PathBuf::from("saved.nika"));
        runtime.pending = Some(
            ProjectChangeSet::workflow_at(
                dir.path(),
                "another change",
                "another.nika",
                WORKFLOW.to_owned(),
            )
            .expect("proposal"),
        );
        assert_eq!(runtime.natural_run_turn(AGAIN), None);
        assert!(runtime.pending.is_some());
        assert!(seen.lock().expect("record").is_empty());
    }

    #[test]
    fn a_waiting_review_or_choice_is_not_answered_by_a_rerun() {
        let (_dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
        runtime.pending_choice = true;
        assert_eq!(runtime.natural_run_turn(AGAIN), None);
        assert!(runtime.pending_choice);
        runtime.pending_choice = false;
        let review = runtime.run_review_asked("Continue?", "The actual run's cost review");
        assert_eq!(runtime.natural_run_turn(AGAIN), None);
        assert_eq!(runtime.waiting_review(), Some(review));
        assert!(seen.lock().expect("record").is_empty());
    }

    #[test]
    fn an_existing_run_observation_does_not_block_a_later_rerun() {
        let (_dir, mut runtime, _) = session(WORKFLOW, TurnAct::RequestRun);
        assert!(matches!(
            runtime.natural_run_turn(AGAIN),
            Some(TurnOutcome::RunRequested { .. })
        ));
        assert!(runtime.requested_run.is_some());
        assert!(matches!(
            runtime.natural_run_turn(AGAIN),
            Some(TurnOutcome::RunRequested { .. })
        ));
    }

    #[test]
    fn an_invalid_ceiling_or_run_option_never_reaches_execution() {
        for line in [
            "Peux-tu relancer à 9 heures ?",
            "Peux-tu relancer --max-cost-usd -1 ?",
            "Peux-tu relancer --access mock --access mock ?",
        ] {
            let (_dir, mut runtime, _) = session(WORKFLOW, TurnAct::RequestRun);
            assert!(
                matches!(
                    runtime.natural_run_turn(line),
                    Some(TurnOutcome::Refusal(_))
                ),
                "{line}"
            );
            assert!(runtime.requested_run.is_none());
        }
    }

    #[test]
    fn a_different_named_workflow_is_not_replaced_with_the_saved_one() {
        for named in [
            "another.nika",
            "./another.nika",
            "`./another.nika`",
            "./another.nika.",
            "./another.nika!",
            "nested/saved.nika",
            "../saved.nika",
            "./nested/../saved.nika",
        ] {
            let (_dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
            let line = format!("Peux-tu relancer {named} ?");
            assert_eq!(runtime.natural_run_turn(&line), None, "{named}");
            assert!(runtime.requested_run.is_none(), "{named}");
            assert!(seen.lock().expect("record").is_empty(), "{named}");
        }
    }

    #[test]
    fn a_relative_workflow_name_matches_its_absolute_saved_path_beside_a_source_file() {
        let (_dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
        let saved = runtime.snapshot.root.join("saved.nika");
        runtime.last_workflow = Some(saved.clone());
        let line = "Le fichier ./sales.csv est rétabli, peux-tu relancer ./saved.nika tel quel ?";
        let outcome = runtime.natural_run_turn(line);
        let Some(TurnOutcome::RunRequested { run, .. }) = outcome else {
            panic!("the same workflow is requested by its relative name: {outcome:?}");
        };
        assert_eq!(run.workflow, PathBuf::from("saved.nika"));
        assert!(run.admits(WORKFLOW));
        assert_eq!(seen.lock().expect("record")[0].1, line);
    }

    #[test]
    fn an_absolute_reference_inside_the_project_runs_its_relative_saved_path() {
        let (_dir, mut runtime, _) = session(WORKFLOW, TurnAct::RequestRun);
        let saved = runtime.snapshot.root.join("saved.nika");
        let line = format!("Relance `{}` tel quel.", saved.display());
        let Some(TurnOutcome::RunRequested { run, .. }) = runtime.natural_run_turn(&line) else {
            panic!("the absolute name designates this project's saved workflow");
        };
        assert_eq!(run.workflow, PathBuf::from("saved.nika"));
        assert!(run.admits(WORKFLOW));
    }

    #[test]
    fn an_outside_or_parent_relative_saved_path_cannot_reach_the_classifier() {
        for saved in [
            "/another-project/saved.nika",
            "../saved.nika",
            "nested/../saved.nika",
        ] {
            let (_dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
            runtime.last_workflow = Some(PathBuf::from(saved));
            assert_eq!(runtime.natural_run_turn(AGAIN), None);
            assert!(runtime.requested_run.is_none());
            assert!(seen.lock().expect("record").is_empty());
        }
    }

    #[test]
    fn a_natural_first_run_names_the_actual_file_without_preparing_or_saving() {
        let (dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
        runtime.last_workflow = None;
        runtime.intent.goal = None;
        let line = "Lance ./saved.nika tel quel.";
        let outcome = runtime.natural_run_turn(line);
        let Some(TurnOutcome::RunRequested { run, .. }) = outcome else {
            panic!("first Run of the explicitly named file: {outcome:?}");
        };
        assert_eq!(run.workflow, PathBuf::from("saved.nika"));
        assert!(run.admits(WORKFLOW));
        assert!(runtime.pending.is_none());
        assert!(runtime.authoring.is_none());
        assert!(runtime.intent.goal.is_none());
        assert!(!dir.path().join("copy.txt").exists());
        assert!(!dir.path().join(".nika/consents.ndjson").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("saved.nika")).expect("bytes"),
            WORKFLOW
        );
        let calls = seen.lock().expect("record");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, line);
        assert_eq!(calls[0].0.phase, SessionPhase::Idle);
        assert_eq!(
            calls[0].0.automation.as_deref(),
            Some(
                "The person's line explicitly names the existing project workflow `saved.nika`. It can be run unchanged through Run admission; no new candidate is needed for that act. Its existence does not authorize a Run or settle a request to change it."
            )
        );
    }

    #[test]
    fn a_unique_named_existing_target_is_not_replaced_by_the_last_workflow() {
        let (dir, mut runtime, _) = session(WORKFLOW, TurnAct::RequestRun);
        std::fs::create_dir(dir.path().join("archive")).expect("archive");
        std::fs::write(dir.path().join("archive/other.nika"), WORKFLOW).expect("other");
        let Some(TurnOutcome::RunRequested { run, .. }) = runtime.natural_run_turn(
            "Lance `archive/other.nika` tel quel, précisément ./archive/other.nika.",
        ) else {
            panic!("the uniquely named existing target reaches Run");
        };
        assert_eq!(run.workflow, PathBuf::from("archive/other.nika"));
        assert!(run.admits(WORKFLOW));
        assert_eq!(
            runtime.last_workflow,
            Some(PathBuf::from("archive/other.nika"))
        );
    }

    #[test]
    fn existing_named_files_do_not_authorize_changes_or_discussion() {
        for act in [
            TurnAct::Modify,
            TurnAct::Mixed,
            TurnAct::Discuss,
            TurnAct::Cancel,
            TurnAct::Unknown,
        ] {
            let (_dir, mut runtime, seen) = session(WORKFLOW, act);
            runtime.last_workflow = None;
            assert_eq!(
                runtime.natural_run_turn("Avant de lancer ./saved.nika, change sa destination."),
                None
            );
            assert_eq!(seen.lock().expect("record").len(), 1);
            assert!(runtime.last_workflow.is_none());
            assert!(runtime.requested_run.is_none());
        }
        let (_dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
        runtime.last_workflow = None;
        let mut decision = TurnDecision::new(TurnAct::RequestRun, RoutingMethod::Model);
        decision.secondary.push(TurnAct::Modify);
        runtime.with_classifier(Box::new(Reading { decision, seen }));
        assert_eq!(
            runtime.natural_run_turn("Lance ./saved.nika en changeant sa destination."),
            None
        );
        assert!(runtime.last_workflow.is_none());
        assert!(runtime.requested_run.is_none());
    }

    #[test]
    fn ambiguous_missing_and_escaping_named_targets_never_fall_back_to_the_last_one() {
        for line in [
            "Lance saved.nika et second.nika.",
            "Lance missing.nika.",
            "Lance ../saved.nika.",
            "Lance nested/../saved.nika.",
            "Lance /outside/saved.nika.",
            "Lance folder.nika.",
            "Lance `missing folder/saved.nika`.",
            "Lance `saved.nika.",
        ] {
            let (dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
            std::fs::write(dir.path().join("second.nika"), WORKFLOW).expect("second");
            std::fs::create_dir(dir.path().join("folder.nika")).expect("directory");
            assert_eq!(runtime.natural_run_turn(line), None, "{line}");
            assert!(seen.lock().expect("record").is_empty(), "{line}");
            assert_eq!(runtime.last_workflow, Some(PathBuf::from("saved.nika")));
            assert!(runtime.requested_run.is_none());
        }
    }

    #[test]
    fn a_quoted_workflow_path_keeps_spaces_in_its_actual_identity() {
        let (dir, mut runtime, _) = session(WORKFLOW, TurnAct::RequestRun);
        runtime.last_workflow = None;
        std::fs::write(dir.path().join("my  saved.nika"), INPUT).expect("spaced file");
        let Some(TurnOutcome::Question { key, .. }) =
            runtime.natural_run_turn("Lance `my  saved.nika` tel quel.")
        else {
            panic!("the named file's own required input must wait");
        };
        assert_eq!(key, "input.name");
        assert_eq!(runtime.pending_input(), Some("name"));
        let TurnOutcome::RunRequested { run, .. } = runtime.answer_input_unrecorded("Thibaut")
        else {
            panic!("the existing input admission completes");
        };
        assert_eq!(run.workflow, PathBuf::from("my  saved.nika"));
        assert_eq!(run.vars, ["name=Thibaut"]);
        assert!(run.admits(INPUT));
    }

    #[cfg(unix)]
    #[test]
    fn linked_named_workflows_and_linked_parent_directories_never_enter_cognition() {
        use std::os::unix::fs::symlink;
        for target in ["alias.nika", "linked/saved.nika"] {
            let (dir, mut runtime, seen) = session(WORKFLOW, TurnAct::RequestRun);
            symlink(dir.path().join("saved.nika"), dir.path().join("alias.nika"))
                .expect("file link");
            symlink(dir.path(), dir.path().join("linked")).expect("directory link");
            assert_eq!(runtime.natural_run_turn(&format!("Lance {target}.")), None);
            assert!(seen.lock().expect("record").is_empty());
            assert_eq!(runtime.last_workflow, Some(PathBuf::from("saved.nika")));
            assert!(runtime.requested_run.is_none());
        }
    }

    #[test]
    fn a_first_named_run_still_requires_the_actual_bytes_to_pass_check() {
        let (dir, mut runtime, _) = session(WORKFLOW, TurnAct::RequestRun);
        runtime.last_workflow = None;
        std::fs::write(dir.path().join("saved.nika"), "nika: broken\ntasks:\n  task:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n")
            .expect("invalid");
        let outcome = runtime.natural_run_turn("Lance ./saved.nika tel quel.");
        let Some(TurnOutcome::Facts(text)) = outcome else {
            panic!("the canonical check holds invalid source: {outcome:?}");
        };
        assert!(text.contains("findings"));
        assert!(runtime.last_workflow.is_none());
        assert!(runtime.requested_run.is_none());
    }
}
