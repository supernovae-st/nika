// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The injected store over the real descriptor-rooted owner, in disposable
//! directories only. No provider, credential, user directory or network access.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use nika_blob::FsBlobStore;
use nika_fs::{EffectLedger, OwnedDir, RoomLimits, RootedFs};
use nika_kernel::BlobError;
use nika_kernel::blob::BlobStoreDyn;
use tempfile::TempDir;

const DATA: &[u8] = b"owned image bytes";
const MIME: &str = "image/png";
const SENTINEL: &[u8] = b"outside unchanged";

struct Room {
    temp: TempDir,
    project: PathBuf,
    outside: PathBuf,
    ledger: Arc<EffectLedger>,
    fs: Arc<RootedFs>,
    store: FsBlobStore<RootedFs>,
}

impl Room {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), SENTINEL).unwrap();
        let ledger = EffectLedger::new(RoomLimits::new(1024 * 1024, 128));
        let fs = Arc::new(RootedFs::new(
            OwnedDir::open(&project).unwrap(),
            Arc::clone(&ledger),
        ));
        let store = FsBlobStore::new(Arc::clone(&fs), ".nika/blobs", 64).unwrap();
        Self {
            temp,
            project,
            outside,
            ledger,
            fs,
            store,
        }
    }

    fn paths(&self) -> (String, PathBuf, PathBuf) {
        let raw = blake3::hash(DATA).to_hex().to_string();
        let shard = self.project.join(".nika/blobs").join(&raw[..2]);
        let blob = shard.join(&raw[2..]);
        let mime = shard.join(format!("{}.mime", &raw[2..]));
        (format!("blake3:{raw}"), blob, mime)
    }

    async fn intact(&self) {
        assert_eq!(
            std::fs::read(self.outside.join("sentinel")).unwrap(),
            SENTINEL
        );
        assert_eq!(std::fs::read_dir(&self.outside).unwrap().count(), 1);
        let drained = self.ledger.seal().join().await;
        assert_eq!(drained.panicked, 0);
        assert_eq!(self.ledger.leftovers(), 0);
    }
}

#[tokio::test]
async fn roundtrip_dedup_and_delete_use_the_owned_filesystem() {
    let room = Room::new();
    assert!(
        !room.project.join(".nika").exists(),
        "constructor performs no writes"
    );
    let first = room
        .store
        .put(Bytes::from_static(DATA), MIME)
        .await
        .unwrap();
    let second = room
        .store
        .put(Bytes::from_static(DATA), "image/webp")
        .await
        .unwrap();
    assert_eq!(first.hash, second.hash);
    assert_eq!(room.store.get(&first.hash).await.unwrap(), DATA);
    let stat = room.store.stat(&first.hash).await.unwrap();
    assert_eq!(stat.mime_type, "image/webp");
    assert_eq!(stat.size, DATA.len() as u64);
    assert!(room.store.exists(&first.hash).await);
    room.store.delete(&first.hash).await.unwrap();
    assert!(!room.store.exists(&first.hash).await);
    assert!(matches!(
        room.store.delete(&first.hash).await,
        Err(BlobError::NotFound { .. })
    ));
    room.intact().await;
}

#[tokio::test]
async fn invalid_inputs_do_not_create_the_store() {
    let room = Room::new();
    for root in ["", "/tmp/blobs", "../outside", ".nika/../../outside", "."] {
        assert!(
            FsBlobStore::new(Arc::clone(&room.fs), root, 64).is_err(),
            "{root}"
        );
    }
    assert!(room.store.put(Bytes::new(), MIME).await.is_err());
    assert!(room.store.put(Bytes::from_static(DATA), " ").await.is_err());
    assert!(matches!(
        room.store.put(Bytes::from(vec![0; 65]), MIME).await,
        Err(BlobError::TooLarge {
            size: 65,
            max: 64,
            ..
        })
    ));
    for key in ["../outside/sentinel", "aé", ""] {
        assert!(matches!(
            room.store.get(key).await,
            Err(BlobError::NotFound { .. })
        ));
        assert!(!room.store.exists(key).await);
    }
    assert!(!room.project.join(".nika").exists());
    room.intact().await;
}

#[tokio::test]
async fn links_at_every_store_parent_refuse_without_outside_effects() {
    let raw = blake3::hash(DATA).to_hex().to_string();
    for parent in [
        PathBuf::from(".nika"),
        PathBuf::from(".nika/blobs"),
        Path::new(".nika/blobs").join(&raw[..2]),
    ] {
        let room = Room::new();
        let link = room.project.join(parent);
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&room.outside, &link).unwrap();
        assert!(matches!(
            room.store.put(Bytes::from_static(DATA), MIME).await,
            Err(BlobError::Io { .. })
        ));
        let (hash, _, _) = room.paths();
        assert!(room.store.get(&hash).await.is_err());
        assert!(room.store.stat(&hash).await.is_err());
        assert!(room.store.delete(&hash).await.is_err());
        assert!(!room.store.exists(&hash).await);
        assert!(
            std::fs::symlink_metadata(link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        room.intact().await;
    }
}

#[tokio::test]
async fn a_symlinked_blob_is_neither_deduplicated_read_nor_deleted() {
    let room = Room::new();
    let (hash, blob, _) = room.paths();
    std::fs::create_dir_all(blob.parent().unwrap()).unwrap();
    symlink(room.outside.join("sentinel"), &blob).unwrap();
    assert!(
        room.store
            .put(Bytes::from_static(DATA), MIME)
            .await
            .is_err()
    );
    assert!(room.store.get(&hash).await.is_err());
    assert!(room.store.stat(&hash).await.is_err());
    assert!(room.store.delete(&hash).await.is_err());
    assert!(!room.store.exists(&hash).await);
    assert!(
        std::fs::symlink_metadata(blob)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    room.intact().await;
}

#[tokio::test]
async fn a_sidecar_link_is_not_read_and_publication_replaces_only_the_link() {
    let room = Room::new();
    let (hash, _, sidecar) = room.paths();
    room.store
        .put(Bytes::from_static(DATA), MIME)
        .await
        .unwrap();
    std::fs::remove_file(&sidecar).unwrap();
    symlink(room.outside.join("sentinel"), &sidecar).unwrap();
    assert!(matches!(
        room.store.stat(&hash).await,
        Err(BlobError::Io { .. })
    ));
    room.store
        .put(Bytes::from_static(DATA), MIME)
        .await
        .unwrap();
    assert!(
        !std::fs::symlink_metadata(sidecar)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(room.store.stat(&hash).await.unwrap().mime_type, MIME);
    room.intact().await;
}

#[tokio::test]
async fn a_replaced_project_path_cannot_redirect_the_held_anchor() {
    let room = Room::new();
    let held = room.temp.path().join("held-project");
    std::fs::rename(&room.project, &held).unwrap();
    symlink(&room.outside, &room.project).unwrap();
    let result = room
        .store
        .put(Bytes::from_static(DATA), MIME)
        .await
        .unwrap();
    let raw = result.hash.strip_prefix("blake3:").unwrap();
    assert_eq!(
        std::fs::read(held.join(".nika/blobs").join(&raw[..2]).join(&raw[2..])).unwrap(),
        DATA
    );
    assert_eq!(room.store.get(&result.hash).await.unwrap(), DATA);
    assert!(
        OwnedDir::open(&room.project).is_err(),
        "a fresh anchor must refuse the substituted root"
    );
    room.intact().await;
}

#[tokio::test]
async fn a_directory_or_mismatched_existing_blob_cannot_be_claimed_as_stored() {
    let room = Room::new();
    let (hash, blob, _) = room.paths();
    std::fs::create_dir_all(&blob).unwrap();
    assert!(
        room.store
            .put(Bytes::from_static(DATA), MIME)
            .await
            .is_err()
    );
    assert!(room.store.stat(&hash).await.is_err());
    assert!(!room.store.exists(&hash).await);
    std::fs::remove_dir(&blob).unwrap();
    std::fs::write(&blob, b"foreign bytes").unwrap();
    assert!(
        room.store
            .put(Bytes::from_static(DATA), MIME)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&blob).unwrap(), b"foreign bytes");
    room.intact().await;
}

#[tokio::test]
async fn missing_mime_falls_back_but_a_directory_mime_is_an_error() {
    let room = Room::new();
    let (hash, _, mime) = room.paths();
    room.store
        .put(Bytes::from_static(DATA), MIME)
        .await
        .unwrap();
    std::fs::remove_file(&mime).unwrap();
    assert_eq!(
        room.store.stat(&hash).await.unwrap().mime_type,
        "application/octet-stream"
    );
    std::fs::create_dir(&mime).unwrap();
    assert!(matches!(
        room.store.stat(&hash).await,
        Err(BlobError::Io { .. })
    ));
    room.intact().await;
}
