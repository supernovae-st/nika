- **Git push gates receive every ref proposal.** Tag-only and unchanged-tree
  pushes now reach the ref-aware gate and protected-branch guard; only the
  gate's explicit deletion-only decision can skip its checks. Real
  Lefthook/local-remote regressions cover refusal and stdin delivery.
