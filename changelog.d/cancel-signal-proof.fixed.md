- **Unix cancellation keeps its signal subscriptions for the whole run.**
  SIGINT/SIGTERM are registered before dispatch and retained between the
  graceful cancel and immediate abort, closing both subscription windows.
  CLI signal and cross-door settlement tests hold a task on a controlled
  FIFO and wait for cancellation acknowledgement before releasing it, with
  exact task counts and sealed-trace checks instead of timing sleeps.
