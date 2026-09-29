- **Compose describes its Core check scope.** The agent tool checks an
  in-memory draft; `valid` does not resolve child files or admit execution.
  The saved draft and its children still require `nika check`. The canonical
  spec, embedded pack and tool description now state that boundary.
