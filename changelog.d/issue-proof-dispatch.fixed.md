- **Manual issue proof checks read the issue.** Workflow dispatch fetches one
  current closed-issue snapshot instead of judging empty event fields. The
  body, labels and close reason reach the same proof gate as normal close
  events. Fetch failures and invalid snapshots stop without reopening or
  commenting on the issue; an actual missing proof still refuses the close.
