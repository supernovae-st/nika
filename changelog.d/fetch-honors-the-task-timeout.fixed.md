- **`nika:fetch` honors its task's `timeout:`.** A fetch whose task declares `timeout: "3m"` gets
  those three minutes for the whole request, as the stdlib states, instead of a fixed 30s that cut
  long API calls (an image generation, a non-streamed inference) and a 30s idle guard that cut a
  server computing before it answers. A fetch without a task `timeout:` keeps 30s, and a timed-out
  fetch names the setting.
