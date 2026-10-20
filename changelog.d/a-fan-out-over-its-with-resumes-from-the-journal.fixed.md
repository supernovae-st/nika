- **A resume serves back a fan-out over its own `with:` and a group fold.** A `for_each` whose
  collection read the task's own binding (`for_each: { items: "${{ with.ids }}" }`) was never
  stamped, so `--resume` ran it live again: the content a gate had shown changed under the
  answer, and every answer was refused NIKA-SEC-010. The stamp now resolves the collection over
  the item-free `with:`, as the run does. A `${{ group.<name> }}` fold is stamped over its
  declared members, without their durations, which the journal cannot serve back.
