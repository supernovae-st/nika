- **A confirm gate whose answer the effect never reads is refused before anything runs.**
  - Consider a gate whose answer was bound but whose `when:` decided on a caller input instead
    (`when: ${{ inputs.go == true }}`). `nika check` passed it without a blocking finding, and after an
    explicit « no » the effect still fired.
  - `nika check` and `nika run` now refuse that shape with `NIKA-SEC-014`, naming the gate and the
    effect. The message teaches the fix: bind the answer and gate on it
    (`when: ${{ with.go == true }}`).
  - A run paused before this fix is refused the same way when it is resumed, since the audit runs
    before any task.
  - A gate that reads the answer in a shape the checker cannot evaluate still gets the advisory
    hint, never a refusal.
