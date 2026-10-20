- **A run's story now speaks the language you write in.** While a workflow runs from a French
  conversation, the busy row and the activity card read « en cours · meteo-maurice-quotidienne »,
  « ↺ today · depuis le cache », « ◇ en pause · `approve` vous attend » and « succeeded · 4/4
  tâches · 9857 ms », and the story the transcript keeps reads the same. The workflow, the
  tasks, the engine's status word, counts and times stay as they are. For other doors, a
  `RunSink` gains a `say` method that defaults to English, so `nika`'s plain output and every
  existing sink are unchanged.
