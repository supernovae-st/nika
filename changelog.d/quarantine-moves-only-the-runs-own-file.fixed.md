- **A failed run's quarantine moves only the file the run created, inside
  the project.** The quarantine and its restore now act through directory
  handles opened under the project root: a recorded path that leaves the
  project (`..`, an absolute path) is refused by name before any file
  operation, a directory on the way that is a symbolic link is refused, and
  the quarantine directory is created without following links. A file moves
  only while it is the very file the run created (same device and inode,
  one name, the size the run left); a link, another file, a second name or
  a changed file at the path stays where it is, and the fold says why. A
  restore links the file back at its path, so a file a person put there
  since is never overwritten: that is a named conflict, and both files stay.
