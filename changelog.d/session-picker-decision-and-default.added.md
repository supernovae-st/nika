- **A conversation can switch its decision seat and save its default.**
  `choose_decision` makes Jev (only the seat the host named) or the author
  alone consult the next turn's finite choices; the author alone clears a
  refusal the earlier seat caused, and an unavailable Jev is refused with
  its reason. `save_operator_default` writes the kind, model, decision seat
  by name and time to `~/.nika/session-intelligence.json` atomically, never
  a key; a default saved deciding alone opens new conversations that way.
