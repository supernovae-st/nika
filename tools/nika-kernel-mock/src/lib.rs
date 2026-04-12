// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hand-written mocks for all nika-kernel traits.
//!
//! Use as a dev-dependency for unit tests that need isolated side effects.
//! Each mock is designed for clarity and debuggability over auto-generation.
//!
//! Conformance test functions verify that mocks honor the same contracts
//! as production implementations.

pub mod builtin;
pub mod clock;
pub mod filesystem;
pub mod http;
pub mod mcp;
pub mod media;
pub mod policy;
pub mod provider;
pub mod record;
pub mod shell;
pub mod store;

pub use builtin::MockBuiltinRouter;
pub use mcp::MockMcpPool;
pub use media::MockMediaContext;
pub use policy::MockPolicyChecker;
pub use provider::MockProvider;
pub use record::MockRecordStore;

// S24: `MockBindingStore` moved to `nika_core::binding::mock` behind the
// `mock-bindings` feature. The old `pub use binding::MockBindingStore`
// re-export lived here because kernel-mock historically dev-depended on
// nika-core — that triangle blocked moving binding/resolve.rs tests into
// nika-core. Deleting the module + its nika-core dep breaks the triangle.
