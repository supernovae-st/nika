- **A home or project path through a symbolic link is refused by name.** Nika opens its owned
  directories (the Session history under the home, among others) through real directories only. A
  path with a symbolic link on the way stopped the terminal Session with « Not a directory (os error
  20) »; the refusal now names the link and the resolved path to use instead.
