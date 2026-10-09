- **The verifier now sees an admitted component the workflow already
  holds.** A request may say to use an admitted catalogue component when
  one applies. When the reviewed workflow holds such a component, the
  question on each part of the request and the questions over its trial
  run are now told so: which component, whether it is expanded into the
  workflow or invoked as a child workflow, and what its holes are bound
  to. A held component can satisfy that one clause. Every other operation,
  target, threshold and output the request asks is still checked on its
  own. When the verifier first judged that clause missing, it can now name
  the held component instead of abstaining. That keeps the part open for
  the trial run or a correction; it never makes the workflow ready by
  itself.
