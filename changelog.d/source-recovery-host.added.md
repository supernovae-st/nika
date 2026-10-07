- **Source recovery reachable from the product by explicit operator configuration.**
  `NIKA_AUTHORING_SOURCE_RECOVERY=<0..3>` (or a host's `AuthoringSettings::source_recovery`)
  is resolved by the shared authoring parser for the CLI and the Session (and its TUI): a count
  outside `0..=3`, or rounds under `off` or `only`, is refused. Serve stays unconfigured: its
  seat resolves only its named settings, never the environment, so its count is 0. The policy carries it on the same seat;
  the CLI receipt states `recovery_requests` and `worst_case_with_recovery` beside its allowance,
  and a Session unknown-cost review reserves them in the same account, stating the allowance and
  the theoretical worst case. Nothing changes when it is absent.
