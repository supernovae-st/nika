use std::fs;
use std::path::Path;
use std::sync::{Arc, Barrier};

use nika_fs::OwnedDir;

use crate::{
    ExecutionError, ExecutionService, ExecutionSnapshot, SnapshotLimits, SnapshotUnitKind,
};

const CHILD: &str = "nika: child\ninputs:\n  url: { type: string, required: true }\npermits:\n  exec: [\"echo\"]\ntasks:\n  fetch:\n    exec: { command: [\"echo\", \"${{ inputs.url }}\"] }\noutputs:\n  report: { value: \"${{ tasks.fetch.output }}\", type: string }\n";

fn parent(target: &str) -> String {
    format!(
        "nika: parent\npermits:\n  exec: [\"echo\"]\ntasks:\n  audit:\n    invoke:\n      workflow: \"{target}\"\n      args: {{ url: \"https://example.com\" }}\n    returns: {{ object: {{ report: string }} }}\n"
    )
}

fn pure_root() -> &'static str {
    "nika: root\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n"
}

#[test]
#[allow(
    clippy::panic,
    reason = "sentinel proves a refused world cannot reach its effect driver"
)]
fn snapshot_admission_refuses_static_model_findings_before_execution() {
    for (model, tokens) in [
        ("openai/gpt-5.2", 32),
        ("openai/gpt-5.2", 200_000),
        ("not-a-provider/model", 512),
    ] {
        let root = format!(
            "nika: root\nmodel: {model}\ntasks:\n  say:\n    infer: {{ prompt: hi, max_tokens: {tokens} }}\n"
        );
        let (_tmp, owned) = project(&[("root.nika.yaml", &root)]);
        let service = ExecutionService::default();
        let admitted = service.admit(&owned, Path::new("root.nika.yaml"));
        assert!(
            matches!(&admitted, Err(ExecutionError::CheckFailed { findings })
            if !findings.is_empty()),
            "{model}/{tokens}: {admitted:?}"
        );
        if let Ok(admitted) = admitted {
            service.execute(admitted, |_| {
                panic!("model refusal reached the effect driver")
            });
        }
    }
}

#[test]
fn effective_override_admits_the_owned_bytes_without_rewriting_the_snapshot() {
    let root = "nika: root\nmodel: openai/gpt-5.2\ntasks:\n  say:\n    infer: { prompt: hi, max_tokens: 32 }\n";
    let (_tmp, owned) = project(&[("root.nika.yaml", root)]);
    let service = ExecutionService::default();
    let admitted = service
        .admit_with_model_override(&owned, Path::new("root.nika.yaml"), Some("mock/echo"))
        .expect("effective mock has no reasoning floor");
    assert_eq!(admitted.snapshot().text("root.nika.yaml"), Some(root));
    let digest = admitted.snapshot().digest().to_owned();
    let snapshot = admitted.snapshot().clone();
    assert!(service.readmit_snapshot(snapshot.clone()).is_err());
    let readmitted = service
        .readmit_snapshot_with_model_override(snapshot, Some("mock/echo"))
        .expect("readmission uses this leg's effective override");
    assert_eq!(readmitted.snapshot().digest(), digest);
    let captured = service
        .admit_root_bytes_with_model_override(
            &owned,
            Path::new("root.nika.yaml"),
            root.as_bytes(),
            Some("mock/echo"),
        )
        .expect("already captured root bytes use the same gate");
    assert_eq!(captured.snapshot().digest(), digest);
}

#[test]
fn a_root_override_cannot_waive_a_captured_childs_own_capacity_gate() {
    let root = "nika: root\nmodel: openai/gpt-5.2\ntasks:\n  say:\n    infer: { prompt: hi, max_tokens: 32 }\n  call:\n    invoke: { workflow: ./child.nika.yaml }\n";
    let child = "nika: child\nmodel: openai/gpt-5.2\ntasks:\n  say:\n    infer: { prompt: hi, max_tokens: 200000 }\n";
    let (_tmp, owned) = project(&[("root.nika.yaml", root), ("child.nika.yaml", child)]);
    let result = ExecutionService::default().admit_with_model_override(
        &owned,
        Path::new("root.nika.yaml"),
        Some("mock/echo"),
    );
    assert!(
        matches!(result, Err(ExecutionError::CheckFailed { findings })
        if findings.iter().any(|finding| finding.contains("child.nika.yaml") && finding.contains("exceeds")))
    );
}

fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, OwnedDir) {
    let tmp = tempfile::tempdir().expect("tempdir");
    for (name, body) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("fixture parent");
        }
        fs::write(path, body).expect("fixture write");
    }
    let owned = OwnedDir::open(tmp.path()).expect("owned project");
    (tmp, owned)
}

struct InterleavingSource {
    project: OwnedDir,
    target: &'static str,
    captured: Arc<Barrier>,
    replaced: Arc<Barrier>,
}

impl crate::snapshot::ByteSource for InterleavingSource {
    fn read(&self, logical_path: &str, limit: usize) -> Result<Vec<u8>, ExecutionError> {
        let bytes =
            <OwnedDir as crate::snapshot::ByteSource>::read(&self.project, logical_path, limit)?;
        if logical_path == self.target {
            self.captured.wait();
            self.replaced.wait();
        }
        Ok(bytes)
    }
}

fn interleaving_source(
    project: &OwnedDir,
    target: &'static str,
) -> (InterleavingSource, Arc<Barrier>, Arc<Barrier>) {
    let captured = Arc::new(Barrier::new(2));
    let replaced = Arc::new(Barrier::new(2));
    (
        InterleavingSource {
            project: project.try_clone().expect("source clone"),
            target,
            captured: Arc::clone(&captured),
            replaced: Arc::clone(&replaced),
        },
        captured,
        replaced,
    )
}

#[allow(
    clippy::disallowed_methods,
    reason = "the synchronous ByteSource race needs an OS thread held at explicit barriers"
)]
fn capture_while_replacing<F>(
    project: &OwnedDir,
    root: &Path,
    target: &'static str,
    replace: F,
) -> ExecutionSnapshot
where
    F: FnOnce() + Send + 'static,
{
    let (source, captured, replaced) = interleaving_source(project, target);
    let attacker = std::thread::spawn(move || {
        captured.wait();
        replace();
        replaced.wait();
    });
    let snapshot = ExecutionSnapshot::capture_from(
        &source,
        root,
        std::iter::empty::<&Path>(),
        SnapshotLimits::default(),
    )
    .expect("capture owns the target read before replacement");
    attacker.join().expect("attacker");
    snapshot
}

#[test]
fn child_replacement_interleaved_after_read_cannot_change_admitted_bytes() {
    let parent = parent("child.nika.yaml");
    let (tmp, owned) = project(&[("root.nika.yaml", &parent), ("child.nika.yaml", CHILD)]);
    let child = tmp.path().join("child.nika.yaml");
    let snapshot = capture_while_replacing(
        &owned,
        Path::new("root.nika.yaml"),
        "child.nika.yaml",
        move || fs::write(child, b"nika: replaced\n").expect("replace child"),
    );
    assert_eq!(snapshot.text("child.nika.yaml"), Some(CHILD));
}

#[test]
fn skill_mutation_after_capture_cannot_change_admitted_bytes() {
    let root = "nika: root\nmodel: mock/echo\npermits:\n  fs:\n    read: [\"skills/review/SKILL.md\"]\ntasks:\n  review:\n    agent: { prompt: \"review\", skills: [\"skills/review/SKILL.md\"] }\n";
    let skill = "---\nname: review\ndescription: Review code.\n---\nOriginal.\n";
    let (tmp, owned) = project(&[("root.nika.yaml", root), ("skills/review/SKILL.md", skill)]);
    let snapshot = ExecutionSnapshot::capture(
        &owned,
        Path::new("root.nika.yaml"),
        SnapshotLimits::default(),
    )
    .expect("capture");
    fs::write(tmp.path().join("skills/review/SKILL.md"), b"replacement").expect("mutate");
    assert_eq!(snapshot.text("skills/review/SKILL.md"), Some(skill));
    assert_eq!(
        snapshot
            .unit("skills/review/SKILL.md")
            .map(crate::CapturedUnit::kind),
        Some(SnapshotUnitKind::Skill)
    );
}

#[test]
fn nested_child_replacement_interleaved_after_read_cannot_change_admitted_bytes() {
    let root = parent("nested/middle.nika.yaml");
    let middle = "nika: middle\ninputs:\n  url: { type: string, required: true }\npermits:\n  exec: [\"echo\"]\ntasks:\n  leaf:\n    invoke:\n      workflow: \"leaf.nika.yaml\"\n      args: { url: \"${{ inputs.url }}\" }\n    returns: { object: { report: string } }\noutputs:\n  report: { value: \"${{ tasks.leaf.output.report }}\", type: string }\n";
    let (tmp, owned) = project(&[
        ("root.nika.yaml", &root),
        ("nested/middle.nika.yaml", middle),
        ("nested/leaf.nika.yaml", CHILD),
    ]);
    let child = tmp.path().join("nested/leaf.nika.yaml");
    let snapshot = capture_while_replacing(
        &owned,
        Path::new("root.nika.yaml"),
        "nested/leaf.nika.yaml",
        move || fs::write(child, b"nika: attacker\n").expect("replace nested child"),
    );
    assert_eq!(snapshot.text("nested/leaf.nika.yaml"), Some(CHILD));
}

#[cfg(unix)]
#[test]
fn symlink_swap_interleaved_after_read_cannot_redirect_execution() {
    use std::os::unix::fs::symlink;

    let parent = parent("child.nika.yaml");
    let (tmp, owned) = project(&[("root.nika.yaml", &parent), ("child.nika.yaml", CHILD)]);
    let child = tmp.path().join("child.nika.yaml");
    let outside = tempfile::NamedTempFile::new().expect("outside");
    fs::write(outside.path(), b"nika: attacker\n").expect("outside write");
    let outside_path = outside.path().to_owned();
    let snapshot = capture_while_replacing(
        &owned,
        Path::new("root.nika.yaml"),
        "child.nika.yaml",
        move || {
            fs::remove_file(&child).expect("remove child");
            symlink(outside_path, child).expect("swap");
        },
    );
    assert_eq!(snapshot.text("child.nika.yaml"), Some(CHILD));
}

#[test]
fn directory_replacement_interleaved_after_read_cannot_redirect_execution() {
    let root = parent("world/child.nika.yaml");
    let (tmp, owned) = project(&[("root.nika.yaml", &root), ("world/child.nika.yaml", CHILD)]);
    let world = tmp.path().join("world");
    let original_world = tmp.path().join("original-world");
    let snapshot = capture_while_replacing(
        &owned,
        Path::new("root.nika.yaml"),
        "world/child.nika.yaml",
        move || {
            fs::rename(&world, original_world).expect("rename original directory");
            fs::create_dir(&world).expect("replacement directory");
            fs::write(world.join("child.nika.yaml"), b"nika: attacker\n")
                .expect("replacement child");
        },
    );
    assert_eq!(snapshot.text("world/child.nika.yaml"), Some(CHILD));
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the synchronous ByteSource race needs an OS thread held at explicit barriers"
)]
fn root_replacement_interleaved_after_read_cannot_change_admitted_bytes() {
    let (tmp, owned) = project(&[("root.nika.yaml", pure_root())]);
    let (source, captured, replaced) = interleaving_source(&owned, "root.nika.yaml");
    let root = tmp.path().join("root.nika.yaml");
    let attacker = std::thread::spawn(move || {
        captured.wait();
        fs::write(root, b"nika: attacker\n").expect("replace root");
        replaced.wait();
    });
    let snapshot = ExecutionSnapshot::capture_from(
        &source,
        Path::new("root.nika.yaml"),
        std::iter::empty::<&Path>(),
        SnapshotLimits::default(),
    )
    .expect("capture owns the already-read root");
    attacker.join().expect("attacker");
    assert_eq!(snapshot.text("root.nika.yaml"), Some(pure_root()));
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "the synchronous ByteSource race needs an OS thread held at explicit barriers"
)]
fn skill_registry_reload_interleaved_after_read_cannot_change_admission() {
    const ROOT: &str = "nika: root\nmodel: mock/echo\npermits:\n  fs:\n    read: [\"skills/review/SKILL.md\"]\ntasks:\n  review:\n    agent: { prompt: \"review\", skills: [\"skills/review/SKILL.md\"] }\n";
    const SKILL: &str = "---\nname: review\ndescription: Review code.\n---\nOriginal.\n";
    let (tmp, owned) = project(&[("root.nika.yaml", ROOT), ("skills/review/SKILL.md", SKILL)]);
    let (source, captured, replaced) = interleaving_source(&owned, "skills/review/SKILL.md");
    let skill = tmp.path().join("skills/review/SKILL.md");
    let attacker = std::thread::spawn(move || {
        captured.wait();
        fs::write(skill, b"attacker registry reload").expect("replace skill");
        replaced.wait();
    });
    let snapshot = ExecutionSnapshot::capture_from(
        &source,
        Path::new("root.nika.yaml"),
        std::iter::empty::<&Path>(),
        SnapshotLimits::default(),
    )
    .expect("capture owns the already-read skill");
    attacker.join().expect("attacker");
    let admitted = ExecutionService::default()
        .readmit_snapshot(snapshot)
        .expect("snapshot admits");
    assert_eq!(
        admitted.snapshot().text("skills/review/SKILL.md"),
        Some(SKILL)
    );
}

#[test]
fn cycles_are_refused() {
    let a = parent("b.nika.yaml");
    let b = parent("a.nika.yaml");
    let (_tmp, owned) = project(&[("a.nika.yaml", &a), ("b.nika.yaml", &b)]);
    let error =
        ExecutionSnapshot::capture(&owned, Path::new("a.nika.yaml"), SnapshotLimits::default())
            .expect_err("cycle");
    assert!(matches!(error, ExecutionError::DependencyCycle { .. }));
}

#[test]
fn duplicate_logical_aliases_are_refused() {
    let root = "nika: root\npermits:\n  exec: [\"echo\"]\ntasks:\n  one:\n    invoke: { workflow: \"child.nika.yaml\", args: { url: x } }\n  two:\n    invoke: { workflow: \"./child.nika.yaml\", args: { url: y } }\n".to_owned();
    let (_tmp, owned) = project(&[("root.nika.yaml", &root), ("child.nika.yaml", CHILD)]);
    let error = ExecutionSnapshot::capture(
        &owned,
        Path::new("root.nika.yaml"),
        SnapshotLimits::default(),
    )
    .expect_err("alias duplicate");
    assert!(matches!(
        error,
        ExecutionError::DuplicateLogicalIdentity { .. }
    ));
}

#[test]
fn identical_duplicate_imports_are_refused() {
    let (_tmp, owned) = project(&[
        ("root.nika.yaml", pure_root()),
        ("imports/policy.bin", "policy-v1"),
    ]);
    let error = ExecutionSnapshot::capture_with_imports(
        &owned,
        Path::new("root.nika.yaml"),
        [
            Path::new("imports/policy.bin"),
            Path::new("imports/policy.bin"),
        ],
        SnapshotLimits::default(),
    )
    .expect_err("duplicate import");
    assert!(matches!(
        error,
        ExecutionError::DuplicateLogicalIdentity { .. }
    ));
}

#[test]
fn child_local_static_defects_are_refused() {
    let root = parent("child.nika.yaml");
    let child = "nika: child\npermits: {}\ntasks:\n  broken:\n    after: [missing]\n    invoke: { tool: nika:jq, args: { input: 1, expression: \".\" } }\n";
    let (_tmp, owned) = project(&[("root.nika.yaml", &root), ("child.nika.yaml", child)]);
    let error = ExecutionSnapshot::capture(
        &owned,
        Path::new("root.nika.yaml"),
        SnapshotLimits::default(),
    )
    .expect_err("child check");
    assert!(matches!(
        error,
        ExecutionError::CheckFailed { .. } | ExecutionError::Parse { .. }
    ));
}

#[test]
fn parse_refuse_stamps_the_spec_code_in_the_detail() {
    let (_tmp, owned) = project(&[("bad.nika.yaml", "nika: v1\nworkflow: nope\n")]);
    let error = ExecutionService::default()
        .admit(&owned, Path::new("bad.nika.yaml"))
        .expect_err("fourteen-key parse");
    let text = error.to_string();
    assert!(
        text.contains("NIKA-PARSE-"),
        "parse refuse must name a spec code: {text}"
    );
}

#[test]
fn check_refuse_stamps_the_analysis_code_in_the_detail() {
    let body = "nika: boom\ntasks:\n  t:\n    exec: { command: [\"true\"] }\n";
    let (_tmp, owned) = project(&[("boom.nika.yaml", body)]);
    let error = ExecutionSnapshot::capture(
        &owned,
        Path::new("boom.nika.yaml"),
        SnapshotLimits::default(),
    )
    .expect_err("undeclared exec");
    let text = error.to_string();
    assert!(
        text.contains("NIKA-AUTH-006") || text.contains("NIKA-SEC-"),
        "check refuse must name a spec code: {text}"
    );
}

#[test]
fn depth_count_and_size_limits_fail_closed() {
    let root = parent("child.nika.yaml");
    let (_tmp, owned) = project(&[("root.nika.yaml", &root), ("child.nika.yaml", CHILD)]);
    let depth = SnapshotLimits::new(0, 8, 8_192, 16_384);
    assert!(matches!(
        ExecutionSnapshot::capture(&owned, Path::new("root.nika.yaml"), depth),
        Err(ExecutionError::DepthLimit { .. })
    ));

    let count = SnapshotLimits::new(8, 1, 8_192, 16_384);
    assert!(matches!(
        ExecutionSnapshot::capture(&owned, Path::new("root.nika.yaml"), count),
        Err(ExecutionError::UnitCountLimit { .. })
    ));

    let size = SnapshotLimits::new(8, 8, 16, 16_384);
    assert!(matches!(
        ExecutionSnapshot::capture(&owned, Path::new("root.nika.yaml"), size),
        Err(ExecutionError::UnitSizeLimit { .. })
    ));

    let total = SnapshotLimits::new(8, 8, 8_192, root.len());
    assert!(matches!(
        ExecutionSnapshot::capture(&owned, Path::new("root.nika.yaml"), total),
        Err(ExecutionError::TotalSizeLimit { .. })
    ));
}

#[test]
fn digest_is_stable_for_the_same_owned_world() {
    let parent = parent("child.nika.yaml");
    let (_a_tmp, a) = project(&[("root.nika.yaml", &parent), ("child.nika.yaml", CHILD)]);
    let (_b_tmp, b) = project(&[("root.nika.yaml", &parent), ("child.nika.yaml", CHILD)]);
    let left =
        ExecutionSnapshot::capture(&a, Path::new("root.nika.yaml"), SnapshotLimits::default())
            .expect("left");
    let right =
        ExecutionSnapshot::capture(&b, Path::new("root.nika.yaml"), SnapshotLimits::default())
            .expect("right");
    assert_eq!(left.digest(), right.digest());
}

#[test]
fn explicit_imports_join_the_same_immutable_world() {
    let (tmp, owned) = project(&[
        ("root.nika.yaml", pure_root()),
        ("imports/policy.bin", "policy-v1"),
    ]);
    let snapshot = ExecutionSnapshot::capture_with_imports(
        &owned,
        Path::new("root.nika.yaml"),
        [Path::new("imports/policy.bin")],
        SnapshotLimits::default(),
    )
    .expect("capture import");
    fs::write(tmp.path().join("imports/policy.bin"), b"policy-v2").expect("mutate import");
    assert_eq!(
        snapshot.bytes("imports/policy.bin"),
        Some(b"policy-v1".as_slice())
    );
    assert_eq!(
        snapshot
            .unit("imports/policy.bin")
            .map(crate::CapturedUnit::kind),
        Some(SnapshotUnitKind::Import)
    );
}

#[test]
fn execute_performs_zero_opens_after_admission() {
    let (tmp, owned) = project(&[("root.nika.yaml", pure_root())]);
    let service = ExecutionService::default();
    let admitted = service
        .admit(&owned, Path::new("root.nika.yaml"))
        .expect("admit");
    fs::remove_file(tmp.path().join("root.nika.yaml")).expect("remove admitted source");
    let verdict = service.execute(admitted, snapshot_digest);
    assert_eq!(verdict.outcome(), verdict.snapshot_digest());
    assert_eq!(verdict.trace_id(), verdict.execution_id().into());
}

#[test]
fn debug_surfaces_do_not_disclose_captured_or_outcome_payloads() {
    const SECRET_SHAPED: &str = "sk-live-w02-must-never-reach-debug";
    let root = format!("{}# {SECRET_SHAPED}\n", pure_root());
    let (_tmp, owned) = project(&[("root.nika.yaml", &root)]);
    let service = ExecutionService::default();
    let admitted = service
        .admit(&owned, Path::new("root.nika.yaml"))
        .expect("admit");

    let snapshot_debug = format!("{:?}", admitted.snapshot());
    let unit_debug = format!(
        "{:?}",
        admitted
            .snapshot()
            .unit("root.nika.yaml")
            .expect("root unit")
    );
    let admitted_debug = format!("{admitted:?}");
    assert!(!snapshot_debug.contains(SECRET_SHAPED));
    assert!(!unit_debug.contains(SECRET_SHAPED));
    assert!(!admitted_debug.contains(SECRET_SHAPED));

    let context_debug = service.execute(admitted, debug_execution_context);
    assert!(!context_debug.outcome().contains(SECRET_SHAPED));

    let admitted = service
        .admit(&owned, Path::new("root.nika.yaml"))
        .expect("readmit");
    let verdict = service.execute(admitted, secret_shaped_outcome);
    let verdict_debug = format!("{verdict:?}");
    assert!(!verdict_debug.contains(SECRET_SHAPED));
    assert_eq!(*verdict.outcome(), SECRET_SHAPED);
}

#[test]
fn owned_root_bytes_capture_stdin_world_without_a_dash_file() {
    let root = "nika: stdin\nmodel: mock/echo\npermits:\n  exec: [\"echo\"]\n  fs:\n    read: [\"skills/review/SKILL.md\"]\ntasks:\n  audit:\n    invoke:\n      workflow: \"child.nika.yaml\"\n      args: { url: \"https://example.com\" }\n    returns: { object: { report: string } }\n  review:\n    agent: { prompt: \"review\", skills: [\"skills/review/SKILL.md\"] }\n";
    let skill = "---\nname: review\ndescription: Review code.\n---\nOriginal.\n";
    let (tmp, owned) = project(&[
        ("child.nika.yaml", CHILD),
        ("skills/review/SKILL.md", skill),
    ]);
    let service = ExecutionService::default();
    let admitted = service
        .admit_root_bytes(&owned, Path::new("-"), root.as_bytes())
        .expect("admit stdin world");

    fs::write(tmp.path().join("child.nika.yaml"), b"nika: replaced\n").expect("mutate child");
    fs::write(tmp.path().join("skills/review/SKILL.md"), b"replacement").expect("mutate skill");

    assert!(!tmp.path().join("-").exists());
    assert_eq!(admitted.snapshot().root(), "-");
    assert_eq!(admitted.snapshot().text("-"), Some(root));
    assert_eq!(admitted.snapshot().text("child.nika.yaml"), Some(CHILD));
    assert_eq!(
        admitted.snapshot().text("skills/review/SKILL.md"),
        Some(skill)
    );
}

#[test]
fn owned_root_bytes_obey_the_same_size_ceiling() {
    let (_tmp, owned) = project(&[]);
    let limits = SnapshotLimits::new(8, 8, 16, 16_384);
    let service = ExecutionService::new(limits);
    let error = service
        .admit_root_bytes(&owned, Path::new("-"), pure_root().as_bytes())
        .expect_err("oversized stdin root");
    assert!(matches!(error, ExecutionError::UnitSizeLimit { .. }));
}

fn snapshot_digest(cx: crate::ExecutionContext<'_>) -> String {
    assert_eq!(cx.snapshot().text("root.nika.yaml"), Some(pure_root()));
    cx.snapshot().digest().to_owned()
}

fn secret_shaped_outcome(_cx: crate::ExecutionContext<'_>) -> &'static str {
    "sk-live-w02-must-never-reach-debug"
}

fn debug_execution_context(cx: crate::ExecutionContext<'_>) -> String {
    format!("{cx:?}")
}
