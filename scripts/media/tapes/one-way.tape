# one-way.tape — the README front door: one real command, one real run.
# Minimal visual language: Geist Mono, graphite frame, one cool-violet accent.

Output one-way.gif
Output one-way.mp4
Output one-way.webm

Require nika

Set Shell zsh
Set FontFamily "Geist Mono"
Set FontSize 26
Set LineHeight 1.35
Set Width 1320
Set Height 500
Set Padding 48
Set Margin 18
Set MarginFill "#08080a"
Set BorderRadius 12
Set WindowBar RingsRight
Set WindowBarSize 38
Set Theme { "name": "nika-one-way", "background": "#0b0b0d", "foreground": "#f4f4f5", "cursor": "#8b8cf8", "black": "#18181b", "red": "#fb7185", "green": "#9fe3c2", "yellow": "#e7c47a", "blue": "#8b8cf8", "magenta": "#b7a8ff", "cyan": "#a5b4fc", "white": "#f4f4f5", "brightBlack": "#71717a", "brightRed": "#fda4af", "brightGreen": "#c4f1da", "brightYellow": "#f3db9e", "brightBlue": "#b8b9ff", "brightMagenta": "#d0c8ff", "brightCyan": "#c7d2fe", "brightWhite": "#ffffff" }
Set TypingSpeed 18ms
Set Framerate 30
Set PlaybackSpeed 1

Env TMPDIR "/tmp"

Hide
Type "clear"
Enter
Show

Sleep 650ms
Type "nika try 01-hello --quiet"
Sleep 180ms
Enter
Sleep 1800ms
Screenshot one-way.png
Sleep 2400ms
