- **The egress proxy no longer waits for stderr before it connects.** It
  wrote each journal line on the connection thread before dialing, so a
  run that held the stderr lock (`nika run --output json`, `nika test`)
  kept every CONNECT of a confined `exec` waiting until the run ended, and
  the child's client timed out (`curl: (28)` on any `permits.net.http`
  host). The line is now queued for the proxy's own journal thread and
  reaches stderr as soon as it is free.
