- **A misconfigured `nika:prompt` fails at once instead of pausing forever.** An unknown
  `mode:` (`select`), a missing `message:` or a `default:` the mode cannot use was coded
  `NIKA-BUILTIN-PROMPT-001`, the failure the runtime turns into a durable pause: an
  unattended run whose author had declared a default waited for a human, and resuming
  an unknown mode with an answer paused it again. These are now
  `NIKA-BUILTIN-PROMPT-002`, judged before anyone is asked, with the modes or the
  expected type named. A confirm `default:` written as a person answers a confirm
  (`yes`, `y`, `no`, `n`, `"true"`, `"false"`) now reads as that answer, as `--answer`
  already did; YAML 1.2 reads an unquoted `yes` as a string. Only a prompt with neither
  a human nor a `default:` keeps `PROMPT-001` and pauses.
