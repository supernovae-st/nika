- **A `claude-code` call no longer runs with the person's own Claude Code setup.** `--tools ""`
  empties only Claude Code's built-in tools: every MCP server of the person's configuration
  (plugins and claude.ai connectors included), their hooks, plugins and personal CLAUDE.md still
  loaded into each authoring, conversation and decision call through `claude -p`. Measured on
  2.1.283: 180 MCP tools and about 115,000 input tokens for a one-word answer, and an authoring
  call that ran a web-search MCP tool, then stopped at its one-turn limit, which the TUI showed as
  « I couldn't use the authoring model for this part ». The call now also passes
  `--strict-mcp-config` and an empty `--setting-sources`, the isolation the ACP completion profile
  already has: a model or effort set in the person's Claude Code settings no longer applies to
  these calls either, and the CLI's own default for the account answers, as on the ACP route.
