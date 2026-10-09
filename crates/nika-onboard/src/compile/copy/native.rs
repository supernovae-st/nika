// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One synchronous compile dispatch owns this account. A host call is admitted before it
//! starts, drained before its usage is charged, and never grants Save or Run. Its live preview
//! is taken once by the proposal boundary; serialized compiler records cannot recreate it.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use super::{Admission, Allowance, Budget, RunEnd, Usage, Witness, WorldBefore};
use nika_compile::surface::literal_projection;
use nika_compile_cognition::rehearse::{
    Attempt, Composed, EffectCounts, Observation, Refusal, Rehearsal, RehearsalFuture,
    RehearsalReport, Rehearse, judged_run,
};
use nika_compile_fidelity::fidelity::stated_routes;
use nika_event::source_id::sha256_hex;

/// Evidence produced in this dispatch only, with no public constructor or writable fields.
/// A completed result contains a checked world witness; a source-only refusal contains none.
#[non_exhaustive]
pub struct Preview {
    candidate_sha256: String,
    witness: Option<Witness>,
    lines: String,
}

impl Preview {
    /// The exact candidate this live decision names.
    #[must_use]
    pub fn candidate_sha256(&self) -> &str {
        &self.candidate_sha256
    }

    /// Consume the live evidence and its words. The caller owns proposal identity and consent.
    #[must_use]
    pub fn into_parts(self) -> (Option<Witness>, String) {
        (self.witness, self.lines)
    }
}

/// The drained dispatch's consumption, last live evidence and any fail-closed account refusal.
#[non_exhaustive]
pub struct Settled {
    turn: Usage,
    preview: Option<Preview>,
    blocked: Option<String>,
}

impl Settled {
    /// Consume the account. Charge its usage even if the caller's compilation failed.
    /// A blocked account discards its preview instead of exposing evidence beside a refusal.
    #[must_use = "charge the returned usage and handle refusals before using a preview"]
    pub fn into_parts(self) -> (Usage, Result<Option<Preview>, String>) {
        let preview = match self.blocked {
            Some(why) => Err(why),
            None => Ok(self.preview),
        };
        (self.turn, preview)
    }
}

struct State {
    turn: Usage,
    in_flight: bool,
    preview: Option<Preview>,
    blocked: Option<String>,
}

/// One compile dispatch's serialized rehearsal account over an explicitly supplied host.
/// It owns no proposal, consent, source basis, or persistent proof.
#[non_exhaustive]
pub struct Scoped {
    root: PathBuf,
    host: Box<dyn Rehearse>,
    allowance: Allowance,
    /// The request its candidates are compiled for, when the caller names it.
    intent: Option<String>,
    state: Mutex<State>,
}

impl Scoped {
    /// Start with the caller's existing turn consumption and round/turn limits.
    #[must_use]
    pub fn new(root: PathBuf, host: Box<dyn Rehearse>, allowance: Allowance) -> Self {
        Self {
            root,
            host,
            allowance,
            intent: None,
            state: Mutex::new(State {
                turn: allowance.before,
                in_flight: false,
                preview: None,
                blocked: None,
            }),
        }
    }

    /// The same account over `intent`, the request its candidates are compiled for: a
    /// destination it states that a candidate sends to as an endpoint's route
    /// ([`stated_routes`]) is no file, so the world observed before that rehearsal leaves it
    /// out, while the host is still handed it. Without it, every destination is a file.
    #[must_use]
    pub fn stating(mut self, intent: &str) -> Self {
        self.intent = Some(intent.to_owned());
        self
    }

    /// The `targets` the world before a rehearsal of `candidate` observes as files: each one,
    /// but a route the request states ([`Self::stating`]).
    fn files(&self, candidate: &str, targets: &[String]) -> Vec<String> {
        let routes = (self.intent.as_deref())
            .and_then(|intent| literal_projection(candidate).map(|doc| stated_routes(intent, &doc)))
            .unwrap_or_default();
        let mut files = targets.to_vec();
        files.retain(|target| !routes.contains(target));
        files
    }

    fn state(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.blocked = Some("the rehearsal account was interrupted".to_owned());
                state
            }
        }
    }

    fn begin(&self) -> Result<Budget, String> {
        let mut state = self.state();
        state.preview = None;
        if let Some(reason) = &state.blocked {
            return Err(reason.clone());
        }
        if state.in_flight {
            let why = "another rehearsal in this dispatch has not drained".to_owned();
            state.blocked = Some(why.clone());
            return Err(why);
        }
        // Each native candidate has one observed world. The turn is carried, never reset.
        let account = Budget::new(self.allowance.round, self.allowance.turn, state.turn);
        if account.admission() != Admission::Open {
            let why = "the shared turn rehearsal budget admits no further world".to_owned();
            state.blocked = Some(why.clone());
            return Err(why);
        }
        state.in_flight = true;
        Ok(account)
    }

    /// Consume this dispatch after compilation; an in-flight or poisoned account is blocked.
    #[must_use]
    pub fn finish(self) -> Settled {
        let mut state = match self.state.into_inner() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.blocked = Some("the rehearsal account was interrupted".to_owned());
                state
            }
        };
        if state.in_flight {
            state.blocked = Some("the rehearsal did not finish draining".to_owned());
        }
        Settled {
            turn: state.turn,
            preview: state.preview,
            blocked: state.blocked,
        }
    }

    async fn observe(
        &self,
        candidate: &str,
        inputs: &[String],
        targets: &[String],
    ) -> RehearsalReport {
        let mut account = match self.begin() {
            Ok(account) => account,
            Err(why) => return refused(candidate, why, Refusal::NotBuilt),
        };
        let files = self.files(candidate, targets);
        let before = match WorldBefore::capture(&self.root, candidate, inputs, &files).await {
            Ok(before) => before,
            Err(why) => {
                let mut state = self.state();
                state.in_flight = false;
                state.blocked = Some(format!("the original world could not be observed: {why}"));
                return refused(candidate, why, Refusal::CopyIn);
            }
        };
        // No second timeout abandons this future: the room owns its stop and drainage.
        let report = self.host.rehearse_reading(candidate, inputs, targets).await;
        let declared = match &report.outcome {
            Rehearsal::Passed { outputs } => outputs.iter().map(|o| o.path.clone()).collect(),
            Rehearsal::Missing { outputs } => outputs.clone(),
            _ => Vec::new(),
        };
        let run = judged_run("observed", &report, inputs, targets, &declared);
        let charged = account.charge(&run.usage);
        {
            let mut state = self.state();
            state.turn = account.turn();
            if charged != Admission::Open {
                state.blocked =
                    Some("the observed rehearsal exceeded its shared budget".to_owned());
            }
        }
        let identity = report.candidate_sha256 == sha256_hex(candidate.as_bytes());
        let preview = if charged != Admission::Open || !identity {
            None
        } else if matches!(
            (&report.attempt, &report.outcome, &run.end),
            (
                Attempt::NeverAttempted,
                Rehearsal::NotRun { .. },
                RunEnd::NotRun { .. }
            )
        ) {
            match &report.outcome {
                Rehearsal::NotRun { reason } => Some(Preview {
                    candidate_sha256: report.candidate_sha256.clone(),
                    witness: None,
                    lines: format!(
                        "Rehearsal not run · {reason} · source-only preview · no output observed · `yes` saves only; running is separate\n"
                    ),
                }),
                _ => None,
            }
        } else if matches!(
            (&report.outcome, &run.end),
            (Rehearsal::Passed { .. }, RunEnd::Completed)
        ) {
            match before.witness(candidate, &report).await {
                Ok(witness) => Some(Preview {
                    candidate_sha256: witness.candidate_sha256().to_owned(),
                    witness: Some(witness),
                    lines: passed_lines(run.read_back.iter().map(|read| {
                        (
                            read.path.as_str(),
                            read.text.as_str(),
                            read.written,
                            read.truncated,
                        )
                    })),
                }),
                Err(why) => {
                    self.state().blocked = Some(format!(
                        "the rehearsal cannot bind the original world: {why}"
                    ));
                    None
                }
            }
        } else {
            None
        };
        let mut state = self.state();
        state.in_flight = false;
        state.preview = preview;
        report
    }
}

impl Rehearse for Scoped {
    fn bound(&self) -> Duration {
        self.host.bound()
    }

    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }

    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(self.observe(candidate, inputs, targets))
    }

    /// The host's own read-only composition check, where its host saves the bytes: a scope adds
    /// no location and spends no rehearsal on it.
    fn compose(&self, candidate: &str) -> Composed {
        self.host.compose(candidate)
    }
}

fn refused(candidate: &str, reason: String, refusal: Refusal) -> RehearsalReport {
    RehearsalReport::new(
        Rehearsal::NotRun { reason },
        Attempt::NeverAttempted,
        EffectCounts::none(),
        sha256_hex(candidate.as_bytes()),
    )
    .with_observation(Observation::refused(refusal))
}

// Readback was validated by judged_run, and the before/after witness covered every written path.
fn passed_lines<'a>(reads: impl Iterator<Item = (&'a str, &'a str, bool, bool)>) -> String {
    let mut lines = "Rehearsed on a copy of your files · nothing ran on the originals · observed result of this candidate · `yes` saves these exact bytes; running is a separate line (« run it »)\n".to_owned();
    let mut observed = false;
    for (path, text, written, truncated) in reads {
        observed = true;
        let publication = if written {
            "written by the run"
        } else {
            "not written by the run"
        };
        let coverage = if truncated {
            "prefix only"
        } else {
            "whole text observed"
        };
        let excerpt: String = text.chars().take(240).collect();
        let _ = writeln!(
            lines,
            "  read back · `{path}` · {publication} · {coverage} · excerpt {excerpt:?}"
        );
    }
    if !observed {
        lines.push_str("  no text output observed\n");
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::room::ObservedRoom;
    use std::sync::Arc;

    fn synthetic_preview() -> Preview {
        Preview {
            candidate_sha256: "synthetic-candidate".into(),
            witness: Some(Witness::new("synthetic-candidate".into(), Vec::new())),
            lines: "synthetic evidence".into(),
        }
    }

    #[test]
    fn a_blocked_account_cannot_release_a_live_preview_but_keeps_its_usage() {
        // An overlapping begin can block while the first observation later completes.
        // Build that settled state directly: no host, concurrency, or workflow executes.
        let spent = Usage::new(1, 1, 10, 20, 30);
        let (charged, result) = Settled {
            turn: spent,
            preview: Some(synthetic_preview()),
            blocked: Some("another rehearsal has not drained".into()),
        }
        .into_parts();
        assert_eq!(charged, spent);
        assert!(matches!(result, Err(why) if why == "another rehearsal has not drained"));
    }

    /// A host whose only answer is its composition check.
    struct Composing(Composed);

    impl Rehearse for Composing {
        fn rehearse<'a>(&'a self, candidate: &'a str, _: &'a [String]) -> RehearsalFuture<'a> {
            Box::pin(async move {
                let outcome = Rehearsal::NotRun {
                    reason: "no room in this test".into(),
                };
                let digest = sha256_hex(candidate.as_bytes());
                RehearsalReport::new(
                    outcome,
                    Attempt::NeverAttempted,
                    EffectCounts::none(),
                    digest,
                )
            })
        }
        fn bound(&self) -> Duration {
            Duration::from_secs(1)
        }
        fn compose(&self, _candidate: &str) -> Composed {
            self.0.clone()
        }
    }

    #[test]
    fn a_scope_forwards_its_hosts_composition_check_and_spends_no_rehearsal() {
        let limits = super::super::Limits::new(1, 1, 1024, 1_000);
        let nothing = Usage::new(0, 0, 0, 0, 0);
        let refused = Composed::Refused {
            reason: "NIKA-COMP-001 the child is missing".into(),
        };
        let host = Box::new(Composing(refused.clone()));
        let scoped = Scoped::new(
            PathBuf::from("."),
            host,
            Allowance::new(limits, limits, nothing),
        );
        assert_eq!(scoped.compose("nika: x\n"), refused);
        let (spent, _) = scoped.finish().into_parts();
        assert_eq!(spent, nothing, "no rehearsal was spent");
    }

    #[test]
    fn an_open_account_releases_its_evidence_and_usage_unchanged() {
        let spent = Usage::new(1, 1, 10, 20, 30);
        let (charged, result) = Settled {
            turn: spent,
            preview: Some(synthetic_preview()),
            blocked: None,
        }
        .into_parts();
        assert_eq!(charged, spent);
        let preview = result.expect("an open account").expect("the live evidence");
        assert_eq!(preview.candidate_sha256(), "synthetic-candidate");
        let (witness, lines) = preview.into_parts();
        assert_eq!(witness.unwrap().candidate_sha256(), "synthetic-candidate");
        assert_eq!(lines, "synthetic evidence");
    }

    /// The stock request in brief: its source, its report and the route of the sink it states.
    const STOCK: &str = "Read ./in.txt, write the report to ./out.txt, then send a POST to \
        /notifications/stock on the local sink http://127.0.0.1:57468.";

    /// The route the stock request states.
    const ROUTE: &str = "/notifications/stock";

    /// A candidate for it: it reads the source, writes the report and sends a POST to the route.
    const POSTING: &str = r#"nika: stock-alerts
permits:
  tools: ["nika:read", "nika:write", "nika:fetch"]
  fs:
    read: ["./in.txt"]
    write: ["./out.txt"]
  net:
    http: ["127.0.0.1"]
tasks:
  source:
    invoke: { tool: "nika:read", args: { path: "./in.txt" } }
  report:
    invoke: { tool: "nika:write", args: { path: "./out.txt", content: "report" } }
  post:
    invoke:
      tool: "nika:fetch"
      args: { url: "http://127.0.0.1:57468/notifications/stock", method: POST, body: "{}" }
"#;

    /// The same candidate, also writing the route as a file it permits: a file stays a file.
    const KEPT: &str = r#"nika: stock-alerts
permits:
  tools: ["nika:read", "nika:write", "nika:fetch"]
  fs:
    read: ["./in.txt"]
    write: ["./out.txt", "/notifications/stock"]
  net:
    http: ["127.0.0.1"]
tasks:
  source:
    invoke: { tool: "nika:read", args: { path: "./in.txt" } }
  report:
    invoke: { tool: "nika:write", args: { path: "./out.txt", content: "report" } }
  kept:
    invoke: { tool: "nika:write", args: { path: "/notifications/stock", content: "kept" } }
  post:
    invoke:
      tool: "nika:fetch"
      args: { url: "http://127.0.0.1:57468/notifications/stock", method: POST, body: "{}" }
"#;

    /// The targets each rehearsal was handed, in order.
    type Handed = Arc<Mutex<Vec<Vec<String>>>>;

    /// The real observed room over a project, recording the targets each rehearsal is handed.
    struct Recording {
        room: ObservedRoom,
        seen: Handed,
    }

    impl Rehearse for Recording {
        fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
            self.rehearse_reading(candidate, inputs, &[])
        }
        fn rehearse_reading<'a>(
            &'a self,
            candidate: &'a str,
            inputs: &'a [String],
            targets: &'a [String],
        ) -> RehearsalFuture<'a> {
            self.seen.lock().unwrap().push(targets.to_vec());
            self.room.rehearse_reading(candidate, inputs, targets)
        }
        fn bound(&self) -> Duration {
            self.room.bound()
        }
    }

    /// A project holding the request's source, and an account over the recorded real room
    /// there, told `intent` when one is given.
    fn project(intent: Option<&str>) -> (tempfile::TempDir, Scoped, Handed) {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("in.txt"), "stock").unwrap();
        let seen = Handed::default();
        let host = Box::new(Recording {
            room: ObservedRoom::new(root.path()),
            seen: Arc::clone(&seen),
        });
        let limits = super::super::Limits::new(3, 3, 1024 * 1024, 30_000);
        let allowance = Allowance::new(limits, limits, Usage::new(0, 0, 0, 0, 0));
        let scoped = Scoped::new(root.path().to_owned(), host, allowance);
        let scoped = match intent {
            Some(intent) => scoped.stating(intent),
            None => scoped,
        };
        (root, scoped, seen)
    }

    /// The stock request: its report is a file of the project, its route an endpoint's. The
    /// world observed before the trial holds the file alone; the room is handed both and refuses
    /// the send before any run (it opens no socket); the account settles open on a source-only
    /// decision, and nothing was written.
    #[tokio::test]
    async fn a_stated_route_is_no_file_of_the_observed_world_and_the_trial_proceeds() {
        let (root, scoped, seen) = project(Some(STOCK));
        let inputs = ["./in.txt".to_owned()];
        let targets = ["./out.txt".to_owned(), ROUTE.to_owned()];
        let report = scoped.rehearse_reading(POSTING, &inputs, &targets).await;
        assert_eq!(report.observation.refusal, Some(Refusal::Effect));
        assert_eq!(*seen.lock().unwrap(), [targets.to_vec()]);
        assert_eq!(scoped.files(POSTING, &targets), ["./out.txt"]);
        let (spent, preview) = scoped.finish().into_parts();
        assert_eq!(spent, Usage::new(1, 0, 0, 0, 0));
        let preview = preview.unwrap().expect("a live source-only decision");
        let (witness, lines) = preview.into_parts();
        assert!(witness.is_none(), "no run binds a world");
        let opening = "Rehearsal not run · task post needs the network";
        assert!(lines.starts_with(opening), "{lines}");
        assert!(!root.path().join("out.txt").exists());
    }

    /// Still no path inside the project, in today's words, with no room asked: the route of a
    /// request that states no sink, a send to another origin, an account told no request, the
    /// route the candidate also writes as a file, and a rooted or escaping file named before
    /// the stated route.
    #[tokio::test]
    async fn an_unstated_route_or_a_file_outside_the_project_still_blocks_the_account() {
        let unstated = STOCK.replace(" on the local sink http://127.0.0.1:57468", "");
        let elsewhere = POSTING.replace("127.0.0.1:57468", "127.0.0.1:9");
        let cases = [
            (Some(unstated.as_str()), POSTING, ROUTE),
            (Some(STOCK), elsewhere.as_str(), ROUTE),
            (None, POSTING, ROUTE),
            (Some(STOCK), KEPT, ROUTE),
            (Some(STOCK), POSTING, "/outside/report.json"),
            (Some(STOCK), POSTING, "../report.json"),
        ];
        for (intent, candidate, refused) in cases {
            let (_root, scoped, seen) = project(intent);
            let inputs = ["./in.txt".to_owned()];
            let mut targets = Vec::from(["./out.txt", refused, ROUTE].map(str::to_owned));
            targets.dedup();
            let report = scoped.rehearse_reading(candidate, &inputs, &targets).await;
            let words = format!("`{refused}` is not a path inside the project");
            let refusal = report.observation.refusal;
            assert_eq!(refusal, Some(Refusal::CopyIn), "{refused}");
            assert!(matches!(&report.outcome, Rehearsal::NotRun { reason } if *reason == words));
            assert!(seen.lock().unwrap().is_empty(), "{refused}");
            let (_, preview) = scoped.finish().into_parts();
            let why = format!("the original world could not be observed: {words}");
            assert_eq!(preview.err(), Some(why), "{intent:?}");
        }
    }
}
