- **The model question no longer demands `<provider>/<model>`.** On a subscription account (codex,
  claude-code), a request with an AI step asked « Which explicit runtime provider/model should run
  the language steps? » with nothing offered: « choisis toi-même » was taken as typed, and the
  compiler answered « Name the runtime model as <provider>/<model> ». The question now offers the
  configured model this conversation recommends (the saved default, else the newest configured
  one), which Enter takes. A reply that leaves the choice to Nika takes it too, said as Nika's
  choice beside the proposal, and a model named or described in words (« prends Claude Haiku ») is
  read against the configured models whose name carries those words, never invented. A reply that
  designates none of them binds nothing and says which model is offered.
