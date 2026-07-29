# nika-hero.tape — the audit-then-run story in ~12 seconds (the README
# and homebrew hero). Governed replacement for the July 2 hand recording
# (asciinema · pre-flag-day output): every command exists in the released
# CLI, every line of output is the binary's own, and the workflow shown
# is scripts/media/fixtures/meeting-actions.nika.yaml — checked green by
# validate-media.sh, run offline on mock/echo.
#
# Rendered by scripts/media/render-tape.sh nika-hero (VHS) in the staged
# workdir; the renderer also installs media/nika-hero.gif (the hotlinked
# path — homebrew-tap and the city READMEs embed it by URL).
Output nika-hero.gif

Set Shell zsh
Set FontFamily "JetBrains Mono"
Set FontSize 21
Set Width 1420
Set Height 780
Set Padding 24
Set Theme { "name": "nika", "background": "#08090b", "foreground": "#dbe3ee", "cursor": "#4f86ff", "black": "#10131a", "red": "#ff6b6b", "green": "#34d399", "yellow": "#f0b45b", "blue": "#4f86ff", "magenta": "#b07bff", "cyan": "#22d3ee", "white": "#dbe3ee", "brightBlack": "#5b6472", "brightRed": "#ff8f8f", "brightGreen": "#5ee6b0", "brightYellow": "#ffd08a", "brightBlue": "#8db4ff", "brightMagenta": "#c9a2ff", "brightCyan": "#67e3f5", "brightWhite": "#ffffff" }
Set TypingSpeed 30ms
Set PlaybackSpeed 1.0

Hide
Type "setopt interactive_comments && clear"
Enter
Show

Type "# audit BEFORE it spends · plan, permits, cost, secrets, types"
Enter
Sleep 700ms
Type "nika check meeting-actions.nika.yaml"
Enter
Sleep 4600ms

Type "# clean — run it (mock/echo: offline · or ollama, mistral, any provider)"
Enter
Sleep 800ms
Type "nika run meeting-actions.nika.yaml"
Enter
Sleep 5200ms
Sleep 1400ms
