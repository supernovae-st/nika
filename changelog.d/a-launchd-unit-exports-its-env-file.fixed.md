- **A launchd unit given an env file passes its keys to the run.** `nika arm --emit launchd
  --env-file <file>` sourced the file without exporting it. The plain `KEY=value` lines of a
  dotenv file never reached the firer, and every scheduled run that needs an API key failed. The
  unit now exports what the file assigns (`set -a`). Units written by an earlier engine are still
  recognized when they are re-emitted or retired.
