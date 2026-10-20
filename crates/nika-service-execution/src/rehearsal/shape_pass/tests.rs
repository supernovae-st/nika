// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The shape pass, through a real replay trial: the steps after a model step that declares its
//! answer's shape run on an example of it, and only a failure of that shape fails the trial.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use nika_fs::{EffectLedger, OwnedDir, Phase, RoomLimits, RootedFs};
use nika_runtime::{RunOutcome, TaskStatus};

use super::SHAPE;
use crate::ServiceExecutionDriver;
use crate::rehearsal::isolated_jq::{IsolatedJq, JqHelper};
use crate::rehearsal::replay::Captures;
use crate::rehearsal::room::DeniedTally;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>; // box-dyn-ok(test-harness): cfg(test) fixtures use heterogeneous setup failures

/// The skipped-path tests' helper, this same binary: one evaluation per start.
fn helper(dir: &Path) -> TestResult<IsolatedJq> {
    let script = dir.join("jq-helper");
    let exe = std::env::current_exe()?;
    let body = format!(
        "#!/bin/sh\nexec /usr/bin/env NIKA_SKIPPED_PATH_JQ=1 '{}' --exact rehearsal::skipped_path::tests::jq_helper --ignored --test-threads=1 -q 3>&1 1>/dev/null 2>/dev/null\n",
        exe.display()
    );
    std::fs::write(&script, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(IsolatedJq::new(
        JqHelper::new(script),
        Instant::now() + Duration::from_secs(60),
    ))
}

/// A digest: a model step that declares an array of stories, and the render after it, whose
/// filter is `filter`.
fn digest(filter: &str) -> String {
    format!(
        "nika: digest\nmodel: xai/grok-4.7\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  pick:\n    infer:\n      prompt: \"pick the stories\"\n      max_tokens: 400\n      schema:\n        type: array\n        items:\n          type: object\n          required: [title]\n          properties:\n            title: {{ type: string }}\n            url: {{ type: string }}\n  render:\n    with: {{ items: \"${{{{ tasks.pick.output }}}}\" }}\n    invoke:\n      tool: nika:jq\n      args:\n        input: {{ items: \"${{{{ with.items }}}}\", day: \"2026-10-17\" }}\n        expression: '{filter}'\n"
    )
}

/// The trial's outcome for `source`: admitted from its bytes, replayed in an empty room with the
/// isolated jq, the shape pass after it.
async fn tried(source: &str) -> TestResult<RunOutcome> {
    let project = tempfile::tempdir()?;
    let service = nika_execution::ExecutionService::default();
    let admitted = service
        .admit_root_bytes(
            &OwnedDir::open(project.path())?,
            Path::new("candidate.nika"),
            source.as_bytes(),
        )
        .map_err(|refusal| format!("HARNESS_INVALID: the door refused the fixture: {refusal}"))?;
    let session = service.begin(admitted);
    let driver = ServiceExecutionDriver::new(session.context(), project.path())
        .ok_or("HARNESS_INVALID: the admitted context lost its root")?;
    let plan = (driver.replay_plan(None)).map_err(|refused| refused.to_string())?;
    let dir = tempfile::tempdir()?;
    let ledger = EffectLedger::new(RoomLimits::new(1 << 20, 64));
    let fs = Arc::new(RootedFs::new(
        OwnedDir::open(dir.path())?,
        Arc::clone(&ledger),
    ));
    let _ = ledger.seal_and_drain().await;
    if ledger.advance() != Ok(Phase::Run) {
        return Err("the room did not reach its run phase".into());
    }
    let jq = Arc::new(helper(dir.path())?);
    let outcome = driver
        .rehearse_over_replaying(
            fs,
            plan,
            Arc::new(DeniedTally::default()),
            Some(jq),
            Some(Arc::new(Captures::new())),
            None,
        )
        .await?;
    Ok(outcome)
}

/// The live gauntlet r04 shape: the render indexes a date that is text. The model step did not
/// run, yet the render meets that error on an example of the stories: the trial fails there.
#[tokio::test]
async fn a_step_after_a_model_step_that_fails_on_its_shape_fails_the_trial() -> TestResult<()> {
    let outcome = tried(&digest(".day.date")).await?;
    assert!(!outcome.ok, "{outcome:?}");
    let render = &outcome.records["render"];
    assert_eq!(render.status, TaskStatus::Failure);
    let error = render.error.as_ref().ok_or("an error")?;
    assert_eq!(error.code, SHAPE);
    assert!(
        error
            .message
            .starts_with("render fails on an answer of the shape pick declares")
            && error.message.contains("cannot index"),
        "{}",
        error.message
    );
    Ok(())
}

/// A render that reads the stories as their shape says passes: nothing is claimed.
#[tokio::test]
async fn a_step_that_reads_the_shape_right_passes() -> TestResult<()> {
    let outcome = tried(&digest(
        "[.items[] | .title + \" \" + (.url // \"\")] | join(\"\\n\")",
    ))
    .await?;
    assert!(outcome.ok, "{:?}", outcome.records.get("render"));
    Ok(())
}

/// A failure of content (text the example holds read as a number) is the example's, never the
/// program's: nothing is claimed.
#[tokio::test]
async fn a_failure_of_the_examples_content_is_never_claimed() -> TestResult<()> {
    let outcome = tried(&digest(".items[0].title | tonumber")).await?;
    assert!(outcome.ok, "{:?}", outcome.records.get("render"));
    Ok(())
}
