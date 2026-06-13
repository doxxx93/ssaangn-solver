#!/bin/zsh
# Pick a hidden secret answer for the two-agent playtest harness and write it to
# /tmp/ssaangn_secret.txt WITHOUT printing it (so the orchestrator stays blind too).
#
# Usage:
#   scripts/pick_secret.sh              # random official answer (excludes 관심)
#   scripts/pick_secret.sh 두부          # force a specific secret
#   scripts/pick_secret.sh --seed 42    # reproducible random pick
#
# Source pool: data/raw/handle_answers.json (official answers, key "2").
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT=/tmp/ssaangn_secret.txt

if [ -n "$1" ] && [ "$1" != "--seed" ]; then
  printf '%s' "$1" > "$OUT"
  echo "secret set (forced)."
  exit 0
fi

seed="${2:-}"
python3 - "$ROOT" "$seed" <<'PY' > "$OUT"
import json, random, sys
root, seed = sys.argv[1], sys.argv[2]
words = json.load(open(f"{root}/data/raw/handle_answers.json"))["2"]
words = [w for w in words if w != "관심"]
if seed:
    random.seed(int(seed))
sys.stdout.write(random.choice(words))
PY
echo "secret picked (hidden) -> $OUT"
