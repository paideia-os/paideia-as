#!/usr/bin/env bash
set -euo pipefail

# v0.9 GitHub bootstrap script
# Creates labels, milestones, and issues across paideia-as and paideia-os repos
# Idempotent: checks existence before creating

# repos
PA_REPO="paideia-os/paideia-as"
OS_REPO="paideia-os/paideia-os"

# Colors
BLUE='\033[0;34m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

log() {
  echo -e "${BLUE}[bootstrap]${NC} $*"
}

success() {
  echo -e "${GREEN}[✓]${NC} $*"
}

step_wait() {
  local count=$1
  if (( count % 30 == 0 )); then
    log "Sleeping 30s after $count operations..."
    sleep 30
  fi
}

# Check if label exists
label_exists() {
  local repo=$1
  local label=$2
  gh label list --repo "$repo" --search "$label" --json name -q '.[0].name' 2>/dev/null | grep -q "^${label}$" && return 0 || return 1
}

# Check if milestone exists
milestone_exists() {
  local repo=$1
  local milestone=$2
  gh api "repos/$repo/milestones" --paginate -q ".[] | select(.title==\"$milestone\") | .title" 2>/dev/null | grep -q "^${milestone}$" && return 0 || return 1
}

# Check if issue exists by title
issue_exists() {
  local repo=$1
  local title=$2
  gh issue list --repo "$repo" --search "in:title \"$title\"" --json number -q '.[0].number' 2>/dev/null | grep -q . && return 0 || return 1
}

# ============ STEP 1: Create labels ============
log "Creating labels..."

# Shared label: v0.9
for repo in "$PA_REPO" "$OS_REPO"; do
  if ! label_exists "$repo" "v0.9"; then
    gh label create "v0.9" --color "00BCD4" --description "v0.9 round bookmark" --repo "$repo"
    success "Created label v0.9 on $repo"
  else
    success "Label v0.9 already exists on $repo"
  fi
done

# paideia-as only
if ! label_exists "$PA_REPO" "pa9-substrate"; then
  gh label create "pa9-substrate" --color "5C6BC0" --description "v0.9 paideia-as substrate gap fixes (P0158, P0211, P3-m2-003)" --repo "$PA_REPO"
  success "Created label pa9-substrate"
else
  success "Label pa9-substrate already exists"
fi

# paideia-os only
declare -a OS_LABELS=(
  "r7-rewrite:#26A69A:v0.9 — 5 paideia-os pseudocode-stub rewrites to real .pdx"
  "r7-unquarantine:#7CB342:v0.9 — single batch issue covering the 7-file unquarantine at checkpoint 2"
  "r6.5-resume:#FB8C00:v0.9 — R6.5 backlog issues resumed after checkpoint 2"
  "d7-resume:#8E24AA:v0.9 — D7 driver-layer backlog issues resumed after checkpoint 2"
)

for label_spec in "${OS_LABELS[@]}"; do
  name=$(echo "$label_spec" | cut -d: -f1)
  color=$(echo "$label_spec" | cut -d: -f2 | tr -d '#')
  desc=$(echo "$label_spec" | cut -d: -f3)

  if ! label_exists "$OS_REPO" "$name"; then
    gh label create "$name" --color "$color" --description "$desc" --repo "$OS_REPO"
    success "Created label $name"
  else
    success "Label $name already exists"
  fi
done

log "Labels created."
step_wait 6

# ============ STEP 2: Create milestones ============
log "Creating milestones..."

# paideia-as milestones
declare -a PA_MILESTONES=(
  "pa9-substrate:v0.9: 3 paideia-as gap fixes. Unblocks checkpoint-2 unquarantine."
  "pa9-closure:v0.9 closure: version bump + tag + retrospective."
)

for milestone_spec in "${PA_MILESTONES[@]}"; do
  slug=$(echo "$milestone_spec" | cut -d: -f1)
  title=$(echo "$milestone_spec" | cut -d: -f2)

  if milestone_exists "$PA_REPO" "$slug"; then
    success "Milestone $slug already exists on paideia-as"
  else
    gh api repos/"$PA_REPO"/milestones -f title="$slug" -f description="$title" > /dev/null
    success "Created milestone $slug on paideia-as"
  fi
done

# paideia-os milestones
declare -a OS_MILESTONES=(
  "r7-rewrite-checkpoint-2:v0.9: 5 IPC pseudocode rewrites to real .pdx."
  "r7-unquarantine:v0.9: batch unquarantine of 8 checkpoint-2 files."
  "r6.5-resume:v0.9 / post-unquarantine: resume R6.5 IRQ + APIC + timer backlog."
  "d7-resume:v0.9 / post-unquarantine: resume D7 driver framework backlog."
)

for milestone_spec in "${OS_MILESTONES[@]}"; do
  slug=$(echo "$milestone_spec" | cut -d: -f1)
  title=$(echo "$milestone_spec" | cut -d: -f2)

  if milestone_exists "$OS_REPO" "$slug"; then
    success "Milestone $slug already exists on paideia-os"
  else
    gh api repos/"$OS_REPO"/milestones -f title="$slug" -f description="$title" > /dev/null
    success "Created milestone $slug on paideia-os"
  fi
done

log "Milestones created."
step_wait 4

# ============ STEP 3: Create issues ============
log "Creating issues..."

# paideia-as issues (m1 - substrate gaps)
PA_ISSUES=(
  "PA9-m1-001:parser + elaborator: accept bare if as unit-typed statement:Bare-if statement context fixes P0158 elaboration gap. Blocks slab.pdx unquarantine.:(a) parse_control.rs parse_if dispatch, (b) elaborator unit-coercion for bare-if, (c) parser regression tests, (d) elaborator tests|S|pa9-substrate|P0158|core/cap/slab.pdx"
  "PA9-m1-002:elaborator: materialise nested array-repeat in struct field initialisers:Nested array-repeat materialisation fixes P0211 elaboration gap. Blocks channel.pdx unquarantine.:(a) lower.rs recursive materialisation pass, (b) RecordLit + ArrayRepeat nesting, (c) elaborator tests|M|pa9-substrate|P0211|core/ipc/channel.pdx"
  "PA9-m1-003:encoder: extend mov dispatch to general base+index*scale+disp SIB form:General SIB form encoder extension fixes byte-emit gap. Blocks enqueue.pdx unquarantine.:(a) encode_mov 3 new arms, (b) disp=0 and disp=N cases, (c) encoder tests|S|pa9-substrate|P3-m2-003|core/sched/enqueue.pdx"
)

for issue_spec in "${PA_ISSUES[@]}"; do
  task_id=$(echo "$issue_spec" | cut -d: -f1)
  title=$(echo "$issue_spec" | cut -d: -f2)
  summary=$(echo "$issue_spec" | cut -d: -f3)
  ac=$(echo "$issue_spec" | cut -d: -f4)
  size=$(echo "$issue_spec" | cut -d: -f5)
  milestone=$(echo "$issue_spec" | cut -d: -f6)
  surface=$(echo "$issue_spec" | cut -d: -f7)
  blocked_file=$(echo "$issue_spec" | cut -d: -f8)

  full_title="$task_id: $title"

  if ! issue_exists "$PA_REPO" "$full_title"; then
    body=$(cat <<EOF
## Summary
$summary

## Acceptance criteria
$ac

## Files
See osarch plan v0.9-osarch-plan.md §3 PA9-m1-* section.

## Dependencies
None directly.

## Estimated size
$size

## Milestone
$milestone

## Unblocks paideia-os file(s)
$blocked_file

## Surfaced by
paideia-as@v0.8.0

## Definition of done
Verified by running the fixture tests and confirming the named file unquarantines cleanly.
EOF
)
    gh issue create \
      --repo "$PA_REPO" \
      --title "$full_title" \
      --body "$body" \
      --label "v0.9,pa9-substrate,enhancement,type:feature" \
      --milestone "$milestone" \
      > /dev/null
    success "Created issue: $full_title"
  else
    success "Issue already exists: $full_title"
  fi
  step_wait 3
done

# paideia-os rewrite issues (m2)
OS_REWRITE_ISSUES=(
  "R7-rewrite-001:Rewrite core/ipc/slots.pdx from pseudocode to real .pdx:Translate slots.pdx from bare module syntax to real module = structure form.:(a) file rewritten in .quarantine/src/kernel/core/ipc/slots.pdx, (b) paideia-as check exits 0, (c) constant values preserved, (d) build verified|XS|r7-rewrite-checkpoint-2|.quarantine/src/kernel/core/ipc/slots.pdx"
  "R7-rewrite-002:Rewrite core/ipc/allocator.pdx from pseudocode to real .pdx:Translate allocator.pdx from bare module syntax to real module = structure form.:(a) file rewritten, (b) paideia-as check exits 0, (c) 3 constants preserved, (d) build verified|XS|r7-rewrite-checkpoint-2|.quarantine/src/kernel/core/ipc/allocator.pdx"
  "R7-rewrite-003:Rewrite core/ipc/dispatch.pdx from pseudocode to real .pdx:Translate dispatch.pdx from bare module syntax to real module = structure form.:(a) file rewritten, (b) paideia-as check exits 0, (c) 3 constants preserved, (d) build verified|XS|r7-rewrite-checkpoint-2|.quarantine/src/kernel/core/ipc/dispatch.pdx"
  "R7-rewrite-004:Rewrite core/ipc/mpsc_lock.pdx from pseudocode to real .pdx:Translate mpsc_lock.pdx from bare module syntax to real module = structure form.:(a) file rewritten, (b) paideia-as check exits 0, (c) 2 constants preserved, (d) build verified|XS|r7-rewrite-checkpoint-2|.quarantine/src/kernel/core/ipc/mpsc_lock.pdx"
  "R7-rewrite-005:Rewrite core/ipc/destroy_channel.pdx from pseudocode to real .pdx:Translate destroy_channel.pdx from bare module syntax to real module = structure form.:(a) file rewritten, (b) paideia-as check exits 0, (c) 1 constant preserved, (d) build verified|XS|r7-rewrite-checkpoint-2|.quarantine/src/kernel/core/ipc/destroy_channel.pdx"
)

for issue_spec in "${OS_REWRITE_ISSUES[@]}"; do
  task_id=$(echo "$issue_spec" | cut -d: -f1)
  title=$(echo "$issue_spec" | cut -d: -f2)
  summary=$(echo "$issue_spec" | cut -d: -f3)
  ac=$(echo "$issue_spec" | cut -d: -f4)
  size=$(echo "$issue_spec" | cut -d: -f5)
  milestone=$(echo "$issue_spec" | cut -d: -f6)
  blocked_file=$(echo "$issue_spec" | cut -d: -f7)

  full_title="$task_id: $title"

  if ! issue_exists "$OS_REPO" "$full_title"; then
    body=$(cat <<EOF
## Summary
$summary

## Acceptance criteria
$ac

## Files
PaideiaOS/$blocked_file (rewritten in place)

## Dependencies
None.

## Estimated size
$size

## Milestone
$milestone

## Unblocks paideia-os file(s)
$blocked_file

## Surfaced by
.plans/v0.9-quarantine-diagnosis.md

## Definition of done
File rewritten cleanly; paideia-as check + build both exit 0.
EOF
)
    gh issue create \
      --repo "$OS_REPO" \
      --title "$full_title" \
      --body "$body" \
      --label "v0.9,r7-rewrite,enhancement,type:feature" \
      --milestone "$milestone" \
      > /dev/null
    success "Created issue: $full_title"
  else
    success "Issue already exists: $full_title"
  fi
  step_wait 3
done

# paideia-as version bump issue (m3)
PA_VERSION_TITLE="PA9-m3-001: Bump workspace.version to 0.9.0; tag v0.9.0; bump PaideiaOS submodule"
if ! issue_exists "$PA_REPO" "$PA_VERSION_TITLE"; then
  pa_version_body=$(cat <<'EOF'
## Summary
Version bump: workspace.version 0.8.0 -> 0.9.0, tag v0.9.0, update CHANGELOG, bump PaideiaOS submodule to v0.9.0.

## Acceptance criteria
- [ ] Cargo.toml workspace.version reads 0.9.0
- [ ] CHANGELOG.md has ## v0.9.0 entry covering the 3 substrate gaps
- [ ] git tag v0.9.0 annotated and pushed
- [ ] PaideiaOS submodule pin updated to v0.9.0 commit
- [ ] tools/find-paideia-as.sh strict-version check passes
- [ ] Cross-repo canaries green

## Files
paideia-as/Cargo.toml, paideia-as/CHANGELOG.md, PaideiaOS/tools/paideia-as (submodule)

## Dependencies
PA9-m1-001, PA9-m1-002, PA9-m1-003 (all must land first)

## Estimated size
XS

## Milestone
pa9-closure

## Unblocks paideia-os file(s)
(gating m4 unquarantine)

## Surfaced by
v0.9 round plan

## Definition of done
v0.9.0 tag pushed; submodule pin updated; find-paideia-as.sh passes.
EOF
)
  gh issue create \
    --repo "$PA_REPO" \
    --title "$PA_VERSION_TITLE" \
    --body "$pa_version_body" \
    --label "v0.9,type:feature,enhancement" \
    --milestone "pa9-closure" \
    > /dev/null
  success "Created issue: PA9-m3-001"
else
  success "Issue already exists: PA9-m3-001"
fi
step_wait 1

# paideia-os batch unquarantine issue (m4)
OS_UNQ_TITLE="R7-unquarantine-001: Batch-unquarantine 8 checkpoint-2 files; build.sh + smoke green"
if ! issue_exists "$OS_REPO" "$OS_UNQ_TITLE"; then
  os_unq_body=$(cat <<'EOF'
## Summary
Batch-move 8 quarantined files from .quarantine/src/kernel/ back to src/kernel/ and verify kernel builds end-to-end.

## Acceptance criteria
- [ ] All 8 .pdx files moved back to src/kernel/ (slab, channel, slots, allocator, dispatch, mpsc_lock, destroy_channel, enqueue)
- [ ] .quarantine/src/kernel/ empty
- [ ] ./tools/build.sh exits 0 and produces build/kernel.elf
- [ ] ./tools/run-smoke.sh green; UART banner appears
- [ ] SARIF files from v0.8.0 deleted
- [ ] Commit names the 8 unquarantined files explicitly
- [ ] Cross-repo canaries green

## Files
8 .pdx moves, 8 .sarif.json deletions, PaideiaOS/STATUS.md update

## Dependencies
PA9-m1-001, PA9-m1-002, PA9-m1-003, R7-rewrite-001..005, PA9-m3-001 (all must land first)

## Estimated size
XS

## Milestone
r7-unquarantine

## Unblocks paideia-os file(s)
core/cap/slab.pdx
core/ipc/channel.pdx
core/ipc/slots.pdx
core/ipc/allocator.pdx
core/ipc/dispatch.pdx
core/ipc/mpsc_lock.pdx
core/ipc/destroy_channel.pdx
core/sched/enqueue.pdx

## Surfaced by
v0.9 round plan checkpoint 2

## Definition of done
All 8 files unquarantined; build.sh + smoke green; .quarantine/ empty.
EOF
)
  gh issue create \
    --repo "$OS_REPO" \
    --title "$OS_UNQ_TITLE" \
    --body "$os_unq_body" \
    --label "v0.9,r7-unquarantine,enhancement,type:feature" \
    --milestone "r7-unquarantine" \
    > /dev/null
  success "Created issue: R7-unquarantine-001"
else
  success "Issue already exists: R7-unquarantine-001"
fi
step_wait 1

# paideia-as retrospective issue (m5)
PA_RETRO_TITLE="PA9-m5-001: v0.9 retrospective + signal R6.5/D7 resume"
if ! issue_exists "$PA_REPO" "$PA_RETRO_TITLE"; then
  pa_retro_body=$(cat <<'EOF'
## Summary
Write v0.9 retrospective covering the 3 substrate fixes, 5 rewrites, batch unquarantine, and R6.5/D7 resume signal.

## Acceptance criteria
- [ ] .plans/v0.9-retrospective.md exists (~60 lines)
- [ ] Covers (a) 3 substrate fixes, (b) 5 rewrites, (c) m4 build/smoke verdict, (d) v1.0 deferred items + rationale
- [ ] PaideiaOS STATUS.md updated with 8 unquarantined files + R6.5/D7 resume signal

## Files
paideia-as/.plans/v0.9-retrospective.md, PaideiaOS/STATUS.md

## Dependencies
R7-unquarantine-001 (m4 must close first)

## Estimated size
XS

## Milestone
pa9-closure

## Unblocks paideia-os file(s)
(closure only)

## Surfaced by
v0.9 round plan

## Definition of done
Retrospective written; STATUS.md updated; R6.5/D7 resume signaled.
EOF
)
  gh issue create \
    --repo "$PA_REPO" \
    --title "$PA_RETRO_TITLE" \
    --body "$pa_retro_body" \
    --label "v0.9,enhancement,type:feature" \
    --milestone "pa9-closure" \
    > /dev/null
  success "Created issue: PA9-m5-001"
else
  success "Issue already exists: PA9-m5-001"
fi
step_wait 1

log "All issues created."

# ============ STEP 4: Verify counts ============
log "Verifying issue counts..."

pa_count=$(gh issue list --repo "$PA_REPO" --label v0.9 --state open --json number -q . | wc -l)
os_count=$(gh issue list --repo "$OS_REPO" --label v0.9 --state open --json number -q . | wc -l)

log "paideia-as v0.9 issues: $pa_count (expect 5: PA9-m1-001, 002, 003, m3-001, m5-001)"
log "paideia-os v0.9 issues: $os_count (expect 6: R7-rewrite-001..005, R7-unquarantine-001)"

total=$((pa_count + os_count))
log "Total v0.9 issues: $total (expect 11)"

if [ "$pa_count" -eq 5 ] && [ "$os_count" -eq 6 ]; then
  success "Issue counts verified!"
else
  echo "WARN: Issue count mismatch. paideia-as=$pa_count (expect 5), paideia-os=$os_count (expect 6)"
fi

log "Bootstrap complete."
