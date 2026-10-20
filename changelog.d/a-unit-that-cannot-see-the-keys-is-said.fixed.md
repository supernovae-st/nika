- **The trigger install says when a scheduled run would not see the API keys.** A unit started by
  launchd does not inherit the terminal's environment, so a workflow whose model reads its key
  there (`NIKA_ANTHROPIC_API_KEY`, `ANTHROPIC_API_KEY`…) would fail at every scheduled run.
  « Installer le déclencheur sur ce Mac ? » now names those keys, says the run would fail until
  the project's units load a file that holds them (`arm --emit --env-file`), and recommends
  keeping the schedule declared. Workflows with no model step, with a mock or a local model, or
  whose units already load an environment file naming their keys keep the card as before.
