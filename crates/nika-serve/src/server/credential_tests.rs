// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::tests::TestWorld;
use super::{
    BoundServer, CredentialRefuse, ExecutionBackend, ExecutionContext, ExecutionDisposition,
    ExecutionOutcome, ServerConfig, ServerError,
};

struct CompletingBackend;

impl ExecutionBackend for CompletingBackend {
    fn execute<'a>(
        &'a self,
        _context: ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        Box::pin(async move { ExecutionDisposition::Succeeded.into() })
    }
}

fn backend() -> Arc<CompletingBackend> {
    Arc::new(CompletingBackend)
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn credential_world_readable_refuses_insecure_mode() {
    use std::os::unix::fs::PermissionsExt as _;

    let world = TestWorld::new();
    std::fs::set_permissions(&world.token, std::fs::Permissions::from_mode(0o644)).expect("mode");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.state,
        &world.token,
    );

    assert!(matches!(
        BoundServer::bind(config, backend()).await,
        Err(ServerError::Credential(CredentialRefuse::InsecureMode))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn credential_short_material_refuses_invalid_material() {
    let world = TestWorld::new();
    std::fs::write(&world.token, "too-short\n").expect("token");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&world.token, std::fs::Permissions::from_mode(0o600))
            .expect("mode");
    }
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.state,
        &world.token,
    );

    assert!(matches!(
        BoundServer::bind(config, backend()).await,
        Err(ServerError::Credential(CredentialRefuse::InvalidMaterial))
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn credential_missing_file_refuses_unreadable() {
    let world = TestWorld::new();
    let missing = world.root.path().join("absent.token");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.state,
        missing,
    );

    assert!(matches!(
        BoundServer::bind(config, backend()).await,
        Err(ServerError::Credential(CredentialRefuse::Unreadable))
    ));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn credential_symlink_refuses_follow() {
    use std::os::unix::fs::symlink;

    let world = TestWorld::new();
    let linked = world.root.path().join("linked.token");
    symlink(&world.token, &linked).expect("token symlink");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.state,
        linked,
    );

    assert!(matches!(
        BoundServer::bind(config, backend()).await,
        Err(ServerError::Credential(CredentialRefuse::FollowRefused))
    ));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn credential_fifo_refuses_without_waiting_for_a_writer() {
    use nix::sys::stat::Mode;
    use nix::unistd::mkfifo;

    let world = TestWorld::new();
    let fifo = world.root.path().join("fifo.token");
    mkfifo(&fifo, Mode::from_bits_truncate(0o600)).expect("token fifo");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.state,
        fifo,
    );

    let result = tokio::time::timeout(
        Duration::from_millis(100),
        BoundServer::bind(config, backend()),
    )
    .await
    .expect("FIFO acquisition must not block");
    assert!(matches!(
        result,
        Err(ServerError::Credential(CredentialRefuse::FollowRefused))
    ));
}
