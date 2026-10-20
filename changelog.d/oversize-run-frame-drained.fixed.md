- **A task with a large output no longer stops the run.** The Session reads a run's frames line by
  line and refused any frame over 1 MiB, ending the child mid-run: a workflow that fetched three
  job boards (one of them 3.4 MB of JSON) failed with « Run cost review refused: Run frame exceeds
  1 MiB; no automatic retry », its effects uncertain. A frame over the cap is now drained without
  being kept and the run goes on to its own end; memory stays bounded. The Session says the step's
  output was neither shown nor kept: the run's trace bounds its lines at the same 1 MiB, so its
  journal stops at that step and the run settles with its evidence lost.
