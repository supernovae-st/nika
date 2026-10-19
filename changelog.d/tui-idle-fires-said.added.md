- **A fire while the workspace sits open shows without a keystroke.** About once a minute, while
  no turn or run is busy, the TUI reads the Session's schedule history again (a read only) and says
  a schedule's new fires as it says them after a turn: once, with the rail. Before, a fire while
  the TUI stood open stayed unsaid until the person typed something. `Options::idle_every` sets
  the period (`None` never asks); a conversation answers through `Conversation::idle`.
