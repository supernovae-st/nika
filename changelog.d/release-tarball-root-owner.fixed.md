- **Release tarballs record root ownership.** The v0.123.1 archives recorded the build runner's
  own uid (1001 on the Linux legs, 501 on macOS), so `tar -x` run as root in a rootless
  container failed to restore an unmapped owner. The packaging step now writes every member
  as 0:0 by number (GNU tar `--owner=0 --group=0 --numeric-owner` on Linux, bsdtar
  `--uid 0 --gid 0 --numeric-owner` on macOS); the archive contents, the checksum step and the
  replay of existing assets are unchanged.
