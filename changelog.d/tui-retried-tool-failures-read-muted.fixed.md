- **A tool failure later work recovered no longer reads as a red failure.** The activity card kept
  every failed tool step as a red « ✗ … failed », even when the next attempt of the same tool
  succeeded a moment later, so ordinary retries read as errors. The failed step keeps its row but
  now reads as retried, muted: « ↺ Generating · reading a project file failed · 7 ms · retried »
  (« réessayé » in French, « r » in ASCII), and the card's heading says no failure. A failure no
  later attempt recovered keeps its red « ✗ » to the end.
