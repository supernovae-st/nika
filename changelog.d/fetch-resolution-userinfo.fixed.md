- **Keep fetch authentication out of extracted document URLs.**
  Links, metadata and article extraction resolve against a landing URL without
  its username or password, including a response-supplied same-origin redirect.
  The request still carries its original authentication; derived outputs and
  traces no longer inherit those credentials from the resolution base.
