- **A launchd unit given an env file passes its keys to the run.** `nika arm --emit launchd
  --env-file <file>` sourced the file through `/bin/sh` without exporting it: the plain
  `KEY=value` lines of a dotenv file never reached the firer, and every scheduled run that needs
  an API key failed. `nika arm fire --env-file <file>` now reads the file itself (plain
  `KEY=value` lines, an optional `export `, comments, blank lines, single or double quotes) and
  fires the beat with those variables set, never printing a value; a file it cannot read, or a
  line that is not `KEY=value`, refuses the fire before anything runs, naming the file and the
  line. The launchd unit hands `--env-file <file>` to the firer in its `ProgramArguments`, with
  no shell wrapper. Units written by an earlier engine are still recognized when they are
  re-emitted or retired.
