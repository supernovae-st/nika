- **`nika check` no longer calls the members of a folded group dead spend.**
  A task that declares `group: <name>` is read by every `${{ group.<name> }}`
  fold, but the `[dead-spend]` hint only counted `tasks.<id>.output`
  references and told the author to remove each member. The hint now reads
  group membership with the same law the DAG uses to fold.
