- **« Enregistre ce workflow et lance-le » no longer asks the workflow to create something.** A
  line asking to save or run the workflow itself was read as an effect its tasks had to perform:
  on the welcome demo, « Enregistre ce workflow et lance-le », then Nika's own suggestion
  « sauvegarde-le sous le nom aggregate-by-key et exécute-le », then the `/help` keyword « run
  aggregate-by-key » were each refused with « DROPPED EFFECT: the request states `create` », so
  a workflow that only computes could not be saved from the conversation. Such a line (the
  workflow, a pronoun standing for it or a workflow's name, with nothing but a time, a polite
  word or the name it is saved under) is now read as the Session's act and accounted for, never
  as an effect. A destination still makes it a write: « enregistre-le dans out/resume.md » writes
  that file.
