- **« Corrige ça » after a failed run now fixes that failure.** A revision asked right after a run
  of the same workflow failed was sent to the author without the run: « No diagnostic was supplied
  with « corrige ça » », so it guessed. In the live gauntlet a fetch refused after a redirect
  (`docs.claude.com` → `platform.claude.com`, NIKA-SEC-004) was « fixed » by removing an unrelated
  `max_tokens` and adding fallbacks, the estimate became unbounded and the next run failed the same
  way. The revision now carries that run (`observed_world.last_run`: the exit and each failed task
  with the error its trace recorded) and the author is told to fix what it reports and touch
  nothing else. Live: the author granted the redirected host for GET and the next run wrote the
  digest. A succeeded or paused run, or a run of another workflow, adds nothing.
