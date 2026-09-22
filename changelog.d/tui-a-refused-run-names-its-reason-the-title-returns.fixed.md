- **A run refused before its first frame names its reason inside the
  viewport; the terminal's title returns; `Tab` on an empty composer
  lists the commands.** In the renderer a run refused at check (a
  workflow with no `permits:` block) or before the start (`NIKA-1709`, a
  file that cannot be read) showed only « run observed · exit 2 »: the
  machine lane prints those refusals as the check verdict document or the
  error envelope, lines without a `kind` the story dropped. The story now
  folds them into one line (« ✖ refused before the start · [NIKA-AUTH-006]
  invoke `nika:read` … · 1 more finding(s) — `nika check` lists them »,
  « ✖ refused · NIKA-1709 · … »). The title (`nika · <project>`) was set
  and never restored: the previous title is pushed on the terminal's
  stack (`CSI 22;0 t`) before ours and popped (`CSI 23;0 t`) by the same
  restore the panic hook and every exit path run. `Tab` on an empty
  composer shows every command in the hint row and inserts nothing. The
  lane's story fold and the child driver gain unit tests (a `/bin/sh`
  child proves the fold, the exit code, the trace and the pid slot).
