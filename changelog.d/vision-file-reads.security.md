- **A `vision:` file is a read inside `permits.fs.read` (breaking).** The
  `infer:` verb opened every `vision: [{ source: file }]` path itself, on
  every route (API and ACP; `nika run`, serve, Session, child workflows,
  `nika test` and rehearsal rooms), with no `permits` check: an absent block,
  a narrower `fs.read` grant, a symlink or a `..` did not keep the file from
  the model provider, and a relative path was read from the process
  directory instead of the run's root. The run now reads each file through
  `nika:read` under the workflow's own boundary, on its resolved path,
  before any provider call: outside it the task fails `NIKA-SEC-004` (one
  `permit_checked` fs frame), a missing file stays `NIKA-432`. `nika check`
  judges a literal or bare-`const` path (`NIKA-AUTH-006` with no `permits:`
  block, `NIKA-SEC-004` outside a declared one), `--infer-permits` prints the
  grant, the drift scan counts it as used, a child's vision reads join
  `NIKA-COMP-002`, and the trifecta file channel follows a vision read of a
  file an untrusted writer left. A `source: url` is unchanged: the provider
  fetches it. Migration: grant each vision file in `permits.fs.read`
  (`nika check --infer-permits` prints the block); embedders hand the verb
  bytes they read (`VisionPart::Loaded`), never a path. `SPEC_PIN` and the
  embedded pack follow nika-spec#400, which also names `images.save` and its
  `-2` to `-4` siblings in the same-path law (`NIKA-SEC-012`).
