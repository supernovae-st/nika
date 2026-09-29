- **A workflow the authoring model wrote is READY after an answer round only once that round judges it.**
  - When the model writes the workflow itself (the native or the sketch door), its first round
    can end with a question, such as the model the workflow runs on. The answer round then
    baked the answer in and was READY with no judge at all.
  - The answer round now keeps the whole request pending on the finished workflow. It is READY
    only when a judge permitted in that round finds it faithful to the whole request: the
    authoring model, or a decision seat.
  - The judge's calls are authoring calls, counted in the receipt with their usage and held to
    `--authoring-max-calls`. One judge request settles a faithful workflow.
  - Without a judge, the answer round is incomplete: the finished workflow is only a preview, and
    the message names the judge to permit. A workflow the judge finds unfaithful stays
    incomplete, naming the part it misses.
  - The answer round of a revision in words works the same way.
  - Limits: the judge is a model, so its approval is bounded evidence, not proof.
