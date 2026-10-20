- **The workspace installs a declared schedule's trigger on your yes (D1).** On a Mac, in your
  own terminal, once « active-le » has declared a schedule in `nika.yaml`, the workspace asks
  « Installer le déclencheur sur ce Mac ? » with two rows, « Installer et activer » (recommended)
  and « Garder déclaré seulement », under the Session's report of what it wrote. « oui » writes
  and loads the launchd unit run by the program you use and says when it fires next (« Installé
  et actif · … prochain run : … »); « désactive-le » removes it. Nothing is installed without that
  yes; any other door, and every test, keeps the schedule declared and names the command.
