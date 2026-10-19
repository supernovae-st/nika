- **A file the run removed no longer reads « produced ».** The run result listed every path the
  run was allowed to write as « produced », so the inbox files a run moved to `traites/` read
  « produced · ./inbox/… ». A path that is not there now now reads « removed » when a
  `nika:remove_file` step of the run took it away, « not there now » otherwise; the run's own
  completed writes still read « produced ».
