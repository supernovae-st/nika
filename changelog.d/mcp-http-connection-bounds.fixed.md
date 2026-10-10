- **`nika mcp --http` holds every connection to a deadline and serves them side by side.**
  A request has 30 s in all to arrive, head and body, and its response 30 s to leave; each
  connection runs on a thread of its own, at most eight at once, the next waiting in the
  listener's backlog, so a slow client holds one connection for a bounded time. A failed accept
  on the server's side pauses the loop briefly.
