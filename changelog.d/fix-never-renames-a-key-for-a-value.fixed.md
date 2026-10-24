- **`nika check --fix` no longer renames an argument when its value is the mistake.** A literal
  outside a builtin's closed set (`nika:inspect` `view: record`) carried its nearest member as the
  key-rename suggestion, so `--fix` rewrote the file to `records: record`. The nearest member is now
  said in the refusal (« did you mean `records`? ») and `--fix` leaves the value for you to change.
