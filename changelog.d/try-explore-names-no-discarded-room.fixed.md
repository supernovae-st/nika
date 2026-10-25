- **`nika try` no longer hands over a command for a file that is gone.** After a rehearsal, the run's
  closing line said « explore: nika run /var/folders/…/nika-try-01-hello/01-hello.nika --json > … »,
  but the rehearsal room is deleted when `try` returns, so that command could never run. A try
  rehearsal now ends on its own line, which names the example and `nika compile --list`; workspace
  runs keep their explore line.
