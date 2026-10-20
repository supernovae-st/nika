- **A model picked in `/intelligence` reads by its name, never its key.** Picking a row
  of the intelligence list wrote the person's line as the row's internal key
  (`choice › 1 claude-code`). It now reads `Intelligence: claude-code`, the name the
  list showed, and a step to another list (`Other choice…`, a connection, `Esc` back)
  adds no line at all: the next list, or Nika's confirmation, says what happened.
  Words typed over the list and typed commands still echo exactly as typed.
