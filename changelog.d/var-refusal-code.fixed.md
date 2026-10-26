- **A `--var` refusal names its code in the `--json` envelope.** An unknown
  input, a value its declared type refuses and a malformed pair reached
  `nika run --json` callers as `{"error":{"code":null,…}}`, while the
  `--inputs-json` channel and the server say `unknown_input` and
  `input_type_mismatch`. The envelope now carries `unknown_input`,
  `input_type_mismatch`, `malformed`, `env_unset`, `env_undeclared_in_ci` or
  `env_name_missing`; the sentence on stderr and exit 3 are unchanged.
