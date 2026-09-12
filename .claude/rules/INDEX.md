# Contributor guidance routes

`AGENTS.md` owns the common contract. Use a rule when the task needs it:

| Rule | When relevant |
|---|---|
| [diamond-discipline.md](diamond-discipline.md) | Crate admission, legacy reference or architecture gates |
| [nika-invariants.md](nika-invariants.md) | Crate/layer changes or ADRs |
| [commit-granularity.md](commit-granularity.md) | Preparing a commit |
| [session-discipline.md](session-discipline.md) | Ownership, interruption, continuation or completion |
| [evolution.md](evolution.md) | Maintaining canonical documentation |

Do not load every rule as an entry ritual. Some clients automatically load
rule files: this index alone does not control host loading. User instructions
and host boundaries take precedence; optional private notes are context, not
a replacement for the public contract.
