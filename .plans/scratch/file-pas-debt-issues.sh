#!/usr/bin/env bash
# .plans/scratch/file-pas-debt-issues.sh
#
# Draft: sub-issue filer for paideia-as#1396 (debt catalog).
# Not marked executable (`chmod +x` deliberately withheld) — Phase 2's
# umbrella-closer chmods and runs it after enumerating the nine unmarked
# runtime failures via `cargo test -p paideia-as --test build_emit`.
#
# Inputs:
#   .plans/scratch/pas-debt-catalog.tsv  — one row per PAS-DEBT id.
#     columns (tab-separated): id  title  priority  deps  bucket  size
#     Comment lines start with `#`. Blank lines skipped.
#
# Requires:
#   - gh authenticated against paideia-os/paideia-as
#   - repo checked out at PWD == paideia-as submodule root
#
# Idempotency: each row is a no-op if an issue with the same
# `[PAS-DEBT-B<bucket>-<mmm>]` prefix in its title already exists in any
# state (`gh issue list --state all --search "$id in:title"`).
#
# Concurrency: flocked so parallel invocations serialise cleanly.
#
# See design/paideia-as-debt-catalog.md §10 (filing order) and §11
# (per-item table + this excerpt).

set -euo pipefail

UMBRELLA=1396
TSV="${TSV:-.plans/scratch/pas-debt-catalog.tsv}"
LOCK="/tmp/pas-debt-file.lock"
MILESTONE="${MILESTONE:-next-wave}"

if [[ ! -r "$TSV" ]]; then
  echo "error: $TSV not readable. Materialise it from" \
       "design/paideia-as-debt-catalog.md §11 first." >&2
  exit 2
fi

exec 9>"$LOCK"
if ! flock -n 9; then
  echo "another filer holds $LOCK; exit." >&2
  exit 1
fi

file_one() {
  local id="$1" title="$2" priority="$3" deps="$4" bucket="$5" size="$6"

  # Resume support: skip if this id is already filed (any state).
  if gh issue list --state all --search "$id in:title" --json number \
       --limit 1 | grep -q '"number"'; then
    echo "skip: $id already filed"
    return 0
  fi

  local body
  body=$(cat <<EOF
Sub-issue of paideia-as#${UMBRELLA} (debt-catalog).

**Catalog entry:** \`${id}\` — Bucket ${bucket}, size ${size}.
**Priority:** ${priority}
**Depends on:** ${deps:-none}

See \`design/paideia-as-debt-catalog.md\` §${bucket} for the exact
site citation (file:line), the symptom description, and the recommended
fix-category taxonomy. This issue is the per-item sub-issue filing for
that entry; do not close paideia-as#${UMBRELLA} until every sub-issue
in the catalog has landed.
EOF
)

  gh issue create \
    --title "[${id}] ${title}" \
    --body "$body" \
    --label "debt,${priority}" \
    --milestone "$MILESTONE"
}

count=0
while IFS=$'\t' read -r id title priority deps bucket size; do
  [[ "$id" == \#* || -z "$id" ]] && continue
  file_one "$id" "$title" "$priority" "$deps" "$bucket" "$size"
  count=$((count + 1))
done < "$TSV"

echo "filed $count sub-issue(s) against paideia-as#${UMBRELLA}"
