#!/usr/bin/env bash
set -euo pipefail
shared="${RUNNER_WORKSPACE:?}/shared-local"
source="${VISE_SHARED_SOURCE:-$HOME/code/vise-parity/shared-local}"
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' research-lock.json | head -1)
if [ ! -f "$shared/minecraft/$version/prepared.json" ]; then
  [ -f "$source/minecraft/$version/prepared.json" ] || { echo "::error::no imported Minecraft $version in $source"; exit 1; }
fi
mkdir -p "$shared"
for entry in "$source"/*; do
  name=$(basename "$entry")
  [ "$name" = vise-client ] || [ -e "$shared/$name" ] && continue
  if [ "$(uname)" = Darwin ]; then cp -cR "$entry" "$shared/"; else cp -R --reflink=auto "$entry" "$shared/"; fi
done
mkdir -p .local "$shared/prepared"
for entry in "$shared"/*; do
  name=$(basename "$entry")
  case "$name" in prepared|sounds|*.log) continue ;; esac
  if [ -L ".local/$name" ] || [ ! -e ".local/$name" ]; then ln -sfn "$entry" ".local/$name"; fi
done

sha256() { if command -v shasum > /dev/null; then shasum -a 256; else sha256sum; fi; }
key=${VISE_PREPARE_KEY:-$(git ls-files -s crates/vise-prepare mods/vise/src/test/java research-lock.json | sha256 | cut -c1-16)}
prepared="$shared/prepared/$key"
lock="$shared/prepared/.lock"
until mkdir "$lock" 2>/dev/null; do
  if [ -n "$(find "$lock" -maxdepth 0 -mmin +30 2>/dev/null)" ]; then rmdir "$lock" 2>/dev/null || true; fi
  sleep 5
done
trap 'rmdir "$lock" 2>/dev/null || true' EXIT
outputs=$({ cat "$shared"/prepared/*/.entries; cd .local && ls -d vise-client*; } 2>/dev/null || true)
for name in $(echo "$outputs" | LC_ALL=C sort -u); do
  if [ -e ".local/$name" ] && [ ! -L ".local/$name" ]; then
    rm -rf ".local/${name:?}"
    echo "Removed .local/$name, left by a failed preparation"
  fi
done
if [ -f "$prepared/.entries" ] && ! grep -qx vise-client "$prepared/.entries"; then
  echo "Preparation $key lacks vise-client: preparing again"
  rm -rf "$prepared"
fi
if [ ! -f "$prepared/.entries" ]; then
  echo "Preparing resources with this checkout's vise-prepare ($key)"
  rm -rf "$prepared" && mkdir -p "$prepared"
  for entry in .local/*; do
    if [ -L "$entry" ] && [[ "$(readlink "$entry")" == "$shared/prepared/"* ]]; then rm "$entry"; fi
  done
  before=$(ls -1A .local | LC_ALL=C sort)
  started=$SECONDS
  cargo build --release --locked -p vise-prepare
  if ! target/release/vise-prepare --root "$PWD" > "$RUNNER_TEMP/prepare.log" 2>&1; then
    tail -40 "$RUNNER_TEMP/prepare.log"
    LC_ALL=C comm -13 <(echo "$before") <(ls -1A .local | LC_ALL=C sort) |
      while read -r name; do rm -rf ".local/${name:?}"; done
    rm -rf "$prepared"
    exit 1
  fi
  LC_ALL=C comm -13 <(echo "$before") <(ls -1A .local | LC_ALL=C sort) > "$RUNNER_TEMP/prepared-entries"
  while read -r name; do mv ".local/$name" "$prepared/$name"; done < "$RUNNER_TEMP/prepared-entries"
  mv "$RUNNER_TEMP/prepared-entries" "$prepared/.entries"
  echo "Prepared in $((SECONDS - started)) s: $(tr '\n' ' ' < "$prepared/.entries")"
fi
for entry in "$shared"/*; do
  name=$(basename "$entry")
  case "$name" in prepared|sounds|*.log) continue ;; esac
  [ -e "$prepared/$name" ] || ln -sfn "$entry" "$prepared/$name"
done
while read -r name; do
  if [ -L ".local/$name" ] || [ ! -e ".local/$name" ]; then ln -sfn "$prepared/$name" ".local/$name"; else
    echo "::error::.local/$name is not a link to the prepared resources"; exit 1; fi
done < "$prepared/.entries"
assets="$prepared/vise-client/$version/assets/minecraft"
if [ -f "$shared/sounds/$version/sounds.json" ] && [ -d "$assets" ]; then
  for entry in sounds.json sounds; do
    [ -L "$assets/$entry" ] && rm "$assets/$entry"
    [ -e "$assets/$entry" ] && continue
    if [ "$(uname)" = Darwin ]; then cp -cR "$shared/sounds/$version/$entry" "$assets/"
    else cp -R --reflink=auto "$shared/sounds/$version/$entry" "$assets/"; fi
  done
fi
touch "$prepared"
ls -1dt "$shared/prepared"/*/ 2>/dev/null | tail -n +5 | while read -r old; do rm -rf "$old"; done
