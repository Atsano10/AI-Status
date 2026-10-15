#!/bin/sh
# Phase 0 spike: Claude Code pipes its statusLine JSON to this script on stdin.
# We save it so we can check whether `rate_limits` shows up on this machine.
# Throwaway: the real bridge (Phase 2) replaces this.

dir="$HOME/Library/Application Support/com.atsano10.statuspal"
mkdir -p "$dir"

# Write to a temp file, then rename, so a reader never sees a half-written file.
cat > "$dir/claude-statusline.json.tmp" && mv "$dir/claude-statusline.json.tmp" "$dir/claude-statusline.json"

echo "StatusPal spike: recording statusLine"
