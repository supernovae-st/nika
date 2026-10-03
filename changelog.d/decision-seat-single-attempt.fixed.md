- **A `typesafe/<jev>` decision seat sends each question at most once.** Its client
  no longer replays a request refused at the HTTP/2 protocol level, and a client
  that could retry is refused before sending. The receipt distinguishes this
  transport from a `provider/name` seat, which keeps its client’s protocol retries.
  An uncertain delivery may still be billed; this is not a price guarantee.
