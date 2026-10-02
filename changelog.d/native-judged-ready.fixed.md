- **A workflow the authoring model writes itself is READY only once it is judged against the whole request.**
  - When the authoring model writes the workflow (the native door), or its tasks and programs
    (the sketch door), the compiler's own checks only admit it: a valid workflow that kept the
    wrong rows, such as the lowest amounts where the request asks for the highest, was READY.
    The finished workflow's own bytes are now judged against the whole request by the same
    bounded judge as a workflow compiled from the model's plan, asked through the authoring
    model.
  - A workflow the judge finds unfaithful, or cannot judge, is not READY: the request stays
    incomplete, naming the part it misses, and nothing of it is kept to replay.
  - The judge's calls are authoring calls: counted in the receipt with their usage, and held
    to `--authoring-max-calls`. A judge call the limit refuses is never sent, and the request
    stays incomplete.
  - The number of authoring requests a compile may need now counts every judge question.
    That number is shown in its review and refuses typed repairs, samples or strategies the grant
    cannot honor:
    - `3 + repairs` for the native door;
    - `4 + repairs` for the sketch door;
    - `2 × samples + 14 × repairs + 12` for a plan compiled from the model's proposals.
  - One check asks at most 8 clause questions. A clause past them is not asked: the request stays
    incomplete, and the message says why.
  - Typed repairs under `--authoring-strategy off` now count.
  - A typed `only` or `escalate` strategy needs `--authoring-max-calls 2` or more, and a typed
    `sketch` needs 3.
  - Limits: the judge is a model, so its approval is bounded evidence, not proof.
