# full-loop.tape — the whole painkiller loop in one honest capture:
#   check catches → the named fix → run (mock, offline) → the receipt.
#
# Rendered by scripts/media/render-tape.sh (VHS · https://github.com/charmbracelet/vhs)
# against the REAL installed binary in a staged git workdir. Every command
# shown exists in the released CLI; every line of output is the binary's own.
# The workflow pair is honesty-gated by validate-media.sh:
#   fixtures/broken-release-notes.nika.yaml must keep FAILING `nika check`,
#   fixtures/fixed-release-notes.nika.yaml must stay clean.
Output full-loop.gif

Set Shell zsh
Set FontFamily "JetBrains Mono"
Set FontSize 22
Set Width 1420
Set Height 780
Set Padding 24
Set Theme { "name": "nika", "background": "#08090b", "foreground": "#dbe3ee", "cursor": "#4f86ff", "black": "#10131a", "red": "#ff6b6b", "green": "#34d399", "yellow": "#f0b45b", "blue": "#4f86ff", "magenta": "#b07bff", "cyan": "#22d3ee", "white": "#dbe3ee", "brightBlack": "#5b6472", "brightRed": "#ff8f8f", "brightGreen": "#5ee6b0", "brightYellow": "#ffd08a", "brightBlue": "#8db4ff", "brightMagenta": "#c9a2ff", "brightCyan": "#67e3f5", "brightWhite": "#ffffff" }
Set TypingSpeed 32ms
Set PlaybackSpeed 1.0

Hide
Type "setopt interactive_comments && clear"
Enter
Show

Type "# an agent wrote release-notes.nika.yaml — audit it BEFORE it spends"
Enter
Sleep 900ms
Type "nika check release-notes.nika.yaml"
Enter
Sleep 5200ms

Type "clear"
Enter
Type "# every finding names its fix — apply the two renames it suggested"
Enter
Sleep 800ms
Type "sed -i '' 's/digets/digest/; s/wrte/write/' release-notes.nika.yaml"
Enter
Sleep 700ms
Type "nika check release-notes.nika.yaml"
Enter
Sleep 5000ms

Type "clear"
Enter
Type "# clean — run it (mock/echo: offline, zero keys · or ollama/…, any provider)"
Enter
Sleep 900ms
Type "nika run release-notes.nika.yaml --model mock/echo"
Enter
Sleep 3600ms

Type "# every run leaves a receipt — a hash-chained trace you can verify"
Enter
Sleep 800ms
Type "nika trace verify .nika/traces/*.ndjson"
Enter
Sleep 4200ms
