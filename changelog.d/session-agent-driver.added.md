- **The selected intelligence leads the Session's conversation.** With an API or a local route,
  a person's line now goes to one loop over that model and the Session's tools
  (`nika/author-tools@0`): it reads the project and the language, writes and edits the candidate,
  asks several questions together with their own identities, and proposes. Every value the
  candidate binds carries its provenance (named, delegated, derived, answered, offered or kept)
  checked against the person's own cited lines; a value they saw bound is never dropped
  silently; a reopen keeps the values and their provenance, never a question or a proposal.
  Saving and running stay the consent door's acts: the person's words open it only for the exact
  candidate they were shown, and a candidate whose effects changed waits for their consent.
  `NIKA_SESSION_DRIVER=rounds`, read when a host door opens the Session, keeps the round driver
  during the transition.
