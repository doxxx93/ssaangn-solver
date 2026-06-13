#!/bin/zsh
# Clue oracle for the two-agent playtest harness (docs/plans/2026-06-14-agent-playtest-harness.md).
#
# Returns the real ssaangn clue for a guess against a hidden secret, using the
# repo's verified matcher (`cargo run clue`). The solver agent calls this and is
# told NOT to read the secret file, so it plays blind.
#
# Usage:  scripts/oracle.sh <guess>
# Secret: $SSAANGN_SECRET if set, else /tmp/ssaangn_secret.txt (see pick_secret.sh).
# Output: "CLUE 🍆🍌"  or  "SOLVED 🥕🥕"
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
secret="${SSAANGN_SECRET:-$(cat /tmp/ssaangn_secret.txt)}"
guess="$1"
clue="$(printf '%s %s\n' "$secret" "$guess" | (cd "$ROOT" && cargo run --quiet --release clue 2>/dev/null))"
if [ "$clue" = "🥕🥕" ]; then
  echo "SOLVED $clue"
else
  echo "CLUE $clue"
fi
