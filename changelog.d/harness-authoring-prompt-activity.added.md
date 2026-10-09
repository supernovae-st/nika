- **A harness authoring call's record says what it received after its
  prompt.** A terminal ACP authoring record now carries `activity`: when
  the prompt was written, how many complete frames arrived after it in
  fixed categories (answer, thought, usage, status, other updates, client
  requests, the answer or error to the prompt) and when the last did;
  another session's frames, stray responses, other notifications and
  unreadable lines are counted apart, so they never read as the call
  advancing. When the session ended, it also names how: completed, the
  transport, or the exact refused category (a token or request limit, a
  declined or called-off turn, a tool, media or other update, a
  permission or client request) that the safe message folds into one
  sentence. No prompt, answer, thought, method, identifier, path or error
  text is kept, and acceptance, deadlines, Stop and retries are
  unchanged. A thought or usage frame proves only that the adapter wrote
  it, and no frame after the prompt cannot tell a stalled adapter from a
  model reasoning without emitting anything.
