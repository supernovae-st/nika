- **A folder you name as `copies/` is tried before « propre ✔ ».** « Dans le dossier copies/ … »,
  « dans commandes/ … » or « du dossier notes/ » named no folder the rehearsal could hold, so a
  workflow reading those files was proposed as clean without running a step on them, and its
  first real run could fail on its own jq. Such a folder is now copied into the rehearsal room
  where the workflow reads under it (within the room's copy bound, originals untouched), and the
  steps after a model step that declares its answer's shape run on an example of that shape: a
  wrong jq is caught before you are asked to say yes.
