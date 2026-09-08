// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Read argument failures must never select a missing-file recovery.

use std::sync::atomic::{AtomicUsize, Ordering};

use bytes::Bytes;
use nika_kernel_mock::MockFs;
use serde_json::json;

use super::*;

fn args(value: serde_json::Value) -> Args {
    match value {
        serde_json::Value::Object(map) => map,
        other => panic!("test object required, got {other}"),
    }
}

#[tokio::test]
async fn read_invalid_arguments_are_not_missing_files() {
    let fs = MockFs::new().with_file("present.txt", "existing bytes");
    for value in [
        json!("true"),
        json!("false"),
        json!(1),
        json!(null),
        json!([]),
        json!({}),
    ] {
        let error = read(&fs, &args(json!({"path": "present.txt", "binary": value})))
            .await
            .expect_err("nonboolean binary must fail");
        assert_eq!(error.code, "NIKA-INVOKE-002", "{value}: {error:?}");
        assert!(!error.transient);
        assert!(error.message.contains("binary"));
    }
    for value in [json!(true), json!(12), json!(null), json!([]), json!({})] {
        let error = read(&fs, &args(json!({"path": value})))
            .await
            .expect_err("nonstring path must fail");
        assert_eq!(error.code, "NIKA-INVOKE-002", "{value}: {error:?}");
        assert!(error.message.contains("path"));
    }
    assert_eq!(
        read(&fs, &Args::new())
            .await
            .expect_err("missing path")
            .code,
        "NIKA-INVOKE-002"
    );
}

struct UnreadableFs(AtomicUsize);

impl FsReadDyn for UnreadableFs {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Err(FsError::PermissionDenied {
            path: path.display().to_string(),
        })
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        self.read(path).await.map(|_| String::new())
    }

    async fn exists(&self, _: &Path) -> bool {
        false
    }

    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        Ok(path.to_owned())
    }
}

#[tokio::test]
async fn read_invalid_arguments_do_not_call_the_read_seam() {
    let fs = UnreadableFs(AtomicUsize::new(0));
    for value in [
        json!({}),
        json!({"path": false}),
        json!({"path": "present.txt", "binary": "true"}),
    ] {
        let error = read(&fs, &args(value)).await.expect_err("invalid args");
        assert_eq!(error.code, "NIKA-INVOKE-002");
    }
    assert_eq!(fs.0.load(Ordering::Relaxed), 0);
    for binary in [false, true] {
        let error = read(&fs, &args(json!({"path": "unreadable", "binary": binary})))
            .await
            .expect_err("permission failure");
        assert_eq!(error.code, "NIKA-BUILTIN-READ-002");
    }
    assert_eq!(fs.0.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn read_valid_modes_and_file_failures_keep_their_contract() {
    let text = "\u{feff}hello\r\n\0no final newline";
    let fs = MockFs::new()
        .with_file("text", text)
        .with_file("bytes", vec![0, 255, 128, 10]);
    for value in [
        json!({"path": "text"}),
        json!({"path": "text", "binary": false}),
    ] {
        assert_eq!(
            read(&fs, &args(value)).await.expect("text read"),
            json!(text)
        );
    }
    assert_eq!(
        read(&fs, &args(json!({"path": "bytes", "binary": true})))
            .await
            .expect("binary read"),
        json!({"bytes_base64": "AP+ACg==", "len": 4})
    );
    assert_eq!(
        read(&fs, &args(json!({"path": "bytes"})))
            .await
            .expect_err("invalid UTF-8")
            .code,
        "NIKA-BUILTIN-READ-003"
    );
    for binary in [false, true] {
        assert_eq!(
            read(&fs, &args(json!({"path": "missing", "binary": binary})))
                .await
                .expect_err("missing file")
                .code,
            "NIKA-BUILTIN-READ-001"
        );
    }
}
