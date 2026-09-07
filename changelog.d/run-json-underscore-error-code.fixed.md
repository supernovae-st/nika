- **Run JSON preserves error codes containing underscores.** Failed runs
  retain codes such as `NIKA-BUILTIN-JSON_MERGE_PATCH-001` in `error.code`
  when they appear in the diagnostic, so consumers can classify the refusal.
