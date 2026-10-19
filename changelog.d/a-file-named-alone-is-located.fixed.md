- **A file the person names alone is found where it is.** A request that names a file by its
  name only (« si la note tarifs-2027.md échoue ») while the file lives in a folder
  (`./notes/tarifs-2027.md`) was refused at every revision as « UNREALIZED PATH », even when the
  workflow read every note through a glob. The observation now locates the one file of that name
  under the project (hidden folders and build outputs aside), and a workflow that reads it where
  it is realizes the name. Two files of that name, or none, leave it unrealized as before.
