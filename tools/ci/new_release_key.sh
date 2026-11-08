#!/usr/bin/env bash
set -euo pipefail
secret=${1:-RELEASE_SIGNING_KEY}
case "$secret" in RELEASE_SIGNING_KEY|RELEASE_SIGNING_KEY_NEXT) ;; *) echo "unknown secret $secret" >&2; exit 2 ;; esac
repo=${GITHUB_REPOSITORY:-vise-rust/vise}
cd "$(dirname "$0")/../.."
gh api "repos/$repo/environments/release" > /dev/null || {
  echo "The release environment doesn't exist yet: create it first (docs/security.md)." >&2; exit 1; }
public=$(mktemp)
trap 'rm -f "$public"' EXIT
cargo run --quiet --locked -p vise-release-sign -- keygen --public-out "$public" \
  | gh secret set "$secret" --env release --repo "$repo"
echo "$secret is set in $repo's release environment. Its public key, for crates/vise-release-sign/release-keys.txt:"
cat "$public"
