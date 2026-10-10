- **A second Ctrl-C no longer leaves exec children running.** The second
  signal ended `nika run` with `exit(130)`, which runs no destructor, while
  every `exec` child leads a process group of its own that neither the
  signal to the CLI nor a terminal's Ctrl-C reaches: the child kept running
  and writing after the CLI was gone. The abort now ends every process group
  the run still owns first (SIGTERM, a one-second grace, SIGKILL, then a
  bounded reap), never signals a group whose leader the process can no
  longer wait for, and says on stderr how many groups ended and which did
  not confirm their end. Nothing is rolled back: what already ran stays
  done, and an effect in flight has an unknown outcome.
