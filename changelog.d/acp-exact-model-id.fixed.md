- **An ACP session that advertises a qualified model id is selected by
  that exact id.** A request for `openai/gpt-5.5` looked only for
  `gpt-5.5`, so a session advertising `openai/gpt-5.5` verbatim was
  refused before any selection. The exact requested id is now matched
  first, the provider-less name only after; a near name is never taken
  for the requested one.
