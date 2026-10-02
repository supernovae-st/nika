- **An `outputs:` entry that reads a secret itself is refused, even with
  `egress: [{ to: outputs }]`.** `nika check` accepted such an entry
  although the export always masked the value (`***`), so the sanction
  promised a flow the engine never delivers. The engine never writes a
  secret's own value on any surface, the outputs export included, so
  `to: "outputs"` declassifies values derived from a secret, such as the
  response of an authenticated call. A direct `${{ secrets.<name> }}` in
  `outputs:` is now refused with `NIKA-SEC-007`, and its message names the
  removal as the repair.
