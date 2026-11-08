#!/bin/sh
set -eu

REPO=vise-rust/vise
VERSION=latest
DIR="${XDG_DATA_HOME:-$HOME/.local/share}/vise"
BIN="$HOME/.local/bin"
ARCHIVE=

die() { echo "vise install: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || die "--version needs a tag"; VERSION=$2; shift 2 ;;
    --dir) [ $# -ge 2 ] || die "--dir needs a folder"; DIR=$2; shift 2 ;;
    --bin) [ $# -ge 2 ] || die "--bin needs a folder"; BIN=$2; shift 2 ;;
    --archive) [ $# -ge 2 ] || die "--archive needs a file"; ARCHIVE=$2; shift 2 ;;
    -h|--help) sed -n '2,14p' "$0" 2>/dev/null || true; exit 0 ;;
    *) die "unknown option $1" ;;
  esac
done

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ASSETS="vise-server-linux-x86_64.tar.gz vise-linux-x86_64.tar.gz" ;;
  Darwin-arm64) ASSETS=vise-macos-arm64.zip ;;
  *) die "no Vise release for $(uname -s) $(uname -m) yet: build it from source (docs/server.md)" ;;
esac

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
if [ -n "$ARCHIVE" ]; then
  [ -f "$ARCHIVE" ] || die "no archive $ARCHIVE"
  if [ "$VERSION" = latest ]; then TAG=local; else TAG=$VERSION; fi
  ASSET=$(basename "$ARCHIVE")
  cp "$ARCHIVE" "$TMP/$ASSET"
else
  command -v curl >/dev/null || die "curl is needed"
  if command -v sha256sum >/dev/null; then SHA="sha256sum"
  elif command -v shasum >/dev/null; then SHA="shasum -a 256"
  else die "sha256sum or shasum is needed"; fi

  api() { curl -fsSL -H "Accept: application/vnd.github+json" "https://api.github.com/repos/$REPO/releases/$1"; }
  if [ "$VERSION" = latest ]; then
    JSON=$(api tags/nightly) || JSON=$(api latest) || die "no release on GitHub"
  else
    JSON=$(api "tags/$VERSION") || die "no release $VERSION on GitHub"
  fi

  TAG=$(printf '%s\n' "$JSON" | sed -n 's/^  "tag_name": "\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$TAG" ] || die "could not read the release's tag"
  case "$TAG" in
    nightly|nightly-*)
      NAME=$(printf '%s\n' "$JSON" | sed -n 's/^  "name": "Nightly \([0-9-]\{10\}\) · \([0-9a-f]\{1,40\}\)".*/nightly-\1-\2/p' | head -n 1)
      [ -z "$NAME" ] || TAG=$NAME ;;
  esac
  for ASSET in $ASSETS; do
    FIELDS=$(printf '%s\n' "$JSON" | awk -v asset="$ASSET" '
      /"name": / { line = $0; sub(/.*"name": "/, "", line); sub(/".*/, "", line); current = line }
      current == asset && /"digest": / { line = $0; sub(/.*"digest": "sha256:/, "", line); sub(/".*/, "", line); digest = line }
      current == asset && /"browser_download_url": / { line = $0; sub(/.*"browser_download_url": "/, "", line); sub(/".*/, "", line); url = line }
      END { print url; print digest }')
    URL=$(printf '%s\n' "$FIELDS" | sed -n 1p)
    DIGEST=$(printf '%s\n' "$FIELDS" | sed -n 2p)
    [ -z "$URL" ] || break
  done
  [ -n "$URL" ] || die "release $TAG has none of $ASSETS"
  [ -n "$DIGEST" ] || die "release $TAG publishes no SHA-256 for $ASSET"

  echo "Downloading Vise $TAG ($ASSET)..."
  curl -fL --progress-bar -o "$TMP/$ASSET" "$URL"
  GOT=$($SHA "$TMP/$ASSET" | cut -d' ' -f1)
  [ "$GOT" = "$DIGEST" ] || die "$ASSET doesn't match its SHA-256 (got $GOT, expected $DIGEST): not installed"
fi

mkdir -p "$TMP/x"
case "$ASSET" in
  *.tar.gz) tar -xzf "$TMP/$ASSET" -C "$TMP/x" ;;
  *.zip) command -v unzip >/dev/null || die "unzip is needed"; unzip -q "$TMP/$ASSET" -d "$TMP/x" ;;
esac
SOURCE=$(dirname "$(find "$TMP/x" -name vise-server -type f | head -n 1)")
[ -x "$SOURCE/vise-server" ] || die "$ASSET holds no vise-server (a release from before it existed?)"

RELEASE="$DIR/releases/$TAG"
mkdir -p "$DIR/releases" "$DIR/game-files/vise-client" "$BIN"
rm -rf "$RELEASE"
mv "$SOURCE" "$RELEASE"
mkdir -p "$RELEASE/.local"
ln -sfn "$DIR/game-files/vise-client" "$RELEASE/.local/vise-client"
ln -sfn "$RELEASE" "$DIR/current"
for tool in vise-server vise-prepare; do
  printf '#!/bin/sh\nexec "%s/current/%s" "$@"\n' "$DIR" "$tool" > "$BIN/$tool"
  chmod +x "$BIN/$tool"
done

echo "Installed Vise $TAG in $RELEASE; vise-server and vise-prepare are in $BIN."
if [ "$(uname -s)" = Linux ] && [ ! -e "$RELEASE/vise-engine" ] && command -v ldd >/dev/null; then
  MISSING=$(ldd "$RELEASE/vise-client" 2>/dev/null | awk '/not found/ { printf "%s ", $1 }')
  if [ -n "$MISSING" ]; then
    echo "The game needs libraries this system doesn't have: $MISSING"
    echo "Install them before starting a server:"
    echo "  Debian, Ubuntu: sudo apt install libasound2 libudev1   (Ubuntu 24.04 and later: libasound2t64)"
    echo "  Fedora, RHEL: sudo dnf install alsa-lib systemd-libs"
    echo "  Arch: sudo pacman -S alsa-lib systemd-libs"
    echo "  NixOS: enable programs.nix-ld with alsa-lib and udev in its libraries"
  fi
fi
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) echo "Add $BIN to your PATH, for example: echo 'export PATH=\"$BIN:\$PATH\"' >> ~/.profile" ;;
esac
if [ -z "$(ls -A "$DIR/game-files/vise-client" 2>/dev/null)" ]; then
  echo "Next, prepare the game files from your own Minecraft 26.3 (it needs Java 25 and the game installed once"
  echo "by the official launcher; see docs/server.md):"
  echo "  vise-prepare --prepare --server-jar /path/to/server.jar"
fi
echo "Then set up a server: mkdir my-server && cd my-server && vise-server init"
