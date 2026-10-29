- **A ceiling chosen after `save & run` is receipted for the next run.**
  When the saved workflow has no durable execution constraint, the Session now
  binds the answer to the exact consent and checked bytes instead of asking
  again on the next `run it`. Existing receipts and one-turn Run overrides keep
  their previous authority; changed or unreadable evidence still refuses.
