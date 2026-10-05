- **The workspace activity card says what is under way, then how it ended.**
  One card per turn follows the Session's typed activity: its heading names
  the phase under way (Understanding, Generating, Checking, Repairing), one
  row keeps the step under way, repairs share one counted row, and the
  workflow author's calls (writing and model review) share one row that says
  in plain words what the call does (`write a step`, `review your request` …),
  the requested model (never a served identity) and whether it is running,
  returned or stopped. The heading counts author calls, repair calls and
  stopped ones, and says that conversation routing and decision-service calls
  are not counted there. A returned call is not shown as a success. When the
  turn ends the card reads `Settled`, `Stopped by you` or `Not completed`,
  with the time the terminal measured. While a turn works, the hint says that
  typing waits and that `Ctrl+C` twice leaves without recalling a call already
  sent; a scrolled-back conversation says `End` returns to the latest
  messages. No phase, model, percentage or usage is inferred from words.
