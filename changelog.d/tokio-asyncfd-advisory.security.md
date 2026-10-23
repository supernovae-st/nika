- **The async runtime is past its October soundness advisory.** `tokio` 1.53.3 makes `AsyncFd::new`
  and its related constructors respect I/O safety again (RUSTSEC-2026-0386), and the yanked
  `chacha20` 0.10.1 is replaced by 0.10.2, in the workspace, fuzz and ACP lockfiles.
