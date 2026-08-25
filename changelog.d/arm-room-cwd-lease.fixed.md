- **The arm-room test reads the cwd under the lease that governs it.**
  `concurrent_run_rooms_are_serialized_and_restore_the_caller` observed
  the process-global cwd twice — the baseline and the restoration —
  without holding `cwd::hold()`, so it measured whichever sibling
  happened to own the process. It went red on three consecutive `main`
  commits with `left` = `cwd::tests`'s chdir-storm room, while this
  test's own restore had already put the process back. The production
  path was never at fault: `enter_room` takes the crate lease and rides
  `fchdir` exactly as `cwd.rs` documents. Both reads now ride the lease,
  which is what makes the claim well-defined. Reproduced 2/20 by running
  the test WITH the storm; 40/40 after (the full suite hides it — 12/12
  green either way, which is why the red only ever surfaced in CI).
