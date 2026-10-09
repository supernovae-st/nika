- **`nika check --json` reports a parse refusal whole.** A workflow
  refused while parsing produced a JSON finding cut at its first line, so
  a diagnostic that quotes an authored value holding a line feed (or a
  carriage return and line feed) lost the rest of its message and its
  explain pointer. The finding is now rendered from the parser's typed
  error: its code and its whole message, never the source frame the
  human lane prints beneath it. The human output and the exit code are
  unchanged.
