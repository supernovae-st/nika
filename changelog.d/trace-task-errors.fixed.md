- **`trace outputs --json` now reports recorded task causes and terminal
  error codes/messages without borrowing evidence from an earlier task
  occurrence.** Its projection is version 2: recovery consumers must read
  `recovered_from` instead of the former `error_code` alias. A recovered
  success has no terminal error, and a new failure or running task no longer
  inherits recovered status.
