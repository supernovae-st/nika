// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The UI's owned turn thread, which polls the complete Session/Compiler/provider future.
//! Debug ACP authoring can exhaust the platform's ordinary 2 MiB worker stack without any
//! recursive turn. Reserve 8 MiB here; the input thread, Stop, authority and retry rules stay
//! unchanged. This is stack capacity for one turn, not a limit on work or a provider policy.

/// Headroom for nested authoring futures, including ACP version/session preparation.
const TURN_STACK_BYTES: usize = 8 * 1024 * 1024;

/// The one constructor used by the real shell and its stack regression test.
pub(super) fn turn() -> std::thread::Builder {
    std::thread::Builder::new()
        .name("nika-tui-turn".to_owned())
        .stack_size(TURN_STACK_BYTES)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    /// Keep more than 3 MiB live across ordinary nested calls. Black-boxing both before and
    /// after the child prevents tail-call or dead-frame elimination in optimized test builds.
    /// This models capacity, not Compiler recursion: the production crash had no repeated loop.
    #[inline(never)]
    #[allow(clippy::large_stack_arrays)] // Stack capacity is the regression's measured behavior.
    fn nested_work(depth: u8) -> usize {
        let frame = [depth; 48 * 1024];
        std::hint::black_box(&frame);
        let below = if depth == 0 {
            0
        } else {
            nested_work(depth - 1)
        };
        let kept = std::hint::black_box(&frame);
        below + usize::from(kept[0]) + usize::from(kept[kept.len() - 1])
    }

    #[test]
    fn turn_worker_returns_its_state_after_nested_work_and_a_following_turn() {
        // No provider, UI or environment access. Both turns use the exact production builder.
        // RUST_MIN_STACK=2097152 when running this test also makes a missing explicit reserve
        // observable even on developer machines that normally enlarge all Rust worker stacks.
        for line in ["first intention", "correction kept after stopping"] {
            let conversation = line.to_owned();
            let finished = super::turn()
                .spawn(move || {
                    assert_eq!(std::thread::current().name(), Some("nika-tui-turn"));
                    (conversation, nested_work(64))
                })
                .expect("turn starts")
                .join()
                .expect("nested work returns without a stack overflow");
            assert_eq!(
                finished.0, line,
                "the worker returns its owned conversation"
            );
            assert_eq!(finished.1, 2 * (64 * 65 / 2));
        }
    }
}
