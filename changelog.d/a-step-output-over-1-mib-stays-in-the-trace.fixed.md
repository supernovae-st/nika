- **A step whose output is over 1 MiB no longer ends its run's trace.** A fetched 454,763-byte
  JSON page made a 1,180,930-byte `task_completed` frame: its `output` and its `outcome` each
  carry the value, the journal refused the line (« journal line is 1180930 bytes; maximum is
  1048576 ») and the trace stopped there, so the run could not be resumed. A frame over the
  line bound now moves its largest values, one at a time, to `<trace>.values/<sha256>` beside
  the journal until it fits, each field keeping the value's digest, and `spilled_fields` names
  them. The chain commits to each digest, `--resume`, `nika trace show|replay` and the debugger
  read the values back only when their bytes match it, and `nika trace verify` refuses a value
  that is missing or altered. The line bound stays; trace retention removes the values with
  their journal and counts them in its budget.
