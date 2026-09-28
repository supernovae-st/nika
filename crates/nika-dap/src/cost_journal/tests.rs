// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The journal fold: settled rows clear, an uncertain settlement blocks, a
//! leased row whose writer is gone becomes ONE durable unknown, a row no lease
//! covers is never judged, and a row cut mid-write is named by its digest.
//! Rows are appended, never rewritten. The lease: held refuses, released is
//! taken again. (Moved with the code from nika-cli-host's `run_cost`.)
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::*;
use serde_json::json;

const HOST: &str = "fixture-host";

fn observation(state: &str, unknown_calls: u64) -> serde_json::Value {
    json!({"attempts": [], "billed_nano_usd": null, "known_subtotal_nano_usd": "0",
        "limit_nano_usd": null, "overridden_defaults": [null, null], "refusal": null,
        "schema": "nika/inference-cost-observation@1", "state": state,
        "unknown_attempts": [], "unknown_calls": unknown_calls, "unknown_cost": null})
}

fn row(invocation: &str, phase: &str, state: &str, lease: Option<(u64, &str)>) -> String {
    let mut row = json!({"schema": "nika/run-cost-observation@1", "invocation": invocation,
        "phase": phase, "observation": observation(state, 0)});
    if let Some((pid, host)) = lease {
        row["lease"] = json!({"pid": pid, "host": host});
    }
    row.to_string()
}

/// The lease holder a review judges from: this host name, and a kernel boot
/// identity only where the fixture proves one.
fn holder(host: &str, boot: Option<&str>) -> Writer {
    Writer {
        pid: 1,
        host: host.into(),
        boot: boot.map(Into::into),
    }
}

fn project(lines: &[String]) -> (tempfile::TempDir, OwnedDir) {
    let root = tempfile::tempdir().unwrap();
    let nika = OwnedDir::open(root.path())
        .unwrap()
        .create_below(&[".nika"])
        .unwrap();
    for line in lines {
        nika.append_line(JOURNAL, line).unwrap();
    }
    (root, nika)
}

fn journal(root: &tempfile::TempDir) -> String {
    std::fs::read_to_string(root.path().join(".nika").join(JOURNAL)).unwrap_or_default()
}

/// The Runs a fold names, on a journal no writer cut mid-write.
fn runs(nika: &OwnedDir, host: &str, observer: &str) -> Vec<Blocker> {
    let exposures = fold(nika, host, observer).unwrap();
    assert!(exposures.torn.is_empty(), "{exposures:?}");
    exposures.runs
}

#[test]
fn an_absent_journal_blocks_nothing_and_writes_nothing() {
    let (root, nika) = project(&[]);
    assert!(
        fold_as(&nika, &holder(HOST, None), "observer")
            .unwrap()
            .is_clear()
    );
    assert!(!root.path().join(".nika").join(JOURNAL).exists());
}

#[test]
fn settled_rows_clear_and_an_uncertain_settlement_blocks_without_a_new_row() {
    let lines = [
        row("run-a", "prepared", "Open", Some((7, HOST))),
        row("run-a", "settled", "Closed", Some((7, HOST))),
        row("run-b", "prepared", "Open", Some((8, HOST))),
        row("run-b", "settled", "Uncertain", Some((8, HOST))),
    ];
    let (root, nika) = project(&lines);
    let before = journal(&root);
    assert_eq!(
        runs(&nika, HOST, "observer"),
        vec![Blocker::new("run-b".into(), Exposure::Uncertain)]
    );
    assert_eq!(journal(&root), before, "a settled Run is never re-derived");
}

/// B12 · the fold reads an observation by field: a choice widened to two
/// requests in flight with the authored-retry law, and an answered 429, settle
/// and block exactly as the historical rows above (nothing fails closed).
#[test]
fn a_widened_choice_and_an_answered_attempt_fold_as_before() {
    let widened = |invocation: &str, phase: &str, state: &str, pid: u64| {
        let mut row: serde_json::Value =
            serde_json::from_str(&row(invocation, phase, state, Some((pid, HOST)))).unwrap();
        row["observation"]["unknown_cost"] = json!({"candidate": "c", "invocation": invocation,
            "provider": "deepseek", "model": "m", "endpoint": "https://api.deepseek.com/v1/chat/completions",
            "max_requests": 6, "max_in_flight": 2, "authored_retry": true,
            "max_output_tokens": 8192, "timeout_ms": 120_000, "declared_tariff": null});
        if phase == "settled" {
            row["observation"]["unknown_calls"] = json!(1);
            row["observation"]["unknown_attempts"] = json!([{"id": 0, "sent": true,
                "estimated_nano_usd": null, "note": "answered HTTP 429; usage and USD cost unknown"}]);
        }
        row.to_string()
    };
    let lines = [
        widened("run-a", "prepared", "Open", 7),
        widened("run-a", "settled", "Closed", 7),
        widened("run-b", "prepared", "Open", 8),
        widened("run-b", "settled", "Uncertain", 8),
    ];
    let (_root, nika) = project(&lines);
    assert_eq!(
        runs(&nika, HOST, "observer"),
        vec![Blocker::new("run-b".into(), Exposure::Uncertain)]
    );
}

/// P3 · the restart derives the killed Run's UNKNOWN once, from the exact
/// `prepared` row its leased writer left, and appends it; a second fold
/// reads the recorded UNKNOWN and appends nothing.
#[test]
fn a_leased_prepared_row_becomes_one_durable_unknown_and_stays_unknown() {
    let prepared = row("run-killed", "prepared", "Open", Some((4242, HOST)));
    let (root, nika) = project(std::slice::from_ref(&prepared));
    let before = journal(&root);
    let unknown = Blocker {
        invocation: "run-killed".into(),
        exposure: Exposure::Unknown { pid: Some(4242) },
        trace: None,
    };
    assert_eq!(runs(&nika, HOST, "run-next"), vec![unknown.clone()]);
    let after = journal(&root);
    assert!(
        after.starts_with(&before),
        "the rows read stay byte-identical"
    );
    let lines: Vec<&str> = after.lines().collect();
    assert_eq!(lines.len(), 2, "exactly one derived row");
    let derived: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(
        derived["invocation"], "run-killed",
        "the same Run's identity"
    );
    assert_eq!(derived["phase"], "unknown");
    let original: serde_json::Value = serde_json::from_str(&prepared).unwrap();
    assert_eq!(
        derived["prior_observation"], original["observation"],
        "the account's last words, verbatim"
    );
    assert!(
        derived.get("observation").is_none(),
        "C3 · N4 · the prepared-time Open and zero counters are labeled prior, \
         never read as the unknown Run's current state"
    );
    assert_eq!(derived["unsettled"]["cause"], "interrupted");
    assert_eq!(
        derived["unsettled"]["writer"],
        json!({"pid": 4242, "host": HOST})
    );
    assert_eq!(
        derived["unsettled"]["prior_sha256"],
        nika_event::source_id::sha256_hex(prepared.as_bytes())
    );
    assert_eq!(derived["unsettled"]["observed_by"], "run-next");
    assert_eq!(
        runs(&nika, HOST, "run-later"),
        vec![unknown],
        "the recorded unknown still blocks: no automatic retry"
    );
    assert_eq!(journal(&root), after, "an unknown is recorded once");
}

#[test]
fn rows_without_a_lease_or_from_another_host_are_never_judged() {
    let lines = [
        row("run-legacy", "prepared", "Open", None),
        row(
            "run-elsewhere",
            "prepared",
            "Open",
            Some((9, "another-host")),
        ),
    ];
    let (root, nika) = project(&lines);
    let before = journal(&root);
    let unjudged = |id: &str| Blocker {
        invocation: id.into(),
        exposure: Exposure::Unjudged,
        trace: None,
    };
    assert_eq!(
        runs(&nika, HOST, "observer"),
        vec![unjudged("run-elsewhere"), unjudged("run-legacy")]
    );
    // A host that cannot name itself judges nothing either.
    let (root_nameless, nika_nameless) =
        project(&[row("run-x", "prepared", "Open", Some((1, "")))]);
    assert_eq!(
        runs(&nika_nameless, "", "observer"),
        vec![unjudged("run-x")]
    );
    assert_eq!(journal(&root), before);
    assert_eq!(journal(&root_nameless).lines().count(), 1);
}

/// C3 · N4 · an unknown row an earlier engine derived (its prior observation
/// under `observation`) still reads as that Run's recorded unknown; a derived
/// row whose prior observation is unreadable fails closed.
#[test]
fn an_earlier_derived_unknown_reads_and_a_broken_prior_fails_closed() {
    let prepared = row("run-e", "prepared", "Open", Some((9, HOST)));
    let current = derived(&prepared, "run-review");
    let earlier = current.replace("\"prior_observation\"", "\"observation\"");
    assert_ne!(earlier, current);
    let (_root, nika) = project(&[prepared.clone(), earlier]);
    assert_eq!(
        runs(&nika, "another-host", "run-next"),
        vec![Blocker::new(
            "run-e".into(),
            Exposure::Unknown { pid: Some(9) }
        )]
    );
    let broken = current.replace("\"unknown_calls\":0", "\"unknown_calls\":\"0\"");
    assert_ne!(broken, current);
    let (_root, nika) = project(&[prepared, broken]);
    assert!(fold_as(&nika, &holder(HOST, None), "run-next").is_err());
}

#[test]
fn an_unrecognized_row_fails_closed() {
    let foreign_phase = row("run-a", "reconciled", "Closed", None);
    let bare_unknown = row("run-a", "unknown", "Open", None);
    let foreign_schema = r#"{"schema":"other@1","invocation":"run-a"}"#.to_owned();
    for text in [foreign_phase, bare_unknown, foreign_schema] {
        let (_root, nika) = project(std::slice::from_ref(&text));
        assert!(
            fold_as(&nika, &holder(HOST, None), "observer").is_err(),
            "{text}"
        );
    }
}

/// E · C2 · a writer killed between a row and its newline leaves a torn tail.
/// The next append first ends that line, so no row fuses with it; the fold
/// names the cut bytes by their digest and keeps blocking — never a parse
/// error that bricks every later review, never a silent skip.
#[test]
fn a_row_cut_mid_write_is_named_by_digest_and_never_fuses() {
    let prepared = row("run-cut", "prepared", "Open", Some((5, HOST)));
    let cut = &prepared[..prepared.len() / 2];
    let (root, nika) = project(&[]);
    std::fs::write(root.path().join(".nika").join(JOURNAL), cut).unwrap();
    let settled = row("run-next", "settled", "Closed", Some((6, HOST)));
    append_row(&nika, &settled).unwrap();
    let text = journal(&root);
    assert_eq!(
        text,
        format!("{cut}\n{settled}\n"),
        "the cut bytes stay, alone"
    );
    let exposures = fold_as(&nika, &holder(HOST, None), "observer").unwrap();
    assert_eq!(
        exposures.torn,
        vec![nika_event::source_id::sha256_hex(cut.as_bytes())]
    );
    assert!(exposures.runs.is_empty(), "run-next settled cleanly");
    assert!(!exposures.is_clear(), "the cut row still blocks");
    assert!(refusal(&exposures).contains("a row was cut mid-write (sha256 "));
    // A row whose newline alone was lost is whole: it is read, then ended.
    let (root_whole, nika_whole) = project(&[]);
    std::fs::write(root_whole.path().join(".nika").join(JOURNAL), &prepared).unwrap();
    assert_eq!(
        runs(&nika_whole, HOST, "observer"),
        vec![Blocker {
            invocation: "run-cut".into(),
            exposure: Exposure::Unknown { pid: Some(5) },
            trace: None,
        }]
    );
    let lines: Vec<String> = journal(&root_whole).lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], prepared);
}

/// The derived `unknown` line a later review appends for `prepared`.
fn derived(prepared: &str, observer: &str) -> String {
    let row: serde_json::Value = serde_json::from_str(prepared).unwrap();
    let id = row["invocation"].as_str().unwrap();
    unknown_row(id, &row, prepared.as_bytes(), observer).to_string()
}

/// The conflicts a fold names: (invocation, sha256 of the exact line, reason).
fn conflicts(exposures: &Exposures) -> Vec<(String, String, &'static str)> {
    exposures
        .conflicts
        .iter()
        .map(|c| (c.invocation.clone(), c.sha256.clone(), c.reason))
        .collect()
}

fn sha(line: &str) -> String {
    nika_event::source_id::sha256_hex(line.as_bytes())
}

/// C3 · N1 · E4's counterexample and its kin: a settled row appended by hand,
/// lease-less or with a copied or foreign writer, after the recorded unknown
/// or before it, never erases the exposure. Each is a named conflict that
/// blocks by itself, and the Run keeps the standing its legal rows give it.
#[test]
fn a_late_or_foreign_settlement_never_erases_an_exposure() {
    let prepared = row("run-k", "prepared", "Open", Some((41, HOST)));
    let unknown = derived(&prepared, "run-review");
    let unleased = row("run-k", "settled", "Closed", None);
    let copied = row("run-k", "settled", "Closed", Some((41, HOST)));
    let (root, nika) = project(&[
        prepared.clone(),
        unknown.clone(),
        unleased.clone(),
        copied.clone(),
    ]);
    let before = journal(&root);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    let killed = Blocker::new("run-k".into(), Exposure::Unknown { pid: Some(41) });
    assert_eq!(exposures.runs, vec![killed]);
    assert_eq!(
        conflicts(&exposures),
        vec![
            (
                "run-k".into(),
                sha(&unleased),
                "carries no cost lease after leased rows began"
            ),
            (
                "run-k".into(),
                sha(&copied),
                "follows the Run's recorded unknown"
            ),
        ]
    );
    assert!(!exposures.is_clear());
    assert_eq!(
        journal(&root),
        before,
        "nothing appended: the unknown was recorded"
    );
    let text = refusal(&exposures);
    assert!(
        text.contains(&format!("(sha256 {})", sha(&copied))),
        "{text}"
    );
    assert!(text.contains("refused as a conflict, the exposure before it stands"));
    // A foreign writer settles before any review derived the unknown: the
    // prepared row still stands, so this review derives it (one row appended).
    let prepared = row("run-f", "prepared", "Open", Some((42, HOST)));
    let foreign = row("run-f", "settled", "Closed", Some((1, HOST)));
    let (root, nika) = project(&[prepared.clone(), foreign.clone()]);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    assert_eq!(
        exposures.runs,
        vec![Blocker::new(
            "run-f".into(),
            Exposure::Unknown { pid: Some(42) }
        )]
    );
    assert_eq!(
        conflicts(&exposures),
        vec![(
            "run-f".into(),
            sha(&foreign),
            "settles a Run from a writer that did not prepare it"
        )]
    );
    let lines: Vec<String> = journal(&root).lines().map(str::to_owned).collect();
    assert_eq!(
        lines,
        vec![prepared.clone(), foreign, derived(&prepared, "run-next")]
    );
}

/// C3 · N1 · the other illegal rows: a Run with no prepared row, a second
/// preparation, an unknown that does not derive from the prepared row's exact
/// bytes, and a settlement changed after the fact. A byte-identical repeat of
/// a settled row (a retried append) stays benign.
#[test]
fn only_legal_transitions_move_a_run() {
    let orphan = row("run-o", "settled", "Closed", Some((5, HOST)));
    let prepared = row("run-p", "prepared", "Open", Some((6, HOST)));
    let again = row("run-p", "prepared", "Open", Some((6, HOST))).replace("Open", "Closed");
    let forged_unknown = derived(&prepared, "run-x").replace(&sha(&prepared), &"0".repeat(64));
    let settled = row("run-s", "settled", "Closed", Some((7, HOST)));
    let changed = row("run-s", "settled", "Uncertain", Some((7, HOST)));
    let (_root, nika) = project(&[
        orphan.clone(),
        prepared.clone(),
        again.clone(),
        forged_unknown.clone(),
        row("run-s", "prepared", "Open", Some((7, HOST))),
        settled.clone(),
        settled.clone(),
        changed.clone(),
    ]);
    let exposures = fold_as(&nika, &holder("another-host", None), "run-next").unwrap();
    assert_eq!(
        exposures.runs,
        vec![Blocker::new("run-p".into(), Exposure::Unjudged)],
        "run-s settled cleanly; its exact repeat is benign"
    );
    assert_eq!(
        conflicts(&exposures),
        vec![
            (
                "run-o".into(),
                sha(&orphan),
                "names a Run with no prepared row before it"
            ),
            (
                "run-p".into(),
                sha(&again),
                "prepares the Run a second time"
            ),
            (
                "run-p".into(),
                sha(&forged_unknown),
                "records unknown from a row other than the Run's prepared one"
            ),
            (
                "run-s".into(),
                sha(&changed),
                "follows the Run's settlement"
            ),
        ]
    );
}

/// C3 · N1 · control: a lease-less pair written before the lease existed still
/// settles, and so does a leased pair; a lease-less row after them does not.
#[test]
fn legacy_pairs_settle_only_before_leased_rows_began() {
    let late = row("run-late", "prepared", "Open", None);
    let (_root, nika) = project(&[
        row("run-legacy", "prepared", "Open", None),
        row("run-legacy", "settled", "Closed", None),
        row("run-leased", "prepared", "Open", Some((8, HOST))),
        row("run-leased", "settled", "Closed", Some((8, HOST))),
        late.clone(),
    ]);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    assert!(exposures.runs.is_empty(), "{exposures:?}");
    assert_eq!(
        conflicts(&exposures),
        vec![(
            "run-late".into(),
            sha(&late),
            "carries no cost lease after leased rows began"
        )]
    );
}

/// C3 · N7 · a Run is named by its execution id and, when a trace recorded
/// that execution, by the trace file `nika trace show` takes: a store name of
/// that trace id whose first frame names the same execution, never a decoy
/// with the same short suffix. Wording speaks of the lease only: the writer
/// "no longer holds" it, never "is gone".
#[test]
fn a_blocking_run_names_the_trace_that_recorded_it() {
    let id = "01a0e5a3-1a92-7779-8387-a719e96ab793";
    let invocation = format!("exe-{id}");
    let prepared = row(&invocation, "prepared", "Open", Some((77, HOST)));
    let (root, nika) = project(std::slice::from_ref(&prepared));
    let traces = root.path().join(".nika").join("traces");
    std::fs::create_dir_all(&traces).unwrap();
    let frame = |uuid: &str| {
        format!(
            "{}\n",
            json!({"kind": "workflow_started", "execution": {"uuid": uuid}})
        )
    };
    let decoy = "2026-09-28T01-00-00Z-b793.ndjson";
    std::fs::write(
        traces.join(decoy),
        frame("01a0e5a3-0000-7000-8000-00000000b793"),
    )
    .unwrap();
    let recorded = "2026-09-28T01-00-01Z-b793.ndjson";
    std::fs::write(traces.join(recorded), frame(id)).unwrap();
    let exposures = fold_as(&nika, &holder(HOST, None), "exe-next").unwrap();
    let [blocker] = exposures.runs.as_slice() else {
        panic!("{exposures:?}");
    };
    assert_eq!(blocker.trace.as_deref(), Some(recorded));
    let text = refusal(&exposures);
    assert!(
        text.contains(&format!(
            "Run {invocation} (trace .nika/traces/{recorded}) ended without a settlement; its writer, process 77, no longer holds the cost lease: billing unknown"
        )),
        "{text}"
    );
    assert!(!text.contains("is gone"), "{text}");
    // An invocation that is not an execution id names no trace.
    let (_root, nika) = project(&[row("run-plain", "prepared", "Open", None)]);
    let exposures = fold_as(&nika, &holder(HOST, None), "exe-next").unwrap();
    assert_eq!(exposures.runs[0].trace, None);
}

const BOOT: &str = "6f1d2c1e-8a47-4f0e-9b1a-2e5b7c9d0a13";
const OTHER_BOOT: &str = "0b9c8d7e-6f5a-4b3c-8d2e-1f0a9b8c7d6e";

/// A `prepared` row whose lease names `host` and, when proven, `boot`.
fn prepared_on(invocation: &str, pid: u64, host: &str, boot: Option<&str>) -> String {
    let mut row: serde_json::Value =
        serde_json::from_str(&row(invocation, "prepared", "Open", Some((pid, host)))).unwrap();
    if let Some(boot) = boot {
        row["lease"]["boot"] = json!(boot);
    }
    row.to_string()
}

/// C3 · N2 · a container restarted on the same kernel comes back under a new
/// hostname. The lease it acquired proves the killed writer's lock is gone,
/// and the boot identity proves it is the same kernel's lock: the Run is
/// derived unknown, once, instead of staying unjudged forever.
#[test]
fn a_restarted_container_on_the_same_kernel_derives_the_unknown() {
    let prepared = prepared_on("run-c", 12, "container-a", Some(BOOT));
    let (root, nika) = project(std::slice::from_ref(&prepared));
    let exposures = fold_as(&nika, &holder("container-b", Some(BOOT)), "run-next").unwrap();
    assert_eq!(
        exposures.runs,
        vec![Blocker::new(
            "run-c".into(),
            Exposure::Unknown { pid: Some(12) }
        )]
    );
    let lines: Vec<String> = journal(&root).lines().map(str::to_owned).collect();
    assert_eq!(
        lines,
        vec![prepared.clone(), derived(&prepared, "run-next")]
    );
}

/// C3 · N2 · negative controls: a journal copied from another machine (another
/// host, another kernel) is never judged, and neither is a row whose boot
/// identity is absent on either side: absent never matches absent, and an
/// empty hostname never matches an empty one. Nothing is appended.
#[test]
fn another_kernel_or_an_absent_identity_is_never_judged() {
    let cases = [
        (
            prepared_on("run-x", 3, "far-host", Some(OTHER_BOOT)),
            holder(HOST, Some(BOOT)),
        ),
        (
            prepared_on("run-x", 3, "far-host", None),
            holder(HOST, Some(BOOT)),
        ),
        (
            prepared_on("run-x", 3, "far-host", Some(BOOT)),
            holder(HOST, None),
        ),
        (
            prepared_on("run-x", 3, "far-host", None),
            holder(HOST, None),
        ),
        (prepared_on("run-x", 3, "", None), holder("", None)),
    ];
    for (prepared, holder) in cases {
        let (root, nika) = project(std::slice::from_ref(&prepared));
        let exposures = fold_as(&nika, &holder, "run-next").unwrap();
        assert_eq!(
            exposures.runs,
            vec![Blocker::new("run-x".into(), Exposure::Unjudged)],
            "{prepared} judged by {holder:?}"
        );
        assert_eq!(journal(&root), format!("{prepared}\n"), "nothing appended");
    }
}

/// The source-compatible `fold` names its holder by hostname only, so a row
/// matched by kernel alone stays unjudged there; `fold_as` with the proven
/// boot identity derives it.
#[test]
fn the_compatible_fold_judges_by_hostname_only() {
    let prepared = prepared_on("run-b", 21, "container-a", Some(BOOT));
    let (root, nika) = project(std::slice::from_ref(&prepared));
    assert_eq!(
        runs(&nika, "container-b", "run-next"),
        vec![Blocker::new("run-b".into(), Exposure::Unjudged)]
    );
    assert_eq!(journal(&root), format!("{prepared}\n"));
    let exposures = fold_as(&nika, &holder("container-b", Some(BOOT)), "run-next").unwrap();
    assert_eq!(
        exposures.runs,
        vec![Blocker::new(
            "run-b".into(),
            Exposure::Unknown { pid: Some(21) }
        )]
    );
}

/// The lease record names the kernel only where the platform proves one.
#[test]
fn the_lease_record_carries_a_proven_boot_identity_only() {
    assert_eq!(
        holder(HOST, Some(BOOT)).json(),
        json!({"pid": 1, "host": HOST, "boot": BOOT})
    );
    assert_eq!(holder(HOST, None).json(), json!({"pid": 1, "host": HOST}));
}

/// C3 · N5 · a writer cut inside a multi-byte character leaves bytes that are
/// not UTF-8: one torn row, named by the digest of its exact bytes. The rows
/// before it still read (never a raw decoding error that bricks every later
/// review), and the next append ends the torn line without touching its bytes.
#[test]
fn a_row_cut_inside_a_multibyte_character_is_torn_not_an_unreadable_journal() {
    let whole =
        r#"{"schema":"nika/run-cost-observation@1","invocation":"run-é","phase":"prepared"}"#;
    let cut = &whole.as_bytes()[..=whole.find('é').unwrap()];
    assert!(
        std::str::from_utf8(cut).is_err(),
        "the fixture cuts inside é"
    );
    let (root, nika) = project(&[
        row("run-a", "prepared", "Open", Some((7, HOST))),
        row("run-a", "settled", "Closed", Some((7, HOST))),
    ]);
    let path = root.path().join(".nika").join(JOURNAL);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.extend_from_slice(cut);
    std::fs::write(&path, &bytes).unwrap();
    let exposures = fold_as(&nika, &holder(HOST, None), "observer").unwrap();
    assert_eq!(exposures.torn, vec![nika_event::source_id::sha256_hex(cut)]);
    assert!(exposures.runs.is_empty(), "run-a settled: {exposures:?}");
    assert!(refusal(&exposures).contains("a row was cut mid-write (sha256 "));
    let next = row("run-b", "prepared", "Open", Some((8, HOST)));
    append_row(&nika, &next).unwrap();
    let mut expected = bytes;
    expected.extend_from_slice(format!("\n{next}\n").as_bytes());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        expected,
        "the cut bytes stay, alone"
    );
}

#[test]
fn the_refusal_names_each_run_and_its_evidence() {
    let text = refusal(&Exposures {
        runs: vec![
            Blocker {
                invocation: "run-a".into(),
                exposure: Exposure::Unknown { pid: Some(12) },
                trace: None,
            },
            Blocker {
                invocation: "run-b".into(),
                exposure: Exposure::Uncertain,
                trace: None,
            },
            Blocker {
                invocation: "run-c".into(),
                exposure: Exposure::Unjudged,
                trace: None,
            },
        ],
        torn: Vec::new(),
        conflicts: vec![Conflict::new(
            "run-d\u{1b}]52;;x\u{7}".into(),
            "ab12".into(),
            "follows the Run's settlement",
        )],
    });
    assert!(text.contains(
        "Run run-a ended without a settlement; its writer, process 12, no longer holds the cost lease"
    ));
    assert!(
        text.contains("a row for Run run-d]52;;x (sha256 ab12) follows the Run's settlement: refused as a conflict"),
        "journal text is escaped: {text:?}"
    );
    assert!(text.contains("Run run-b settled with a sent request whose charge is unknown"));
    assert!(text.contains("Run run-c was admitted and never settled"));
    assert!(text.contains("no automatic retry"));
    assert!(text.contains(".nika/inference-cost-observations.ndjson"));
}

/// One unknown-cost attempt as the account serializes it (nika-providers
/// `UnknownAttemptReceipt`): `sent` and `estimated_nano_usd` are what its
/// counters and subtotal are made of.
fn attempt(sent: bool, estimated: Option<&str>) -> serde_json::Value {
    json!({"id": 0, "choice": null, "pricing": {"kind": "unknown"}, "sent": sent,
        "usage": null, "estimated_nano_usd": estimated, "native_estimated_nano": estimated,
        "currency": null, "response_model": null, "request_id": null, "note": "fixture"})
}

/// One priced attempt as the account serializes it (`AttemptReceipt`).
fn priced(sent: bool, estimated: Option<&str>) -> serde_json::Value {
    json!({"id": 0, "model": "m", "endpoint": "https://api.example.test/v1", "sent": sent,
        "estimated_nano_usd": estimated, "reserved_nano_usd": "9000", "usage": null,
        "billing_provider": "p", "currency": "USD", "source": null, "as_of": null,
        "source_sha256": null, "note": "fixture"})
}

/// A leased `phase` row for `invocation` whose observation is the fresh
/// account's with `changes` written over it.
fn observed(invocation: &str, phase: &str, pid: u64, changes: &serde_json::Value) -> String {
    let state = if phase == "prepared" {
        "Open"
    } else {
        "Closed"
    };
    let mut row: serde_json::Value =
        serde_json::from_str(&row(invocation, phase, state, Some((pid, HOST)))).unwrap();
    for (key, value) in changes.as_object().unwrap() {
        row["observation"][key] = value.clone();
    }
    row.to_string()
}

/// A prepared Run and a settlement no legal transition can let through: the
/// settlement is a conflict, the prepared row stands, and this review derives
/// that Run's unknown once, appended after the rows it read.
fn assert_refused_settlement(changes: &serde_json::Value, reason: &str) {
    let prepared = observed("run-t", "prepared", 31, &json!({}));
    let settlement = observed("run-t", "settled", 31, changes);
    let (root, nika) = project(&[prepared.clone(), settlement.clone()]);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    assert_eq!(
        exposures.runs,
        vec![Blocker::new(
            "run-t".into(),
            Exposure::Unknown { pid: Some(31) }
        )],
        "{settlement}"
    );
    assert_eq!(
        conflicts(&exposures),
        vec![("run-t".into(), sha(&settlement), reason)],
        "{settlement}"
    );
    assert!(!exposures.is_clear());
    let lines: Vec<String> = journal(&root).lines().map(str::to_owned).collect();
    assert_eq!(
        lines,
        vec![prepared.clone(), settlement, derived(&prepared, "run-next")],
        "the rows read stay byte-identical; one derived row is appended"
    );
    assert!(refusal(&exposures).contains(reason));
}

/// B5 · root's independent fault review: a settlement its own account could
/// never have written never clears the Run. The account closes before it
/// settles (`Run ended; fresh decision required`), counts as unknown exactly
/// the sent attempts it could not price, and only ever adds nonnegative
/// estimates to its known subtotal (nika-providers `InferenceReceipt::
/// observation`, unchanged since the journal's first writer).
#[test]
fn a_settlement_its_own_account_contradicts_never_clears() {
    for (changes, reason) in [
        (
            json!({"state": "Open"}),
            "settles the Run with its account still open",
        ),
        (
            json!({"unknown_calls": 1}),
            "counts unknown calls its sent attempts do not record",
        ),
        (
            json!({"known_subtotal_nano_usd": "-1"}),
            "reports a negative known subtotal",
        ),
    ] {
        assert_refused_settlement(&changes, reason);
    }
}

/// B5 · the nearby contradictions are conflicts too: counters that disagree
/// with the attempts either way, a subtotal no estimate backs, an attempt no
/// account writes (a negative or non-decimal estimate, an estimate on a request
/// never sent, a non-boolean `sent`), missing attempt lists, and an uncertain
/// settlement whose counters contradict it (it blocked before; now it is named).
#[test]
fn nearby_contradictions_are_named_conflicts_never_clears() {
    let unknown_call = attempt(true, None);
    for (changes, reason) in [
        (
            json!({"unknown_attempts": [unknown_call]}),
            "counts unknown calls its sent attempts do not record",
        ),
        (
            json!({"unknown_calls": 2, "unknown_attempts": [unknown_call]}),
            "counts unknown calls its sent attempts do not record",
        ),
        (
            json!({"known_subtotal_nano_usd": "1500"}),
            "reports a known subtotal its attempts do not add up to",
        ),
        (
            json!({"known_subtotal_nano_usd": "1500", "attempts": [priced(true, Some("1000"))]}),
            "reports a known subtotal its attempts do not add up to",
        ),
        (
            json!({"known_subtotal_nano_usd": "-1500",
                "unknown_attempts": [attempt(true, Some("-1500"))]}),
            "records an attempt its account cannot write",
        ),
        (
            json!({"known_subtotal_nano_usd": "1500",
                "unknown_attempts": [attempt(false, Some("1500"))]}),
            "records an attempt its account cannot write",
        ),
        (
            json!({"known_subtotal_nano_usd": "1500",
                "attempts": [{"sent": true, "estimated_nano_usd": 1500}]}),
            "records an attempt its account cannot write",
        ),
        (
            json!({"unknown_calls": 1,
                "unknown_attempts": [{"sent": "yes", "estimated_nano_usd": null}]}),
            "records an attempt its account cannot write",
        ),
        (
            json!({"attempts": null}),
            "records its account without both attempt lists",
        ),
        (
            json!({"state": "Uncertain", "unknown_calls": 1}),
            "counts unknown calls its sent attempts do not record",
        ),
    ] {
        assert_refused_settlement(&changes, reason);
    }
}

/// B5 · controls: every settlement the account does write keeps its meaning.
/// Closed clears, including a completed unknown-cost call whose USD price stays
/// unknown (the TUI's own Run: `unknown_calls` 1 with that sent attempt), a
/// declared USD estimate, priced attempts that add up, and a request reserved
/// but never sent. Uncertain blocks with its sent request. A byte-identical
/// repeat stays benign, and nothing is appended.
#[test]
fn every_settlement_the_account_writes_keeps_its_meaning() {
    for (changes, exposure) in [
        (json!({}), None),
        (
            json!({"unknown_calls": 1, "unknown_attempts": [attempt(true, None)]}),
            None,
        ),
        (
            json!({"known_subtotal_nano_usd": "1500",
                "unknown_attempts": [attempt(true, Some("1500"))]}),
            None,
        ),
        (
            json!({"known_subtotal_nano_usd": "1500",
                "attempts": [priced(true, Some("1000")), priced(true, Some("500"))]}),
            None,
        ),
        (
            json!({"unknown_attempts": [attempt(false, None)], "attempts": [priced(false, None)]}),
            None,
        ),
        (
            json!({"state": "Uncertain", "unknown_calls": 2,
                "unknown_attempts": [attempt(true, None)], "attempts": [priced(true, None)]}),
            Some(Exposure::Uncertain),
        ),
    ] {
        let prepared = observed("run-s", "prepared", 32, &json!({}));
        let settlement = observed("run-s", "settled", 32, &changes);
        let (root, nika) = project(&[prepared, settlement.clone(), settlement.clone()]);
        let before = journal(&root);
        let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
        let blockers: Vec<Blocker> = exposure
            .into_iter()
            .map(|e| Blocker::new("run-s".into(), e))
            .collect();
        assert_eq!(exposures.runs, blockers, "{settlement}");
        assert!(exposures.conflicts.is_empty(), "{settlement}");
        assert_eq!(journal(&root), before, "nothing appended: {settlement}");
    }
}

/// B5 · a `prepared` row is the account before any request (the host writes
/// it right after the review confirms the choice): Open, with no attempt. One
/// whose account already moved is a conflict, so the settlement after it names
/// a Run with no prepared row; both block, and nothing is appended.
#[test]
fn a_prepared_row_whose_account_already_moved_is_a_conflict() {
    for changes in [
        json!({"state": "Closed"}),
        json!({"state": "Uncertain"}),
        json!({"unknown_calls": 1, "unknown_attempts": [attempt(true, None)]}),
        json!({"attempts": [priced(false, None)]}),
    ] {
        let prepared = observed("run-m", "prepared", 33, &changes);
        let settlement = observed("run-m", "settled", 33, &json!({}));
        let (root, nika) = project(&[prepared.clone(), settlement.clone()]);
        let before = journal(&root);
        let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
        assert!(exposures.runs.is_empty(), "{prepared}");
        assert_eq!(
            conflicts(&exposures),
            vec![
                (
                    "run-m".into(),
                    sha(&prepared),
                    "prepares the Run with an account that already moved"
                ),
                (
                    "run-m".into(),
                    sha(&settlement),
                    "names a Run with no prepared row before it"
                ),
            ],
            "{prepared}"
        );
        assert!(!exposures.is_clear());
        assert_eq!(journal(&root), before, "nothing appended: {prepared}");
    }
}

/// The route an unknown-cost choice names, as the account serializes it.
fn choice() -> serde_json::Value {
    json!({"provider": "deepseek", "model": "deepseek-chat",
        "endpoint": "https://127.0.0.1:18443/v1", "candidate": "c", "declared_tariff": null,
        "invocation": "i", "max_output_tokens": 16, "max_requests": 1, "timeout_ms": 4000})
}

/// A Run prepared by `pid` and settled Uncertain with one sent request the
/// provider named `req-1`: its prepared and settled lines.
fn uncertain_run(invocation: &str, pid: u64) -> (String, String) {
    let sent = json!({"sent": true, "estimated_nano_usd": null, "request_id": "req-1"});
    let prepared = observed(
        invocation,
        "prepared",
        pid,
        &json!({"unknown_cost": choice()}),
    );
    let settled = observed(
        invocation,
        "settled",
        pid,
        &json!({"unknown_cost": choice(), "state": "Uncertain", "unknown_calls": 1,
            "unknown_attempts": [sent]}),
    );
    (prepared, settled)
}

/// The `reconciled` row a lease holder writes for the Run whose latest line is
/// `head`: its own facts copied, the operator's attestation, a local principal.
fn resolution(head: &str, word: &str) -> serde_json::Value {
    let head_row: serde_json::Value = serde_json::from_str(head).unwrap();
    let recorded = facts(&head_row);
    json!({"schema": "nika/run-cost-observation@1", "invocation": head_row["invocation"],
        "phase": "reconciled", "reconciliation": {"schema": RECONCILIATION,
            "prior_sha256": sha(head), "resolution": word,
            "evidence": {"class": "operator_attestation", "verified": false,
                "reference": "invoice INV-7 line 2"},
            "route": recorded["route"], "provider_request_ids": recorded["provider_request_ids"],
            "window": recorded["window"],
            "principal": {"kind": "local_account", "uid": 501, "name": "operator"},
            "project": {"binding": "0".repeat(64), "basis": "host-local inspected-directory binding"},
            "observed_at": "2026-09-28T04:00:00Z"},
        "lease": {"pid": 77, "host": HOST}})
}

/// B6 · P4 · a final resolution of the Run's latest row ends its exposure and
/// keeps every row: `billed` and `not_billed` alike, on the settled Uncertain
/// row itself. The fold appends nothing.
#[test]
fn a_final_resolution_ends_an_uncertain_exposure_and_keeps_every_row() {
    for word in ["billed", "not_billed"] {
        let (prepared, settled) = uncertain_run("run-u", 51);
        let resolved = resolution(&settled, word);
        let claim = &resolved["reconciliation"];
        assert_eq!(
            claim["route"]["model"], "deepseek-chat",
            "copied, never typed"
        );
        assert_eq!(claim["provider_request_ids"], json!(["req-1"]));
        let (root, nika) = project(&[prepared, settled, resolved.to_string()]);
        let before = journal(&root);
        let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
        assert!(exposures.is_clear(), "{word}: {exposures:?}");
        assert_eq!(journal(&root), before, "{word}: nothing appended");
    }
}

/// B6 · `still_unknown` keeps the Run blocking and becomes its latest row, so
/// a later resolution must name it: one that names the older row is stale.
#[test]
fn still_unknown_keeps_blocking_and_becomes_the_runs_latest_row() {
    let (prepared, settled) = uncertain_run("run-h", 52);
    let held = resolution(&settled, "still_unknown").to_string();
    let rows = [prepared, settled.clone(), held.clone()];
    let (_root, nika) = project(&rows);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    let still = Blocker::new("run-h".into(), Exposure::StillUnknown);
    assert_eq!(exposures.runs, vec![still.clone()]);
    assert!(refusal(&exposures).contains("Run run-h was reconciled as still unknown"));
    let stale = resolution(&settled, "billed").to_string();
    let (_root, nika) = project(&[rows.to_vec(), vec![stale.clone()]].concat());
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    assert_eq!(exposures.runs, vec![still]);
    assert_eq!(
        conflicts(&exposures),
        vec![(
            "run-h".into(),
            sha(&stale),
            "reconciles a row other than the Run's latest"
        )]
    );
    let final_row = resolution(&held, "billed").to_string();
    let (_root, nika) = project(&[rows.to_vec(), vec![final_row]].concat());
    assert!(
        fold_as(&nika, &holder(HOST, None), "run-next")
            .unwrap()
            .is_clear()
    );
}

/// B6 · a recorded unknown is resolved through its own digest, in the shape
/// this engine derives and in the earlier `observation` shape.
#[test]
fn a_recorded_unknown_is_resolved_through_its_own_digest() {
    let invocation = "exe-01a0e5c5-de7d-7199-aa9e-4830e9cda9c8";
    let prepared = observed(
        invocation,
        "prepared",
        53,
        &json!({"unknown_cost": choice()}),
    );
    let current = derived(&prepared, "exe-01a0e5ea-d6cc-7621-8f55-01df6f1b026d");
    let earlier = current.replace("\"prior_observation\"", "\"observation\"");
    for unknown in [current, earlier] {
        let resolved = resolution(&unknown, "not_billed");
        assert_eq!(
            resolved["reconciliation"]["window"],
            json!({"basis": "uuidv7-execution-ids", "not_before": "2026-09-28T02:09:05.149Z",
                "not_after": "2026-09-28T02:49:28.012Z"}),
            "the ids' own times, labeled as such"
        );
        let (_root, nika) = project(&[prepared.clone(), unknown, resolved.to_string()]);
        let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
        assert!(exposures.is_clear(), "{exposures:?}");
    }
}

/// B6 · only an uncertain settlement, a recorded unknown or a still-unknown
/// resolution can be resolved. A clean settlement, a prepared (unjudged) Run
/// and an orphan are conflicts; after a final, only its exact repeat is benign.
#[test]
fn only_a_runs_uncertain_or_unknown_latest_row_can_be_resolved() {
    let prepared = observed("run-c", "prepared", 54, &json!({}));
    let clean = observed("run-c", "settled", 54, &json!({}));
    let on_clean = resolution(&clean, "billed").to_string();
    let far = prepared_on("run-f", 55, "far-host", None);
    let on_prepared = resolution(&far, "billed").to_string();
    let mut orphan = resolution(&clean, "billed");
    orphan["invocation"] = json!("run-o");
    let orphan = orphan.to_string();
    let (_root, nika) = project(&[
        prepared,
        clean,
        on_clean.clone(),
        far,
        on_prepared.clone(),
        orphan.clone(),
    ]);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    assert_eq!(
        exposures.runs,
        vec![Blocker::new("run-f".into(), Exposure::Unjudged)]
    );
    assert_eq!(
        conflicts(&exposures),
        vec![
            (
                "run-c".into(),
                sha(&on_clean),
                "reconciles a Run that settled without uncertainty"
            ),
            (
                "run-f".into(),
                sha(&on_prepared),
                "reconciles a Run that has not settled or been recorded unknown"
            ),
            (
                "run-o".into(),
                sha(&orphan),
                "names a Run with no prepared row before it"
            ),
        ]
    );
    let (prepared, settled) = uncertain_run("run-d", 56);
    let done = resolution(&settled, "billed").to_string();
    let again = resolution(&done, "not_billed").to_string();
    let (_root, nika) = project(&[
        prepared.clone(),
        settled.clone(),
        done.clone(),
        done.clone(),
    ]);
    assert!(
        fold_as(&nika, &holder(HOST, None), "run-next")
            .unwrap()
            .is_clear(),
        "an exact repeat"
    );
    let (_root, nika) = project(&[prepared, settled, done, again.clone()]);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    assert!(exposures.runs.is_empty());
    assert_eq!(
        conflicts(&exposures),
        vec![(
            "run-d".into(),
            sha(&again),
            "follows the Run's reconciliation"
        )]
    );
}

/// B6 · a resolution this door does not support never clears: the Run keeps
/// its exposure, and the row is a named conflict.
#[test]
fn a_resolution_this_door_does_not_support_never_clears() {
    let (prepared, settled) = uncertain_run("run-x", 57);
    let long = "r".repeat(513);
    let cases: [(&str, serde_json::Value, &str); 13] = [
        (
            "/reconciliation/evidence/class",
            json!("provider_invoice"),
            "carries evidence this engine does not support",
        ),
        (
            "/reconciliation/evidence/verified",
            json!(true),
            "carries evidence this engine does not support",
        ),
        (
            "/reconciliation/evidence/reference",
            json!(""),
            "carries evidence this engine does not support",
        ),
        (
            "/reconciliation/evidence/reference",
            json!("see \u{1b}[2J"),
            "carries evidence this engine does not support",
        ),
        (
            "/reconciliation/evidence/reference",
            json!(long),
            "carries evidence this engine does not support",
        ),
        (
            "/reconciliation/principal/kind",
            json!("claimed"),
            "names no local principal",
        ),
        (
            "/reconciliation/principal/uid",
            json!("501"),
            "names no local principal",
        ),
        (
            "/reconciliation/principal/uid",
            json!(1_u64 << 40),
            "names no local principal",
        ),
        (
            "/reconciliation/observed_at",
            json!("yesterday"),
            "records no readable observation time",
        ),
        (
            "/reconciliation/route/model",
            json!("deepseek-reasoner"),
            "names a route or request its Run does not record",
        ),
        (
            "/reconciliation/provider_request_ids",
            json!([]),
            "names a route or request its Run does not record",
        ),
        (
            "/reconciliation/window/not_before",
            json!("2026-01-01T00:00:00Z"),
            "names a route or request its Run does not record",
        ),
        (
            "/reconciliation/resolution",
            json!("refunded"),
            "names a resolution this engine does not know",
        ),
    ];
    for (pointer, value, reason) in cases {
        let mut forged = resolution(&settled, "billed");
        *forged.pointer_mut(pointer).unwrap() = value;
        let forged = forged.to_string();
        let (_root, nika) = project(&[prepared.clone(), settled.clone(), forged.clone()]);
        let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
        assert_eq!(
            exposures.runs,
            vec![Blocker::new("run-x".into(), Exposure::Uncertain)],
            "{pointer}"
        );
        assert_eq!(
            conflicts(&exposures),
            vec![("run-x".into(), sha(&forged), reason)],
            "{pointer}"
        );
    }
}

/// B6 · a resolution without the lease its writer held never clears: after
/// leased rows it carries no lease at all, and in a legacy (lease-less)
/// journal it still must name the lease it held.
#[test]
fn a_resolution_without_its_writers_lease_never_clears() {
    let (prepared, settled) = uncertain_run("run-x", 57);
    let mut unleased = resolution(&settled, "billed");
    unleased.as_object_mut().unwrap().remove("lease");
    let unleased = unleased.to_string();
    let (_root, nika) = project(&[prepared, settled, unleased.clone()]);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    assert_eq!(
        conflicts(&exposures),
        vec![(
            "run-x".into(),
            sha(&unleased),
            "carries no cost lease after leased rows began"
        )]
    );
    let legacy_prepared = row("run-l", "prepared", "Open", None);
    let mut legacy_settled: serde_json::Value =
        serde_json::from_str(&uncertain_run("run-l", 58).1).unwrap();
    legacy_settled.as_object_mut().unwrap().remove("lease");
    let legacy_settled = legacy_settled.to_string();
    let mut legacy = resolution(&legacy_settled, "billed");
    legacy.as_object_mut().unwrap().remove("lease");
    let legacy = legacy.to_string();
    let (_root, nika) = project(&[legacy_prepared, legacy_settled, legacy.clone()]);
    let exposures = fold_as(&nika, &holder(HOST, None), "run-next").unwrap();
    assert_eq!(
        exposures.runs,
        vec![Blocker::new("run-l".into(), Exposure::Uncertain)]
    );
    assert_eq!(
        conflicts(&exposures),
        vec![(
            "run-l".into(),
            sha(&legacy),
            "reconciles without holding the cost lease"
        )]
    );
}

/// B6 · a reconciliation under a contract this engine does not know, or one
/// that names no prior row, is unreadable: prior exposure is unknown (never a
/// clear journal), exactly as an engine before this phase reads every one.
#[test]
fn an_unreadable_reconciliation_fails_closed() {
    let (prepared, settled) = uncertain_run("run-r", 59);
    for (pointer, value) in [
        (
            "/reconciliation/schema",
            json!("nika/cost-reconciliation@2"),
        ),
        ("/reconciliation/prior_sha256", serde_json::Value::Null),
    ] {
        let mut unreadable = resolution(&settled, "billed");
        *unreadable.pointer_mut(pointer).unwrap() = value;
        let (_root, nika) = project(&[prepared.clone(), settled.clone(), unreadable.to_string()]);
        let error = fold_as(&nika, &holder(HOST, None), "run-next").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData, "{pointer}");
    }
}

/// B6 · the facts a resolution copies: the route of the unknown-cost choice,
/// every provider request id, and a window labeled as the `UUIDv7` time of the
/// ids themselves (null for any id that is not a `UUIDv7` execution id).
#[test]
fn the_window_is_the_time_of_the_ids_never_a_request_time() {
    let v4 = "exe-2b1f8a4e-6c3d-4f5a-9b8c-7d6e5f4a3b2c";
    for (invocation, expected) in [
        (
            "exe-01a0e5ea-d6cc-7621-8f55-01df6f1b026d",
            json!("2026-09-28T02:49:28.012Z"),
        ),
        ("run-x", serde_json::Value::Null),
        (v4, serde_json::Value::Null),
    ] {
        let head = observed(
            invocation,
            "settled",
            60,
            &json!({"unknown_cost": choice()}),
        );
        let recorded = facts(&serde_json::from_str(&head).unwrap());
        assert_eq!(recorded["window"]["basis"], "uuidv7-execution-ids");
        assert_eq!(recorded["window"]["not_before"], expected, "{invocation}");
        assert_eq!(recorded["window"]["not_after"], serde_json::Value::Null);
        assert_eq!(
            recorded["route"],
            json!({"provider": "deepseek", "model": "deepseek-chat",
            "endpoint": "https://127.0.0.1:18443/v1"})
        );
    }
}

/// A held lease refuses a second holder and names it; a dropped lease is taken
/// again (the kernel's release is the whole liveness proof).
#[test]
fn a_held_lease_is_busy_and_a_released_one_is_taken_again() {
    let (root, nika) = project(&[]);
    let writer = Writer::this_process();
    let Taken::Held(lease) = take(&nika, &writer).unwrap() else {
        panic!("a free lease is taken");
    };
    match take(&nika, &writer).unwrap() {
        Taken::Busy { pid } => assert_eq!(pid, Some(u64::from(writer.pid))),
        Taken::Held(_) => panic!("a held lease is never shared"),
    }
    drop(lease);
    assert!(matches!(take(&nika, &writer).unwrap(), Taken::Held(_)));
    let record = std::fs::read_to_string(root.path().join(".nika").join(LEASE)).unwrap();
    let record: serde_json::Value = serde_json::from_str(&record).unwrap();
    assert_eq!(record, writer.json());
}

#[test]
fn unlinking_a_live_leases_name_does_not_allow_a_second_writer() {
    let (root, nika) = project(&[]);
    let writer = Writer::this_process();
    let Taken::Held(lease) = take(&nika, &writer).unwrap() else {
        panic!("free lease");
    };
    std::fs::remove_file(root.path().join(".nika").join(LEASE)).unwrap();
    assert!(matches!(take(&nika, &writer).unwrap(), Taken::Busy { .. }));
    drop(lease);
    assert!(matches!(take(&nika, &writer).unwrap(), Taken::Held(_)));
}

#[test]
fn a_journal_shared_by_hard_link_is_refused_without_changing_its_bytes() {
    let (root, nika) = project(&[row("run", "prepared", "Open", Some((7, HOST)))]);
    let (other, other_nika) = project(&[]);
    let before = journal(&root);
    std::fs::hard_link(
        root.path().join(".nika").join(JOURNAL),
        other.path().join(".nika").join(JOURNAL),
    )
    .unwrap();
    assert!(take(&nika, &Writer::this_process()).is_err());
    assert!(take(&other_nika, &Writer::this_process()).is_err());
    assert_eq!(journal(&root), before);
    assert_eq!(journal(&other), before);
}

#[test]
fn replacing_journal_and_lock_names_does_not_bypass_a_live_directory_lease() {
    let (root, nika) = project(&[row("run", "prepared", "Open", Some((7, HOST)))]);
    let writer = Writer::this_process();
    let Taken::Held(lease) = take(&nika, &writer).unwrap() else {
        panic!("free lease");
    };
    let dir = root.path().join(".nika");
    std::fs::rename(dir.join(JOURNAL), dir.join("set-aside.ndjson")).unwrap();
    std::fs::copy(dir.join("set-aside.ndjson"), dir.join(JOURNAL)).unwrap();
    std::fs::remove_file(dir.join(LEASE)).unwrap();
    assert!(matches!(take(&nika, &writer).unwrap(), Taken::Busy { .. }));
    drop(lease);
}

#[test]
fn a_linked_legacy_lock_still_excludes_the_new_lease() {
    let (_root, nika) = project(&[]);
    let legacy = Flock::lock(
        nika.open_lock(LEASE).unwrap(),
        FlockArg::LockExclusiveNonblock,
    )
    .unwrap();
    let writer = Writer::this_process();
    assert!(matches!(take(&nika, &writer).unwrap(), Taken::Busy { .. }));
    drop(legacy);
    assert!(matches!(take(&nika, &writer).unwrap(), Taken::Held(_)));
}

#[test]
fn replacing_nika_inside_a_stable_project_does_not_admit_a_second_writer() {
    let (root, nika) = project(&[row("run", "prepared", "Open", Some((7, HOST)))]);
    let writer = Writer::this_process();
    let Taken::Held(lease) = take(&nika, &writer).unwrap() else {
        panic!("free lease");
    };
    let before = journal(&root);
    std::fs::rename(
        root.path().join(".nika"),
        root.path().join(".nika-set-aside"),
    )
    .unwrap();
    let replacement = OwnedDir::create(root.path(), &[".nika"]).unwrap();
    std::fs::write(root.path().join(".nika").join(JOURNAL), &before).unwrap();
    assert!(matches!(
        take(&replacement, &writer).unwrap(),
        Taken::Busy { .. }
    ));
    assert_eq!(journal(&root), before);
    assert!(!root.path().join(".nika").join(LEASE).exists());
    drop(lease);
    assert!(matches!(
        take(&replacement, &writer).unwrap(),
        Taken::Held(_)
    ));
}

#[test]
fn project_leases_do_not_serialize_sibling_projects() {
    let parent = tempfile::tempdir().unwrap();
    let a = OwnedDir::create(parent.path(), &["a", ".nika"]).unwrap();
    let b = OwnedDir::create(parent.path(), &["b", ".nika"]).unwrap();
    let writer = Writer::this_process();
    let Taken::Held(_a) = take(&a, &writer).unwrap() else {
        panic!("first project lease");
    };
    assert!(matches!(take(&b, &writer).unwrap(), Taken::Held(_)));
}

#[test]
fn moving_an_opened_nika_does_not_change_the_project_being_locked() {
    let (root, original) = project(&[]);
    let project = OwnedDir::open(root.path()).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let writer = Writer::this_process();
    let Taken::Held(lease) = take(&original, &writer).unwrap() else {
        panic!("original project lease");
    };
    std::fs::rename(
        root.path().join(".nika"),
        root.path().join(".nika-original"),
    )
    .unwrap();
    let opened = OwnedDir::create(root.path(), &[".nika"]).unwrap();
    std::fs::rename(root.path().join(".nika"), elsewhere.path().join(".nika")).unwrap();
    assert!(matches!(
        take_at(&project, &opened, &writer).unwrap(),
        Taken::Busy { .. }
    ));
    assert!(!elsewhere.path().join(".nika").join(JOURNAL).exists());
    drop(lease);
    assert!(take_at(&project, &opened, &writer).is_err());
    assert!(!elsewhere.path().join(".nika").join(JOURNAL).exists());
}

#[test]
fn an_explicit_project_refuses_a_sibling_directory_without_writes() {
    let (root, own) = project(&[]);
    let (_other_root, other) = project(&[]);
    let project = OwnedDir::open(root.path()).unwrap();
    let writer = Writer::this_process();
    assert!(take_at(&project, &other, &writer).is_err());
    assert!(other.read_optional(LEASE).unwrap().is_none());
    let Taken::Held(held) = take_at(&project, &own, &writer).unwrap() else {
        panic!("original project can still acquire its lease");
    };
    assert!(matches!(
        take_at(&project, &own, &writer).unwrap(),
        Taken::Busy { .. }
    ));
    drop(held);
    assert!(matches!(
        take_at(&project, &own, &writer).unwrap(),
        Taken::Held(_)
    ));
}
