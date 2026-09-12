`nika init --wire detected` preserves the detected-client scope instead of
substituting `all`. Both init paths propagate client wiring failures; the
wizard no longer reports a ready project after a refused or failed wire.

New projects receive a human `NIKA.md` guide, file-purpose receipts, Git
guidance and an explicit offer for shared project settings. Claude project
hooks now ship beside Cursor hooks, with project-root paths and the same
canonical scripts. Existing files remain preserved. The optional `nika.yaml`
receipt carries its status marker before the next-command block.
