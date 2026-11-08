#!/usr/bin/env bash
set -euo pipefail
java_major=$(sed -n 's/.*"java_major": *\([0-9]*\).*/\1/p' research-lock.json | head -1)
bin="$RUNNER_TEMP/vise-bin"
mkdir -p "$bin"
if [ "$(uname)" = Darwin ]; then
  ln -sfn /opt/homebrew/bin/python3.13 "$bin/python3"
  ln -sfn /opt/homebrew/bin/python3.13 "$bin/python"
  java_home=$(/usr/libexec/java_home -v "$java_major")
  { echo "$bin"; echo "$java_home/bin"; echo /opt/homebrew/bin; } >> "$GITHUB_PATH"
  echo "JAVA_HOME=$java_home" >> "$GITHUB_ENV"
  if [ -x /opt/homebrew/bin/sccache ] && [ -z "${VISE_NO_SCCACHE:-}" ]; then
    { echo "RUSTC_WRAPPER=/opt/homebrew/bin/sccache"; echo "CARGO_INCREMENTAL=0"; } >> "$GITHUB_ENV"
  fi
else
  python=$(command -v python3.13 || true)
  [ -n "$python" ] || { echo "::error::no python3.13 on this runner"; exit 1; }
  ln -sfn "$python" "$bin/python3"
  ln -sfn "$python" "$bin/python"
  major_of() { "$1/bin/java" -version 2>&1 | sed -n 's/.*version "\([0-9]*\).*/\1/p' | head -1; }
  java_home=${JAVA_HOME:-}
  if [ -z "$java_home" ] || [ "$(major_of "$java_home")" != "$java_major" ]; then
    java_home=""
    if command -v java > /dev/null; then
      candidate=$(dirname "$(dirname "$(readlink -f "$(command -v java)")")")
      [ "$(major_of "$candidate")" = "$java_major" ] && java_home=$candidate
    fi
  fi
  if [ -z "$java_home" ] && command -v nix-build > /dev/null; then
    out=$(nix-build '<nixpkgs>' --no-out-link -A "jdk${java_major}_headless" 2>/dev/null || true)
    [ -n "$out" ] && java_home=$out/lib/openjdk
  fi
  [ -n "$java_home" ] || { echo "::error::no JDK $java_major on this runner"; exit 1; }
  { echo "$bin"; echo "$java_home/bin"; } >> "$GITHUB_PATH"
  echo "JAVA_HOME=$java_home" >> "$GITHUB_ENV"
  if [ -z "${VISE_NO_SCCACHE:-}" ] && sccache=$(command -v sccache); then
    { echo "RUSTC_WRAPPER=$sccache"; echo "CARGO_INCREMENTAL=0"; } >> "$GITHUB_ENV"
  fi
fi
echo "VISE_NO_AUDIO=1" >> "$GITHUB_ENV"
