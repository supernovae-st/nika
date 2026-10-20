- **A Session host answers one question by its identity.** Both Session doors take `answer`
  (`command`, `snapshot`, `question`, `line`) and `delegate` (`command`, `snapshot`,
  `question`) and advertise `sessionQuestion`: a client that read several questions asked
  together, or the choices a proposal shows, binds its reply to the one it names and the others
  keep waiting, where a plain line answers every question whose offers it names. The host takes
  the identity only among those the current snapshot asks (`unknown_question` otherwise, with
  nothing sent), and `delegate` leaves that choice to Nika as the person's act, never as words.
