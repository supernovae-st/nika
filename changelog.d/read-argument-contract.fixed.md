- **Read argument errors no longer recover as missing files.** Check rejects
  literal nonstring `path` and nonboolean `binary` values, while whole-value
  bindings remain checked at runtime. Invalid arguments report
  `NIKA-INVOKE-002`; `NIKA-BUILTIN-READ-001` remains exclusive to missing
  files, preserving first-run recovery and the existing read permissions.
