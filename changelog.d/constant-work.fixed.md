- **A quoted text written to a named file compiles without a model.**
  « write 'hello' to ./a.txt » (and « écris « bonjour » dans ./a.txt »)
  used to ask for a language model to draft the text. `nika compile`
  now writes the quoted text exactly as stated through `nika:write`,
  with no question, no model and no provider call: punctuation,
  instructions, template-like text (`${{ … }}`), escaped quotes, an
  empty value, Unicode and a line break inside the quotes are written
  byte for byte, and a path inside the quoted text is neither read nor
  written. A write gated by an approval (« … once I approve », « ask me
  before writing 'hello' to ./a.txt ») waits for it alone and shows the
  exact text, to the path as spelled; a banned or contradicted write is
  still never written. Translating or rephrasing the text, an unquoted
  or ambiguous object, or a JSON/CSV destination still asks.
