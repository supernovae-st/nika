- **`nika guard` no longer refuses plain shell it can read.** Loops,
  `if`, `[ … ]`, `find` without `-exec`, `xargs` with any other command,
  a heredoc script such as `python3 - <<'EOF'` and a command word in a
  variable that cannot make a run (`"$EDITOR" notes.txt`) were refused
  as `guard_unavailable`, blocking agent commands that run no workflow.
  The guard now judges the commands inside them and keeps no opinion
  when none is a `nika run`; `find -exec nika`, `xargs nika` and
  `$N run x` stay visible.
