- **`nika explain NIKA-PARSE-001` names the causes the refusal names
  (#1608).** Its checklist sent a `prompt: Note: here` author looking for a
  missing `:` when the line holds one too many. It now leads with the causes
  the refusal itself names on the pointed line: a `: ` inside a plain value
  (quote the whole value, use a block scalar, or put a key of its own on its
  own line), an unquoted `${{ … }}` inside a flow mapping (quote it), and a
  `{` or `[` the file never closes; then a stray tab, unbalanced quotes, a
  key missing its `:` and a de-commented modeline.
