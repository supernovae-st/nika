// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::{Path, PathBuf};

use bytes::Bytes;
use nika_fs::TokioFs;
use nika_kernel::fs::{FsError, FsReadDyn, FsWriteDyn};
use tokio::sync::Barrier;

struct ObservedAbsence {
    target: PathBuf,
    observed: Barrier,
    created: Barrier,
}

impl FsReadDyn for ObservedAbsence {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        TokioFs.read(path).await
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        TokioFs.read_to_string(path).await
    }

    async fn exists(&self, path: &Path) -> bool {
        let exists = TokioFs.exists(path).await;
        if path == self.target {
            assert!(!exists, "the precheck must really observe an absent target");
            self.observed.wait().await;
            self.created.wait().await;
        }
        exists
    }

    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        TokioFs.canonicalize(path).await
    }
}

impl FsWriteDyn for ObservedAbsence {
    async fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        TokioFs.write_new(path, contents).await
    }

    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        TokioFs.write(path, contents).await
    }

    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        TokioFs.create_dir_all(path).await
    }

    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        TokioFs.remove_file(path).await
    }
}

struct Scratch(PathBuf);

#[tokio::test]
async fn mock_exclusive_write_preserves_an_implicit_directory() -> std::io::Result<()> {
    let fs = nika_kernel_mock::MockFs::new().with_file("dir/child", b"peer");
    let refused = fs.write_new(Path::new("dir"), b"replacement").await;
    assert!(matches!(refused, Err(FsError::AlreadyExists { .. })));
    assert_eq!(
        fs.read(Path::new("dir/child"))
            .await
            .map_err(std::io::Error::other)?
            .as_ref(),
        b"peer"
    );
    assert!(fs.read(Path::new("dir")).await.is_err());
    Ok(())
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0); // seam-bypass-ok: test owns and observes its real filesystem scratch.
    }
}

#[tokio::test]
async fn write_preserves_a_target_created_after_absence_check() -> std::io::Result<()> {
    write_race(false).await
}

#[tokio::test]
async fn judged_write_preserves_a_target_created_after_absence_check() -> std::io::Result<()> {
    write_race(true).await
}

async fn write_race(through_judged: bool) -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!("nika-write-race-{}", uuid::Uuid::new_v4())); // seam-bypass-ok: unique test scratch name, not workflow entropy.
    std::fs::create_dir(&root).map_err(std::io::Error::other)?; // seam-bypass-ok: test owns and observes its real filesystem scratch.
    let scratch = Scratch(root);
    let fs = ObservedAbsence {
        target: scratch.0.join("target.txt"),
        observed: Barrier::new(2),
        created: Barrier::new(2),
    };
    let args = serde_json::from_value(serde_json::json!({
        "path": fs.target, "content": "first-writer", "overwrite": false
    }))
    .map_err(std::io::Error::other)?;
    let peer = async {
        fs.observed.wait().await;
        let written = tokio::fs::write(&fs.target, b"peer-complete").await;
        fs.created.wait().await;
        written
    };
    let boundary = crate::FsBoundary::unbounded();
    let judged = crate::judged_fs::JudgedFs::new(&fs, &boundary);
    let write = async {
        if through_judged {
            super::write(&judged, &args).await
        } else {
            super::write(&fs, &args).await
        }
    };
    let (result, peer_result) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(write, peer)
    })
    .await
    .map_err(std::io::Error::other)?;
    peer_result.map_err(std::io::Error::other)?;
    let observed = tokio::fs::read(&fs.target)
        .await
        .map_err(std::io::Error::other)?;
    assert_eq!(observed, b"peer-complete", "write result: {result:?}");
    assert!(matches!(result, Err(failure) if failure.code == "NIKA-BUILTIN-WRITE-002"));
    Ok(())
}

#[tokio::test]
async fn exclusive_write_uses_the_dispatch_guard_and_judged_backend() -> std::io::Result<()> {
    use std::sync::Arc;

    use nika_kernel::runtime::tool_executor::{ToolCall, ToolExecuteDyn};
    use nika_kernel_mock::{MockClock, MockHttp};

    let root = std::env::temp_dir().join(format!("nika-write-guard-{}", uuid::Uuid::new_v4())); // seam-bypass-ok: unique test scratch name, not workflow entropy.
    std::fs::create_dir(&root).map_err(std::io::Error::other)?; // seam-bypass-ok: test owns and observes its real filesystem scratch.
    let scratch = Scratch(root);
    let boundary =
        crate::FsBoundary::declared(vec![], vec![format!("{}/allowed/**", scratch.0.display())]);
    let dispatcher = crate::BuiltinDispatcher::new(
        Arc::new(TokioFs),
        Arc::new(MockHttp::new()),
        Arc::new(MockClock::new()),
        Arc::new(crate::NullEmitter),
        Arc::new(crate::NonInteractive),
        Arc::new(crate::NoWorkflow),
    )
    .with_fs_boundary(boundary);
    let target = scratch.0.join("allowed/new/file.bin");
    let call = |path: &Path| {
        ToolCall::new(
            "write",
            "nika:write",
            serde_json::json!({
                "path": path, "content": {"bytes_base64": "AP8="}, "overwrite": false
            }),
        )
    };
    let created = dispatcher
        .execute(call(&target))
        .await
        .map_err(std::io::Error::other)?;
    assert!(!created.is_error, "{}", created.content);
    assert_eq!(std::fs::read(&target)?, [0, 255]); // seam-bypass-ok: independent test observation.
    let refused = dispatcher
        .execute(call(&target))
        .await
        .map_err(std::io::Error::other)?;
    assert!(refused.is_error && refused.content.starts_with("NIKA-BUILTIN-WRITE-002"));
    assert_eq!(std::fs::read(&target)?, [0, 255]); // seam-bypass-ok: independent test observation.
    let outside = scratch.0.join("denied/new/file.bin");
    let denied = dispatcher
        .execute(call(&outside))
        .await
        .map_err(std::io::Error::other)?;
    assert!(denied.is_error && denied.content.starts_with("NIKA-SEC-004"));
    assert!(!scratch.0.join("denied").exists());
    Ok(())
}
