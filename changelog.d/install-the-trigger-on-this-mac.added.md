- **A declared schedule can be installed on this Mac from the conversation.** Once a schedule
  lands in `nika.yaml`, the terminal Session asks « Installer le déclencheur sur ce Mac ? ». The
  question names the workflow, its cadence and zone, the missed-run policy, the ceiling per run
  and the effect (one launchd unit run by the program that wrote the workflow). « Installer et
  activer » writes the unit as `arm --emit launchd --write` does, loads it and says when it runs
  next. « Garder déclaré seulement » installs nothing, and « désactive-le » unloads and removes
  it. Nothing is installed without that yes. Hosts other than the terminal on macOS keep the
  schedule declared, as before.
