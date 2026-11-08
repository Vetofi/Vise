#!/bin/bash
set -euo pipefail
repo=${VISE_REPO:-$HOME/code/vise}
root=${VISE_PORT_MAIN:-$HOME/code/vise-parity/port-main}
ref=${VISE_PORT_REF:-origin/main}
shared=$HOME/code/vise-parity/shared-local
log() { echo "$(date -u +%FT%TZ) $*"; }

if [ ! -e "$root/.git" ]; then
  git -C "$repo" fetch -q origin
  git -C "$repo" worktree add -q --detach "$root" "$ref"
fi
cd "$root"
git fetch -q origin
git checkout -q --detach "$ref"
if [ ! -d .local ]; then
  mkdir .local
  for d in minecraft vise-client tools datagen; do ln -s "$shared/$d" ".local/$d"; done
fi
source "$HOME/code/vise-parity/env.sh"

if [ -n "${VISE_PORT_VERSION:-}" ]; then
  kind=release new=$VISE_PORT_VERSION
else
  set +e
  python3 tools/version_port.py detect --dry-run ${VISE_PORT_SNAPSHOTS:+--snapshots} > "$root/.local/detect.txt"
  found=$?
  set -e
  cat "$root/.local/detect.txt"
  case $found in
    0) exit 0 ;;
    10) kind=release ;;
    11) kind=snapshot ;;
    *) log "detect failed ($found)"; exit "$found" ;;
  esac
  new=$(sed -n "s/^New $kind: \([^ ]*\) .*/\1/p" "$root/.local/detect.txt" | head -1)
fi
old=$(python3 -c 'import json; print(json.load(open("research-lock.json"))["minecraft"]["version"])')
log "Minecraft $new ($kind) is out; Vise pins $old"

if [ "$kind" = snapshot ]; then
  python3 tools/version_port.py port "$new" --no-regen
else
  python3 tools/version_port.py port "$new"   # prepare both, diff, regen, report
  python3 tools/parity.py > "$root/.local/baseline.log" 2>&1 || true
  python3 tools/version_port.py harnesses "$new" --old "$old" --baseline .local/parity-runner/state.json
  python3 tools/version_port.py report "$old" "$new" --lines 30
  python3 tools/version_port.py publish "$old" "$new" --check-github --show 0
fi
[ -n "${VISE_PORT_VERSION:-}" ] || python3 tools/version_port.py detect ${VISE_PORT_SNAPSHOTS:+--snapshots} > /dev/null || true
report=${VISE_VERSIONS:-$HOME/code/vise-parity/versions}/diff-$old-$new/report.md
log "Port report: $report"
if [ -n "${VISE_PORT_KIRA_CARD:-}" ]; then
  text="Minecraft $new ($kind) is out. Port report ready on the Mac mini ($report); publish's dry run is in the log."
  node "$HOME/code/vise-parity/tools/kira.mjs" card_note \
    "$(python3 -c 'import json,sys; print(json.dumps({"ref": "card " + sys.argv[1], "text": sys.argv[2]}))' \
      "$VISE_PORT_KIRA_CARD" "$text")" > /dev/null || true
fi
