- **Keep the terminal honest in bare `nika`.** A word wider than the composer (a
  path, a URL, a hash, a sentence without spaces) now wraps instead of being
  typed out of sight, and the composer grows by the rows it really paints;
  `SIGHUP` restores the terminal before leaving, as `SIGTERM` does; `--color` or
  `CLICOLOR_FORCE` under `NO_COLOR` shows its colours; and the inline view is
  cleared as it opens, so a partial line left under the cursor never shows
  inside it.
