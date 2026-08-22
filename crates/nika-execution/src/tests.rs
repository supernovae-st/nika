use std::fs;
use std::path::Path;

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

#[test]
fn child_mutation_after_capture_cannot_change_admitted_bytes() {
    let parent = parent("child.nika.yaml");
    let (tmp, owned) = project(&[("root.nika.yaml", &parent), ("child.nika.yaml", CHILD)]);
    let snapshot = ExecutionSnapshot::capture(
        &owned,
        Path::new("root.nika.yaml"),
        SnapshotLimits::default(),
    )
    .expect("capture");
    fs::write(tmp.path().join("child.nika.yaml"), b"nika: replaced\n").expect("mutate");
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

#[cfg(unix)]
#[test]
fn symlink_swap_after_capture_cannot_redirect_execution() {
    use std::os::unix::fs::symlink;

    let parent = parent("child.nika.yaml");
    let (tmp, owned) = project(&[("root.nika.yaml", &parent), ("child.nika.yaml", CHILD)]);
    let snapshot = ExecutionSnapshot::capture(
        &owned,
        Path::new("root.nika.yaml"),
        SnapshotLimits::default(),
    )
    .expect("capture");
    let outside = tempfile::NamedTempFile::new().expect("outside");
    fs::write(outside.path(), b"nika: attacker\n").expect("outside write");
    fs::remove_file(tmp.path().join("child.nika.yaml")).expect("remove child");
    symlink(outside.path(), tmp.path().join("child.nika.yaml")).expect("swap");
    assert_eq!(snapshot.text("child.nika.yaml"), Some(CHILD));
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

fn snapshot_digest(cx: crate::ExecutionContext<'_>) -> String {
    assert_eq!(cx.snapshot().text("root.nika.yaml"), Some(pure_root()));
    cx.snapshot().digest().to_owned()
}
