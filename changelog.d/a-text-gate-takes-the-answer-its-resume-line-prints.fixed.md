- **A choice gate takes the answer its resume line prints.** A choice gate whose choices spell
  numbers (`choices: ["42", "17"]`) printed `--answer review=42` to resume, then refused that
  very line: `42` was read as a number, not one of its strings, and a shell could not pass the
  JSON string without a second quoting. An `input` or `choice` gate now takes the text typed
  (`'"42"'` still gives its text); a word outside the choices is still refused.
