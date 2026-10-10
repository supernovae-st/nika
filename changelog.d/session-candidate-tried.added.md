- **A conversation's candidate is tried before it is shown.** Each public
  GET source the candidate names is observed once for the conversation
  through the guarded observer (no credentials, no private address,
  bounded), and the observed room replays those pages to a run of the
  candidate where nothing leaves: a GET of exactly an observed address is
  answered with its page, every other request is refused. A step the trial
  cannot run (a model step, a request other than GET, an effect outside
  the room) is named, never run as itself. A failed trial, or a filter that
  keeps nothing of the items it read, goes back to the author with its
  facts before any proposal; a passed trial is bound to the proposal, which
  says what it ran on and what it did not run.
