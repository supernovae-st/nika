- **Finite Session choices use native terminal controls and keep the current
  request.** Single and multiple answers, intelligence settings, proposal actions,
  paused questions, and required boolean/enum inputs have selectable lists.
  Navigation and empty submission apply nothing; readers and settings preserve
  the draft. An explicit Save/Run request continues through clarification without
  asking for the same permission again. After reopening, a new rerun request can
  reuse an unchanged saved or previously observed workflow through normal Run
  admission; reopening itself performs no action. Failed conversation requests
  retain their original context for recovery, and expired questions show their
  renewal requirement before an answer.
