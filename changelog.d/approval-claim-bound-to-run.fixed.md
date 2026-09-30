- **An approval is single-use for the run, whichever HOME resumes it.**
  - A used approval was refused again only from the same HOME. Resuming the same paused run under
    another HOME ran the gated effect a second time.
  - The claim is now also written in the run's project (`.nika/approval-claims`), the project a
    resume is already bound to. A second use is refused from any HOME.
  - Limits: a run resumed with `--resume-unverified`, or one recorded before runs were bound to
    their project, can still be resumed from another project under another HOME. Its approval
    window (15 minutes) bounds that.
