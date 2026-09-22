- **Bare `nika` opens on the human's question, and asks for an intelligence
  only when a turn needs one.** The session no longer stops at a first
  screen before any value: it opens on « What do you want to automate? »,
  the engine's facts and the deterministic compiler answer with no choice
  made, and the first line only an intelligence can answer (a conversation,
  or work the reader cannot settle alone) asks the first screen in context,
  names why, keeps that line and resumes it exactly as typed once the choice
  is made (`TurnOutcome::Resumed`); a typo keeps the request waiting and
  `cancel` continues without a choice. The banner's engine facts (root,
  intelligence and where the context goes, authoring seat) move to
  `/status`; an explicit choice this machine cannot serve stays the one
  warning on the banner. Same law in the plain loop and behind `nika --tui`;
  a pipe keeps the concierge.
