- **An authoring call's receipt says how many requests it sent.** Each
  authoring call in a compile's receipt records `requests_sent`, read
  from the provider's own record of its dispatches: a call that left
  twice says two, a call its deadline cut says one, and a harness route,
  which counts its own invocations, says nothing.
