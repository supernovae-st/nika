- **The Linux jail no longer drops a write grant without a word.** bwrap
  can only bind a path that exists, and a jailed command cannot create its
  own grant. The jail used `--bind-try`, which skipped such a grant
  silently, so a command that had to create the granted file failed far
  from the cause (on Linux only: macOS Seatbelt admits a file that does
  not exist yet). Existing write grants now bind with `--bind` (a source
  that vanishes before the mount fails loudly), and a spawn that leaves
  grants unbound writes one line on stderr naming them with the remedy:
  grant the directory, and make sure it exists before an exec writes into
  it. An absent grant is not refused, since another task of the workflow
  may be the one that creates it.
