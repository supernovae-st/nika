# Nika Constellation — DX Makefile
# Run from scripts/ dir or use `make -C scripts <target>`

.PHONY: help stats dashboard verify nuke-check verify-all session-prep clean

help:
	@echo "🦋 Nika Constellation — Dev Commands"
	@echo ""
	@echo "  make stats          — Quick state dashboard (terminal)"
	@echo "  make dashboard      — Regenerate HTML dashboard"
	@echo "  make dash-open      — Generate + open HTML dashboard"
	@echo "  make nuke-check     — Verify M1 nuke candidates are safe"
	@echo "  make verify-all     — Run all 4 cargo checks (the ritual)"
	@echo "  make session-prep   — Show all stats + open dashboard (pre-session)"
	@echo ""

stats:
	@bash nika-stats.sh

dashboard:
	@python3 update-constellation-dashboard.py

dash-open: dashboard
	@open ../docs/constellation/index.html

nuke-check:
	@bash nuke-verify.sh

verify-all:
	@cd ../tools && echo "🔍 cargo check --workspace" && cargo check --workspace --quiet
	@cd ../tools && echo "🔍 cargo check --no-default-features" && cargo check --workspace --no-default-features --quiet
	@cd ../tools && echo "🧪 cargo test --workspace --lib" && cargo test --workspace --lib --quiet
	@cd ../tools && echo "📎 cargo clippy --all-targets" && cargo clippy --workspace --all-targets --quiet
	@echo ""
	@echo "✅ All 4 checks passed — ready to commit"

session-prep: stats dashboard
	@echo ""
	@echo "📂 Memory files to read for next session:"
	@echo "  ~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_roadmap_v5_honest.md"
	@echo "  ~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_m1_copy_paste_prompt.md"
	@echo "  ~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_extraction_runbook.md"
	@echo ""
	@echo "🚀 Ready to start next session"

clean:
	@rm -f ../docs/constellation/index.html
	@echo "Cleaned generated dashboard"
