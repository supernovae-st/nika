- **A harness authoring call's record tells thinking from answering.** The
  `activity` of a terminal ACP authoring record now also says when this
  session's first thought and first answer arrived (`first_thought_ms`,
  `first_answer_ms`) and which update it received last, by its closed word
  and time (`last_update`), each null before any. A receipt can now say a
  call was silent until its first answer, or that its last update before a
  silence was a thought. The fields are additive; counts, deadlines and
  acceptance are unchanged, and no frame text is kept.
