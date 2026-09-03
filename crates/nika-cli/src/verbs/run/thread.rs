// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The interactive thread's narrow bridge into the production run pipeline.

use crate::verbs::exit;

use super::RunVerdict;

pub(super) fn block_on_run<F>(
    runtime: &tokio::runtime::Runtime,
    future: F,
    interruptible: bool,
) -> RunVerdict
where
    F: std::future::Future<Output = RunVerdict>,
{
    if !interruptible {
        return runtime.block_on(future);
    }
    runtime.block_on(async {
        tokio::select! {
            verdict = future => verdict,
            signal = tokio::signal::ctrl_c() => match signal {
                Ok(()) => RunVerdict::interrupted(),
                Err(error) => {
                    eprintln!("nika: cannot listen for interrupt: {error}");
                    RunVerdict::bare(exit::ENV)
                }
            },
        }
    })
}
