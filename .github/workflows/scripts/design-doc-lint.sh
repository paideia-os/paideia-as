#!/usr/bin/env bash
# Verifies that PRs touching crates/<crate>/src/** reference a design doc URL
# in paideia-os/paideia-os/design/toolchain/**, OR contain a
# `Design-Doc-Waiver: <reason>` line in the PR body or commit messages.
#
# Adapted from design/02-development-environment.md §12.3 for the single-repo
# paideia-as case (the design corpus lives in the sibling paideia-os repo).

set -euo pipefail

# Only enforce on PRs (not pushes).
if [ "${GITHUB_EVENT_NAME:-}" != "pull_request" ]; then
  echo "Not a PR; skipping design-doc lint."
  exit 0
fi

PR_NUMBER="${GITHUB_REF_NAME%/merge}"
PR_NUMBER="${PR_NUMBER%%/*}"

# Get the changed files via gh CLI (available in GitHub Actions runners).
changed_files=$(gh pr view "$PR_NUMBER" --json files -q '.files[].path')

# Filter to src/** changes under crates/.
touched_src=$(echo "$changed_files" | grep -E '^crates/[^/]+/src/' || true)

if [ -z "$touched_src" ]; then
  echo "No src/ changes; design-doc lint not applicable."
  exit 0
fi

echo "Changes detected in:"
echo "$touched_src" | sed 's/^/  /'
echo

# Get PR body and commit messages to scan.
pr_body=$(gh pr view "$PR_NUMBER" --json body -q '.body')
commit_msgs=$(gh pr view "$PR_NUMBER" --json commits -q '.commits[].messageBody' || true)
combined="$pr_body
$commit_msgs"

# Regex for the design-doc URL.
DESIGN_REGEX='paideia-os/paideia-os/(blob|tree)/[^ ]+/design/toolchain/'

if echo "$combined" | grep -qE "$DESIGN_REGEX"; then
  echo "✓ Design-doc URL referenced."
  exit 0
fi

# Allow explicit waiver.
if echo "$combined" | grep -qE '^Design-Doc-Waiver:'; then
  reason=$(echo "$combined" | grep -E '^Design-Doc-Waiver:' | head -1 | sed 's/^Design-Doc-Waiver:[[:space:]]*//')
  echo "⚠ Design-Doc-Waiver claimed: $reason"
  echo "  Flagged for monthly review."
  exit 0
fi

echo "✗ src/ changes require a reference to a design doc in paideia-os/paideia-os/design/toolchain/**"
echo "  Add a URL like https://github.com/paideia-os/paideia-os/blob/main/design/toolchain/<doc>.md"
echo "  to the PR body or a commit message, OR include 'Design-Doc-Waiver: <reason>' if no doc applies."
exit 1
