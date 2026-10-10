// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A persistent conversation on the spawned adapter: identity before dialect, the Codex
//! profile proven on its exact binary before the spawn (its configuration read back with
//! Nika's server the only one enabled), a fresh scratch directory as the session's root, and
//! the adapter's whole process group ended with the conversation.

use nika_kernel::ai::harness::HarnessError;

use super::{SpawnedHarness, kill_probe_group};
use crate::authoring::acp::{Profile, refusal};
use crate::client::conversation::{Opening, open};
use crate::conversation::{self as law, Conversation, ConversationSetup, ToolOffer};

/// What a conversation keeps alive and lets go together: the adapter child, its process group
/// (an npm wrapper's grandchild goes with it) and its scratch directory.
struct Kept {
    _child: tokio::process::Child,
    group: Option<u32>,
    _scratch: tempfile::TempDir,
}

impl Drop for Kept {
    fn drop(&mut self) {
        kill_probe_group(self.group);
    }
}

impl SpawnedHarness {
    /// Open one persistent conversation on this adapter under its audited conversation
    /// profile, mounting Nika's tool server `offer` ([`crate::conversation`]): the version is
    /// judged before the spawn, the agent's identity and the transport after `initialize`, and
    /// the selection `setup` asks before the first prompt.
    ///
    /// # Errors
    /// The adapter has no audited conversation profile, is unavailable or outside its pin, its
    /// profile cannot be proven, no offered endpoint is one the agent can mount, or the
    /// handshake or the selection fails; nothing falls back.
    pub async fn converse(
        &self,
        setup: ConversationSetup,
        offer: ToolOffer,
    ) -> Result<Conversation, HarnessError> {
        let profile = Profile::for_seat(&self.adapter.id).ok_or_else(|| {
            refusal(&format!(
                "ACP conversation has no audited profile for adapter `{}`; no fallback",
                self.adapter.id
            ))
        })?;
        self.probe_version().await?;
        let profile_env = match profile {
            Profile::Codex => {
                let (server, env) = law::codex_mount(&offer, law::choose(true, &offer)?);
                let mut proven = self
                    .codex_profile_env(Some((offer.name.as_str(), server)))
                    .await?;
                proven.extend(env);
                proven
            }
            Profile::ClaudeCode => Vec::new(),
        };
        let scratch = tempfile::tempdir().map_err(|_| HarnessError::Unavailable {
            reason: "ACP conversation cannot create its scratch directory".to_owned(),
        })?;
        let mut child = self.spawn_child(Some(scratch.path()), &profile_env)?;
        let unpiped = |pipe: &str| HarnessError::Session {
            reason: format!("the child's {pipe} was not piped"),
        };
        let stdout = child.stdout.take().ok_or_else(|| unpiped("stdout"))?;
        let stdin = child.stdin.take().ok_or_else(|| unpiped("stdin"))?;
        let cwd = scratch.path().to_path_buf();
        let kept = Kept {
            group: child.id(),
            _child: child,
            _scratch: scratch,
        };
        let opening = Opening {
            setup,
            offer,
            profile,
            cwd,
        };
        open(stdout, stdin, opening, kept).await
    }
}
