// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The existing loopback transport and real room, with per-invocation receipts.
use super::*;

pub(super) struct Case {
    pub project: PathBuf,
    pub rooms: PathBuf,
}

impl Case {
    pub(super) fn new() -> Self {
        // Keep the owned case even on a failed assertion; the outer qualification receipt owns
        // its cleanup after evidence inspection. HOME and TMPDIR are supplied by that runner.
        let case = tempfile::Builder::new()
            .prefix("nika-native-edit-")
            .tempdir()
            .expect("owned native edit fixture")
            .keep();
        let project = case.join("project");
        let rooms = case.join("rooms");
        std::fs::create_dir_all(&project).expect("fixture project directory");
        std::fs::create_dir_all(&rooms).expect("fixture rooms directory");
        write(&project, SOURCE, ORIGINAL);
        write(&project, "out/copied.txt", "stale copied target");
        write(&project, "out/note.txt", "stale note target");
        write(&project, "witness.txt", "unchanged witness");
        Self { project, rooms }
    }
}

pub(super) fn session(case: &Case, calls: Arc<Calls>) -> SessionRuntime {
    let selected = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        model: Some(MODEL.into()),
        locus: DataLocus::Metered {
            provider: "deepseek".into(),
        },
        ready: true,
        why: None,
    };
    let mut s = SessionRuntime::open(
        &case.project,
        selected,
        Box::new(ProviderReasoner {
            model: MODEL.into(),
            label: "Synthetic local transport".into(),
        }),
    );
    s.set_authoring_context(AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none().with_strategy("only"),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    ));
    s.admit_money("budget 2 USD", false, false)
        .expect("synthetic admission");
    s.with_classifier(Box::new(Acts));
    let scratch = case.rooms.clone();
    s.with_rehearsal_host(Arc::new(move |world| {
        Box::new(Observed {
            room: ObservedRoom::new(world).with_scratch_parent(&scratch),
            calls: calls.clone(),
        })
    }));
    s
}

struct Acts;
impl TurnClassifier for Acts {
    fn classify_with_admission(
        &mut self,
        context: &TurnContext,
        raw: &str,
        _: &nika_providers::InferenceAdmission,
    ) -> TurnDecision {
        self.classify(context, raw)
    }
    fn classify(&mut self, context: &TurnContext, raw: &str) -> TurnDecision {
        let act = if raw == ADD {
            TurnAct::Modify
        } else if context.phase == SessionPhase::QuestionPending {
            TurnAct::Answer
        } else {
            TurnAct::Unknown
        };
        TurnDecision::new(act, RoutingMethod::Model)
    }
}

pub(super) fn peer() -> Peer {
    let native = |candidate, questions| {
        json!({"candidate": candidate, "questions": questions, "gaps": [], "notes": ""}).to_string()
    };
    let question = json!([{"key":"const.note_text", "label":"What should the additional note say?", "answer_type":"text", "why":"The note's content is the human's choice."}]);
    let original = native(BASE, json!([]));
    let addition = native(ADDITION, question);
    let judge = r#"{"choice":"faithful"}"#;
    Peer::start(vec![
        (200, response(&original)),
        (200, response(judge)),
        (200, response(&addition)),
        (200, response(&addition)),
        // A multiword content answer is read verbatim before its recorded edit is judged.
        (200, response(NOTE)),
        (200, response(judge)),
        // Peer repeats its last response on extra traffic. An unplanned request receives a
        // failure, never another valid author/judge answer; every stage also asserts the count.
        (
            400,
            json!({"error":{"message":"unplanned synthetic transport request"}}),
        ),
    ])
}

pub(super) fn assert_wire(peer: &Peer) {
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 6);
    assert!(bodies.iter().all(|body| body["model"] == "deepseek-v4-pro"));
    let judged = |body: &Value| body.to_string().contains("unfaithful");
    assert!(!judged(&bodies[0]));
    assert!(bodies[0].to_string().contains(DIRECT));
    assert!(bodies[2].to_string().contains(ADD) && bodies[3].to_string().contains(ADD));
    assert!(bodies[4].to_string().contains(NOTE));
    assert!(bodies[4].to_string().contains("Copy that value exactly"));
    assert!(!judged(&bodies[4]), "the answer is read before replay");
    assert!(bodies[5].to_string().contains(NOTE));
    assert!(judged(&bodies[1]));
    assert!(!judged(&bodies[2]) && !judged(&bodies[3]));
    assert!(
        judged(&bodies[5]),
        "the answer replays and judges; it must not re-author"
    );
}

#[derive(Default)]
pub(super) struct Calls {
    next: AtomicUsize,
    reports: Mutex<Vec<RehearsalReport>>,
}
impl Calls {
    pub(super) fn count(&self) -> usize {
        self.next.load(Ordering::SeqCst)
    }
    pub(super) fn assert_outputs(&self, index: usize, added: bool) {
        let reports = self.reports.lock().expect("captured room reports");
        let report = &reports[index];
        assert!(matches!(report.attempt, Attempt::Completed { .. }));
        assert!(matches!(report.outcome, Rehearsal::Passed { .. }));
        assert!(report.room.prepared && report.room.cleaned);
        assert_eq!(report.room.late_refused, 0);
        assert_eq!(report.effects, EffectCounts::none());
        let ledger = &report.observation.ledger;
        assert!(ledger.drained);
        assert_eq!(ledger.panicked, 0);
        assert_eq!(ledger.leftovers, 0);
        assert_eq!(ledger.late_refused, 0);
        assert_eq!(report.observation.finals.len(), if added { 2 } else { 1 });
        output(report, "out/copied.txt", ORIGINAL);
        if added {
            output(report, "out/note.txt", NOTE);
        }
        assert_eq!(ledger.written.len(), if added { 2 } else { 1 });
    }
}

fn output(report: &RehearsalReport, path: &str, expected: &str) {
    let matches: Vec<_> = report
        .observation
        .finals
        .iter()
        .filter(|entry| entry.path.strip_prefix("./").unwrap_or(&entry.path) == path)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "one complete readback per requested target"
    );
    let FinalState::File {
        digest,
        held: Held::Whole(text),
    } = &matches[0].state
    else {
        panic!("a whole text readback is required")
    };
    assert!(
        text == expected,
        "actual repeated behavior must match the requested content"
    );
    assert_eq!(digest, &Digest::of(expected.as_bytes()));
}

struct Observed {
    room: ObservedRoom,
    calls: Arc<Calls>,
}
impl Rehearse for Observed {
    fn bound(&self) -> Duration {
        self.room.bound()
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
        Box::pin(async move {
            // Allocate at invocation, never at factory creation: repairs may reuse this host.
            let subrun = self.calls.next.fetch_add(1, Ordering::SeqCst) + 1;
            room_call(
                &json!({"schema":"native-edit-room/1", "event":"BEGIN", "test":TEST, "subrun":subrun, "candidate_sha256":sha256_hex(candidate.as_bytes()), "candidate_bytes":candidate.len()}),
            );
            let report = self.room.rehearse_reading(candidate, inputs, targets).await;
            let mut receipt = returned(TEST, subrun, &report);
            receipt["schema"] = json!("native-edit-room/1");
            // The projection intentionally omits arbitrary diagnostic task/code strings.
            receipt["failed"] =
                json!({"present": matches!(report.outcome, Rehearsal::Failed { .. })});
            room_call(&receipt);
            self.calls
                .reports
                .lock()
                .expect("captured room reports")
                .push(report.clone());
            report
        })
    }
}

/// Snapshot returned-call accounting before any assertion about the call's result.
/// A panic before return, or inside Peer observation, leaves these totals unavailable.
pub(super) fn record_counts(s: &SessionRuntime, peer: &Peer, calls: &Calls, stage: &str) {
    let bodies = peer.bodies();
    let reports = calls.reports.lock().ok();
    let returned = reports.as_ref().map(|reports| reports.len());
    let completed = reports.as_ref().map(|reports| {
        reports
            .iter()
            .filter(|report| matches!(report.attempt, Attempt::Completed { .. }))
            .count()
    });
    let (inference_attempts, inference_state) = match s.inference_receipt() {
        Ok(Some(receipt)) => (Some(receipt.attempts.len()), "available"),
        Ok(None) => (None, "absent"),
        Err(_) => (None, "unavailable"),
    };
    room_call(&json!({
        "schema":"native-edit-counts/1", "test":TEST, "stage":stage,
        "transport_requests":bodies.len(), "host_begins":calls.count(),
        "host_returns":returned, "completed_attempt_reports":completed,
        "report_record_available":reports.is_some(),
        "shared_turn_attempts":s.rehearsals.turn.attempts,
        "inference_attempts":inference_attempts, "inference_state":inference_state,
        "transport_body_sha256":bodies.iter().map(|body| sha256_hex(body.to_string().as_bytes())).collect::<Vec<_>>(),
        "native_runtime_ids":"not exposed by this port"
    }));
}

pub(super) fn assert_counts(
    s: &SessionRuntime,
    peer: &Peer,
    calls: &Calls,
    transport: usize,
    rooms: usize,
) {
    let bodies = peer.bodies();
    let reports = calls.reports.lock().expect("captured room reports");
    let completed = reports
        .iter()
        .filter(|report| matches!(report.attempt, Attempt::Completed { .. }))
        .count();
    assert_eq!(
        bodies.len(),
        transport,
        "actual captured transport requests"
    );
    assert_eq!(
        s.inference_receipt()
            .expect("inference receipt readable")
            .expect("inference receipt present")
            .attempts
            .len(),
        transport
    );
    assert_eq!(calls.count(), rooms, "actual room invocations");
    assert_eq!(reports.len(), rooms, "each BEGIN returned");
    assert_eq!(
        completed, rooms,
        "the positive fixture requires completed attempts"
    );
    assert_eq!(
        s.rehearsals.turn.attempts,
        u32::try_from(rooms).expect("bounded expected room count")
    );
}

pub(super) fn snapshot(root: &Path, stage: &str) -> Value {
    let files: Vec<_> = [SOURCE, "out/copied.txt", "out/note.txt", "witness.txt"]
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let bytes = std::fs::read(root.join(path)).expect("fixture bytes");
            let metadata = std::fs::metadata(root.join(path)).expect("fixture metadata");
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode()
            };
            #[cfg(not(unix))]
            let mode = u32::from(metadata.permissions().readonly());
            json!({"index":index,"bytes":bytes.len(),"sha256":sha256_hex(&bytes),"mode":mode})
        })
        .collect();
    let snapshot = json!(files);
    room_call(
        &json!({"schema":"native-edit-snapshot/1","test":TEST,"stage":stage,"files":snapshot}),
    );
    snapshot
}

pub(super) fn unchanged(case: &Case, before: &Value, stage: &str) {
    assert_eq!(
        &snapshot(&case.project, stage),
        before,
        "original fixtures unchanged"
    );
    assert!(
        std::fs::read_dir(&case.rooms)
            .expect("owned rooms directory")
            .next()
            .is_none()
    );
}
