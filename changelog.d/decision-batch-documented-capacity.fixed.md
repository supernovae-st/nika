- **A System One decision seat no longer starts a batch with a request known to fail.** A
  seat of `typesafe/jev-1.13.0` plans each batch inside the capacity the service documents (64k
  tokens per request), splitting a request in halves before it leaves when its exact body would
  exceed it, and never again sends a body as large as one the service refused for capacity. The
  Foundry qualification of a large knowledge recall used to send the whole batch first and only
  shrink after `max_tokens_exceeded` refusals. Other decision models keep their earlier
  behaviour.
