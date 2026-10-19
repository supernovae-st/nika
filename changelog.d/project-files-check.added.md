- **`prompt:`, `system:` and `schema:` can name a project file, and
  `nika check` judges it.** `{ file: <path> }` resolves from the folder of
  the workflow that names it, stays inside the root workflow's folder, and
  is read only inside that workflow's `permits.fs.read` boundary, never
  through a symbolic link: a broken path law is `NIKA-FILE-001`, a missing
  file `NIKA-FILE-002`, more than 65,536 bytes or bytes that are not UTF-8
  `NIKA-FILE-003`, a schema file holding no JSON Schema `NIKA-TYPE-007`.
  The text or schema read then meets every law the same field written
  inline meets, in a composed child too, where each finding names the
  child, task, field and path. A door without the files names them as
  unread (`judged.project_files: false`). No run carries project files
  yet: `nika run`, `nika test` and a captured world refuse such a workflow
  before any task, and the runtime refuses a file no compose step read,
  at launch and at the `infer:` or `agent:` dispatch.
