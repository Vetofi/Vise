#!/usr/bin/env bash
set -euo pipefail
root=${RUNNERS:-$HOME/.actions-runners}
max_kb=$(( ${MAX_TARGET_GB:-12} * 1024 * 1024 ))
keep=${KEEP_PREPARED:-2}
stale_min=$(( ${STALE_DAYS:-3} * 24 * 60 ))
busy() { pgrep -f "$1/bin/Runner.Worker" > /dev/null; }

any_busy=0
for runner in "$root"/vise "$root"/vise-[0-9]*; do
  [ -d "$runner/_work" ] || continue
  if busy "$runner"; then echo "$(basename "$runner"): in a job, skipped"; any_busy=1; continue; fi
  rm -rf "${runner:?}/_work/_temp/"* 2>/dev/null || true
  checkout="$runner/_work/vise/vise"
  [ -d "$checkout" ] || continue
  stale=0
  for dir in "$checkout"/.local/*/; do
    dir=${dir%/}
    [ -L "$dir" ] || [ "$(basename "$dir")" = parity-runner ] && continue
    if [ -z "$(find "$dir" -mmin -"$stale_min" -print -quit 2>/dev/null)" ]; then rm -rf "$dir"; stale=$((stale + 1)); fi
  done
  [ "$stale" = 0 ] || echo "$(basename "$runner"): removed $stale harness folders untouched for ${STALE_DAYS:-3} days"
  targets=()
  [ -f "$checkout/target/CACHEDIR.TAG" ] && targets+=("$checkout/target")
  while IFS= read -r tag; do targets+=("$(dirname "$tag")"); done < <(
    find "$checkout/.local" -mindepth 2 -maxdepth 2 -name CACHEDIR.TAG -type f 2>/dev/null)
  [ ${#targets[@]} -gt 0 ] || continue
  size=$(du -sk "${targets[@]}" | awk '{s += $1} END {print s}')
  if [ "$size" -gt "$max_kb" ]; then
    rm -rf "${targets[@]}"
    echo "$(basename "$runner"): removed ${#targets[@]} build folders ($(( size / 1024 / 1024 )) GB)"
  fi
done

minutes() {
  local t=$1 d=0 h=0 m a b c
  if [[ $t == *-* ]]; then d=${t%%-*}; t=${t#*-}; fi
  IFS=: read -r a b c <<< "$t"
  if [ -n "$c" ]; then h=$a m=$b; else m=$a; fi
  echo $(( 10#$d * 1440 + 10#$h * 60 + 10#$m ))
}
orphan_min=$(( ${ORPHAN_HOURS:-3} * 60 ))
ps -A -o pid=,ppid=,etime=,command= | while read -r pid ppid etime command; do
  case "$command" in *java*) ;; *) continue ;; esac
  case "$command" in *"$root"/*/_work/*|*/.claude/worktrees/*/.local/*|*/code/vise/.local/*) ;; *) continue ;; esac
  [ "$(minutes "$etime")" -ge "$orphan_min" ] || continue
  if [ "$ppid" != 1 ]; then
    case "$(ps -o command= -p "$ppid" 2>/dev/null)" in *"systemd --user"*) ;; *) continue ;; esac
  fi
  if kill "$pid" 2>/dev/null; then echo "orphaned java $pid ($etime old) stopped: ${command:0:160}"; fi
done

prepared="$root/vise/_work/vise/shared-local/prepared"
if [ "$any_busy" = 0 ] && [ -d "$prepared" ] && mkdir "$prepared/.lock" 2>/dev/null; then
  trap 'rmdir "$prepared/.lock" 2>/dev/null || true' EXIT
  ls -1t "$prepared" | tail -n +"$(( keep + 1 ))" | while read -r old; do
    rm -rf "${prepared:?}/$old"
    echo "prepared resources $old removed"
  done
fi
df -h "$root" | tail -1
