- **A `vision:` file is a read inside `permits.fs.read` (breaking).** The
  `infer:` verb opened every `vision: [{ source: file }]` path itself, on
  every route (API and ACP; `nika run`, serve, Session, child workflows,
  `nika test` and rehearsal rooms), with no `permits` check: an absent block,
  a narrower `fs.read` grant, a symlink or a `..` did not keep the file from
  the model provider, and a relative path was read from the process
  directory instead of the run's root. The run now reads each file through
  `nika:read` under the workflow's own boundary, on its resolved path,
  before any provider call: outside it the task fails `NIKA-SEC-004` (one
  `permit_checked` fs frame), and a granted file that cannot be read fails
  with the read's own code (`NIKA-BUILTIN-READ-001` when no file is there,
  `-002` otherwise) instead of the internal `NIKA-432`. `nika check` judges
  a literal or bare-`const` path (`NIKA-AUTH-006` with no `permits:` block,
  `NIKA-SEC-004` outside a declared one), and refuses a computed path, or a
  computed `images.save`, under a boundary whose side grants no path at all.
  `--infer-permits` prints the grant, the drift scan counts it as used, a
  child's vision reads join `NIKA-COMP-002`, and the check card no longer
  calls such a body pure compute. The `vision:` entries are sink fields of
  the task, on check and run alike: a secret in a vision URL or path is the
  `infer` leak (`NIKA-SEC-006` unless `egress: [{ to: "infer" }]`), and
  untrusted content that picks an entry reaches the answer, so the trifecta
  (`NIKA-SEC-009`) follows it as it follows a `nika:read` path; the file
  channel follows a vision read of a file an untrusted writer left. A
  `source: url` needs no grant: the provider fetches it. Migration: grant
  each vision file in `permits.fs.read` (`nika check --infer-permits` prints
  the block); embedders hand the verb bytes they read (`VisionPart::Loaded`),
  never a path.
