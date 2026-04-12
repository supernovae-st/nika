// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! BindingStore trait — re-exported from nika-core.
//!
//! The trait itself lives in `nika-core::binding::store` as of S23 so
//! that `binding/resolve.rs` (which consumes it) can live in nika-core
//! without a circular dependency on nika-kernel. This module stays for
//! backwards compatibility with code that imports
//! `nika_kernel::binding::{BindingStore, BindingStoreError}`.

pub use nika_core::binding::{BindingStore, BindingStoreError};
