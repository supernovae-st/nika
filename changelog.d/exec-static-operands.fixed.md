- **Static exec permission checks** now judge bare immutable string
  constants in exec URL and script operands using the same rules as
  literals. Script permission inference uses the same known arguments.
  Replaceable inputs stay dynamic; shell expansion and computed working
  directories remain outside this check.
