- **An authoring answer is refused when the AI app moved the chosen
  model or effort during the turn.** Over ACP, an agent may switch the
  model or the reasoning effort mid-turn (a rate-limit fallback, for
  instance). Workflow runs already refused such an answer; authoring now
  does too, naming the move, while a move of a dimension nobody chose is
  kept in the call's record.
