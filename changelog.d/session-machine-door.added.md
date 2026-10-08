- **`nika session --json`: the Session for programs.** The same native
  Session as bare `nika` in this directory, spoken as one JSON object
  per line on stdio: a client submits a line against the snapshot it
  answers, reads the proposal and the questions, consents to Save and
  asks for a Run separately, and closes by ending its input. The
  engine identity lists the `sessionHost` capability.
