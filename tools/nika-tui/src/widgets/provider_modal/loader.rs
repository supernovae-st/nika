// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Async loader for provider modal data
//!
//! Orchestrates loading of provider statuses and native models
//! in the background, communicating results via channels.
//!

use std::sync::Arc;
use tokio::sync::mpsc;

use super::provider_checker::ProviderChecker;
use super::state::{ConnectionStatus, NativeModelInfo};

/// Events sent from loader to UI
#[derive(Debug, Clone)]
pub enum LoaderEvent {
    /// Provider status update
    ProviderStatus {
        provider: &'static str,
        status: ConnectionStatus,
    },
    /// All providers checked
    ProvidersComplete,
    /// Native inference availability check
    NativeAvailable(bool),
    /// Native models loaded
    NativeModels(Vec<NativeModelInfo>),
    /// Loading error
    Error { source: String, message: String },
}

/// Commands sent from UI to loader
#[derive(Debug, Clone)]
pub enum LoaderCommand {
    /// Check all provider statuses
    CheckProviders,
    /// Load native models
    LoadNativeModels,
    /// Check native inference availability
    CheckNative,
    /// Stop the loader
    Stop,
}

/// Async loader for provider modal
pub struct ModalLoader {
    /// Command sender (UI → Loader)
    cmd_tx: mpsc::Sender<LoaderCommand>,
    /// Event receiver (Loader → UI)
    event_rx: mpsc::Receiver<LoaderEvent>,
    /// Handle to the loader task
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl ModalLoader {
    /// Create a new loader and start the background task
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<LoaderCommand>(16);
        let (event_tx, event_rx) = mpsc::channel::<LoaderEvent>(32);

        let handle = tokio::spawn(Self::run_loop(cmd_rx, event_tx));

        Self {
            cmd_tx,
            event_rx,
            handle: Some(handle),
        }
    }

    /// Send a command to the loader
    pub async fn send(
        &self,
        cmd: LoaderCommand,
    ) -> Result<(), mpsc::error::SendError<LoaderCommand>> {
        self.cmd_tx.send(cmd).await
    }

    /// Try to send a command without blocking
    pub fn try_send(
        &self,
        cmd: LoaderCommand,
    ) -> Result<(), mpsc::error::TrySendError<LoaderCommand>> {
        self.cmd_tx.try_send(cmd)
    }

    /// Try to receive an event without blocking
    pub fn try_recv(&mut self) -> Option<LoaderEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Check if there are pending events
    pub fn has_events(&self) -> bool {
        !self.event_rx.is_empty()
    }

    /// Stop the loader
    pub async fn stop(&mut self) {
        let _ = self.cmd_tx.send(LoaderCommand::Stop).await;
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }

    /// Background loop processing commands
    async fn run_loop(
        mut cmd_rx: mpsc::Receiver<LoaderCommand>,
        event_tx: mpsc::Sender<LoaderEvent>,
    ) {
        let checker = Arc::new(ProviderChecker::new());

        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                LoaderCommand::CheckProviders => {
                    let checker = Arc::clone(&checker);
                    let tx = event_tx.clone();

                    // Spawn provider checks
                    tokio::spawn(async move {
                        let results = checker.check_all().await;
                        for (provider, status) in results {
                            let _ = tx
                                .send(LoaderEvent::ProviderStatus { provider, status })
                                .await;
                        }
                        let _ = tx.send(LoaderEvent::ProvidersComplete).await;
                    });
                }
                LoaderCommand::CheckNative => {
                    let tx = event_tx.clone();

                    // Native inference via mistral.rs
                    // Check if native-inference feature is enabled
                    tokio::spawn(async move {
                        #[cfg(feature = "native-inference")]
                        let available = true;
                        #[cfg(not(feature = "native-inference"))]
                        let available = false;

                        let _ = tx.send(LoaderEvent::NativeAvailable(available)).await;
                    });
                }
                LoaderCommand::LoadNativeModels => {
                    let tx = event_tx.clone();

                    // Native models are loaded from HuggingFace cache
                    // For now, return empty list - actual model discovery happens via nika model list
                    tokio::spawn(async move {
                        // TODO: Implement native model discovery from HuggingFace cache
                        let models: Vec<NativeModelInfo> = Vec::new();
                        let _ = tx.send(LoaderEvent::NativeModels(models)).await;
                    });
                }
                LoaderCommand::Stop => {
                    break;
                }
            }
        }
    }
}

impl Default for ModalLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ModalLoader {
    fn drop(&mut self) {
        // Send stop command if handle exists
        if self.handle.is_some() {
            // Use runtime spawn to ensure Stop is delivered
            // try_send can fail silently if the channel buffer is full
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                let tx = self.cmd_tx.clone();
                rt.spawn(async move {
                    let _ = tx.send(LoaderCommand::Stop).await;
                });
            } else {
                // No runtime available - try_send is best effort
                let _ = self.cmd_tx.try_send(LoaderCommand::Stop);
            }
        }
    }
}

/// Simple state tracker for loading operations
#[derive(Debug, Clone, Default)]
pub struct LoadingState {
    /// Providers are being checked
    pub checking_providers: bool,
    /// Native models are being loaded
    pub loading_models: bool,
    /// Provider statuses by name
    pub provider_statuses: Vec<(&'static str, ConnectionStatus)>,
    /// Native inference availability
    pub native_available: Option<bool>,
    /// Loaded native models
    pub native_models: Vec<NativeModelInfo>,
}

impl LoadingState {
    /// Create new loading state
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a loader event and update state
    pub fn process_event(&mut self, event: LoaderEvent) {
        match event {
            LoaderEvent::ProviderStatus { provider, status } => {
                // Update or add provider status
                if let Some(pos) = self
                    .provider_statuses
                    .iter()
                    .position(|(p, _)| *p == provider)
                {
                    self.provider_statuses[pos] = (provider, status);
                } else {
                    self.provider_statuses.push((provider, status));
                }
            }
            LoaderEvent::ProvidersComplete => {
                self.checking_providers = false;
            }
            LoaderEvent::NativeAvailable(available) => {
                self.native_available = Some(available);
            }
            LoaderEvent::NativeModels(models) => {
                self.native_models = models;
                self.loading_models = false;
            }
            LoaderEvent::Error { .. } => {
                // Could track errors here if needed
                self.loading_models = false;
            }
        }
    }

    /// Check if any loading is in progress
    pub fn is_loading(&self) -> bool {
        self.checking_providers || self.loading_models
    }

    /// Get provider status by name
    pub fn get_provider_status(&self, provider: &str) -> Option<&ConnectionStatus> {
        self.provider_statuses
            .iter()
            .find(|(p, _)| *p == provider)
            .map(|(_, s)| s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loading_state_default() {
        let state = LoadingState::new();
        assert!(!state.checking_providers);
        assert!(!state.loading_models);
        assert!(state.provider_statuses.is_empty());
        assert!(state.native_available.is_none());
    }

    #[test]
    fn test_loading_state_is_loading() {
        let mut state = LoadingState::new();
        assert!(!state.is_loading());

        state.checking_providers = true;
        assert!(state.is_loading());

        state.checking_providers = false;
        state.loading_models = true;
        assert!(state.is_loading());
    }

    #[test]
    fn test_loading_state_process_provider_status() {
        let mut state = LoadingState::new();
        state.process_event(LoaderEvent::ProviderStatus {
            provider: "anthropic",
            status: ConnectionStatus::Connected { latency_ms: 100 },
        });

        assert_eq!(state.provider_statuses.len(), 1);
        assert!(matches!(
            state.get_provider_status("anthropic"),
            Some(ConnectionStatus::Connected { latency_ms: 100 })
        ));
    }

    #[test]
    fn test_loading_state_process_providers_complete() {
        let mut state = LoadingState::new();
        state.checking_providers = true;
        state.process_event(LoaderEvent::ProvidersComplete);
        assert!(!state.checking_providers);
    }

    #[test]
    fn test_loading_state_process_native_available() {
        let mut state = LoadingState::new();
        state.process_event(LoaderEvent::NativeAvailable(true));
        assert_eq!(state.native_available, Some(true));
    }

    #[test]
    fn test_loading_state_process_native_models() {
        use super::super::state::NativeModelDetails;

        let mut state = LoadingState::new();
        state.loading_models = true;

        let models = vec![NativeModelInfo {
            name: "llama3.2".to_string(),
            size: 4_700_000_000,
            digest: "sha256:abc".to_string(),
            modified_at: "2026-02-24".to_string(),
            details: NativeModelDetails {
                parameter_size: "8B".to_string(),
                quantization_level: "Q4_0".to_string(),
                family: Some("llama".to_string()),
            },
        }];

        state.process_event(LoaderEvent::NativeModels(models));
        assert!(!state.loading_models);
        assert_eq!(state.native_models.len(), 1);
    }

    #[test]
    fn test_loading_state_update_existing_provider() {
        let mut state = LoadingState::new();

        // Add initial status
        state.process_event(LoaderEvent::ProviderStatus {
            provider: "openai",
            status: ConnectionStatus::Checking,
        });

        // Update to connected
        state.process_event(LoaderEvent::ProviderStatus {
            provider: "openai",
            status: ConnectionStatus::Connected { latency_ms: 50 },
        });

        // Should have only one entry, updated
        assert_eq!(state.provider_statuses.len(), 1);
        assert!(matches!(
            state.get_provider_status("openai"),
            Some(ConnectionStatus::Connected { latency_ms: 50 })
        ));
    }

    #[test]
    fn test_loading_state_get_unknown_provider() {
        let state = LoadingState::new();
        assert!(state.get_provider_status("unknown").is_none());
    }

    #[tokio::test]
    async fn test_modal_loader_creation() {
        let mut loader = ModalLoader::new();
        // Should not panic
        loader.stop().await;
    }

    #[tokio::test]
    async fn test_modal_loader_send_stop() {
        let mut loader = ModalLoader::new();
        let result = loader.send(LoaderCommand::Stop).await;
        assert!(result.is_ok());
        loader.stop().await;
    }

    #[tokio::test]
    async fn test_modal_loader_try_recv_empty() {
        let mut loader = ModalLoader::new();
        // Should return None when no events
        let event = loader.try_recv();
        assert!(event.is_none());
        loader.stop().await;
    }

    #[tokio::test]
    async fn test_modal_loader_check_providers() {
        let mut loader = ModalLoader::new();

        // Send check command
        let _ = loader.send(LoaderCommand::CheckProviders).await;

        // Give some time for processing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Should have some events (provider statuses)
        // Note: May or may not have events depending on timing
        loader.stop().await;
    }
}
