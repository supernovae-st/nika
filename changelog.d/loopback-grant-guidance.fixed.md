- **Check names the exact loopback grant a run accepts.** A task that
  fetches an exact loopback host (`127.0.0.1`, `localhost`, `::1`) was
  told no permit could admit it, although the run admits that host once
  its exact literal is declared in `permits.net.http`. The finding now
  says so and offers that one grant as its fix; a wildcard or another
  loopback host still does not grant it, and private, link-local and
  metadata targets keep the floor's refusal with no grant fix.
