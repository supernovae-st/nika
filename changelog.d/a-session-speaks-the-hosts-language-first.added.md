- **A Session can speak the host's language before the person writes.**
  `SessionRuntime::with_initial_language` takes the host's guess from its locale. The restored
  notice and `/status` then read in French under `fr_FR` from the first screen. A line the
  person types, or one a restored conversation kept, still decides. The restored notice is now
  said in the restored language. A turn that parks with nothing to ask says how to go on,
  rather than showing an empty question.
