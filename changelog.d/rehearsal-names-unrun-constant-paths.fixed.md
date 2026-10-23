- **Rehearsal warnings include files whose paths are constants.** When a skipped model step
  feeds a file write through `${{ const.out_path }}`, the trial now names that file in its
  warning that the read-back is not a result of the run. Literal paths retain the same warning;
  paths that depend on runtime inputs or task results are not guessed.
