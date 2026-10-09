- **A harness authoring call's record names the last protocol step it
  closed.** Beside its phase, a terminal ACP authoring record now says
  which step this side last completed: the stream opened, the agent's
  initialize accepted, its session created, the requested selection
  checked, the prompt written, or an answer chunk received. A call that
  stalls is localized to the step after it, without any adapter text; it
  says nothing about what the agent's model did, its other activity or
  how long anything generated, and the deadline is unchanged.
