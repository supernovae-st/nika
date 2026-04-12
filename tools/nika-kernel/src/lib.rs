// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika Kernel — trait definitions for all side effects.
//!
//! This crate sits at L0.5 in the dependency graph, between nika-core (L0)
//! and the implementation crates (L1+). It contains:
//!
//! - 10 trait definitions abstracting every side effect
//! - Provider DTOs (InferRequest, InferResponse, InferEvent)
//! - TaskScope splinter traits for compile-time capability enforcement
//! - Bundle structs for ergonomic composition
//!
//! **Zero implementations live here.** This crate defines contracts only.
//! Implementations live in their respective crates (nika-provider, nika-http, etc.)

pub mod binding;
pub mod builtin;
pub mod caps;
pub mod clock;
pub mod events;
pub mod filesystem;
pub mod http;
pub mod mcp;
pub mod policy;
pub mod provider;
pub mod scope;
pub mod shell;
pub mod store;
pub mod task_local;
