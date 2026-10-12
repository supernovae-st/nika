- **An agent no longer runs the calls of a message cut at its output limit.** Each call of
  such a message is answered « not run: your message was cut at the output limit; re-issue the
  call with complete arguments » and the loop goes on under the task's own limits, as the
  Session's agent does. A turn the provider interrupted on its side is never acted on: the task
  fails `NIKA-INFER-001`, transient, naming the provider's own word.
