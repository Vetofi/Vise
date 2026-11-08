#!/usr/bin/env bash
set -euo pipefail
repo=${GITHUB_REPOSITORY:-vise-rust/vise}
api="repos/$repo/branches/main/protection"
body=$(gh api "$api" | jq '{
  required_status_checks: (.required_status_checks
    | {strict, checks: [.checks[] | {context, app_id}]}),
  enforce_admins: .enforce_admins.enabled,
  required_pull_request_reviews: {
    require_code_owner_reviews: true,
    required_approving_review_count: 0,
    dismiss_stale_reviews: true,
    require_last_push_approval: false,
    bypass_pull_request_allowances: {users: ["ludovic111"], teams: [], apps: []}
  },
  restrictions: null,
  required_linear_history: .required_linear_history.enabled,
  allow_force_pushes: .allow_force_pushes.enabled,
  allow_deletions: .allow_deletions.enabled,
  required_conversation_resolution: .required_conversation_resolution.enabled,
  lock_branch: .lock_branch.enabled,
  allow_fork_syncing: .allow_fork_syncing.enabled
}')
echo "$body"
if [[ "${1:-}" == --apply ]]; then
  gh api -X PUT "$api" --input - <<< "$body" > /dev/null
  set_now=$(gh api "$api" --jq '[.required_pull_request_reviews.require_code_owner_reviews,
    .enforce_admins.enabled, ([.required_pull_request_reviews.bypass_pull_request_allowances.users[]?.login]
    | index("ludovic111") != null), ([.required_status_checks.checks[].context] | index("verify") != null)]
    | map(tostring) | join(" ")')
  if [[ "$set_now" != "true true true true" ]]; then
    echo "GitHub didn't keep everything (code owners, admins enforced, Ludovic's bypass, verify): $set_now" >&2
    exit 1
  fi
  echo "main now requires code owners' reviews; Ludovic may bypass them; verify is required for everyone."
else
  echo "(not applied: run with --apply)"
fi
