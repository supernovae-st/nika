- **The key a provider reads is named, and the Nika-scoped variable is the one advised.**
  The probe lists the key variables set for each provider, names only, in the order the
  run reads them; the census and `nika doctor` name the one that supplies the key
  (`NIKA_ANTHROPIC_API_KEY` before `ANTHROPIC_API_KEY`), and two variables set with
  different values earn a doctor warning naming both, compared in memory where the keys
  are read and never printed. A missing key at run or in a conversation now advises
  `NIKA_<ID>_API_KEY`, which no other tool reads (exporting `ANTHROPIC_API_KEY` can move
  another application, such as Claude Code, onto that key), and a conversation shows it
  without the command-line teaching. An API pick naming the `claude` alias
  (`/intelligence 2 claude/<model>`) selects Anthropic.
