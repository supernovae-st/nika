// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hand-written mocks for all nika-kernel traits.
//!
//! Use as a dev-dependency for unit tests that need isolated side effects.
//! Each mock is designed for clarity and debuggability over auto-generation.
//!
//! Conformance test functions verify that mocks honor the same contracts
//! as production implementations.

pub mod clock;
pub mod filesystem;
pub mod http;
pub mod media;
pub mod policy;
pub mod record;
pub mod shell;
pub mod store;

pub use media::MockMediaContext;
pub use policy::MockPolicyChecker;
pub use record::MockRecordStore;
