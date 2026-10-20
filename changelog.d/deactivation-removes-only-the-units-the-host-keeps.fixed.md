- **« désactive-le » removes only units the trigger host says it keeps.** When an earlier
  conversation had installed a trigger, the Session built its launchd path in
  `~/Library/LaunchAgents` itself. A host that keeps its units elsewhere was then handed a file it
  never wrote. A host now says where its units are (`TriggerHost::unit_of`, given by the launchd
  host). The declared state of a schedule also asks the host whether its unit is installed.
