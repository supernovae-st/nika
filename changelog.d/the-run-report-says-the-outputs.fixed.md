- **The run report says the workflow's outputs.** A run's declared `outputs:` (the totals of
  an aggregate, the message of a translation report) appeared only in a side panel's JSON; the
  report now lists each one (« output · totals · {"north":15,"south":20} »), a long value cut
  at 160 characters. A read whose task failed and was recovered, such as a state file that does
  not exist yet on a first run, reads « not there · ./state.txt · nothing was read » instead of
  « read ».
