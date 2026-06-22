#!/usr/bin/env bash
set -euo pipefail

# PA8-m1-001: Verify alleged PA7C-m5-002 regression
# Outcomes: (a) Real regression + bisect, (b) Misdiagnosis (clean build), (c) No regression (checkpoint-2 evidence)

PAIDEIA_OS_REPO="${PAIDEIA_OS_REPO:-/home/snunez/Development/PaideiaOS}"
PAIDEIA_AS_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Snapshot state
SAVED_PAIDEIA_AS_BRANCH=""
SAVED_PAIDEIA_OS_SUBMODULE_SHA=""
REGRESS_DECISION="UNKNOWN"
BISECT_SHA=""

# Cleanup on exit
cleanup() {
    local exit_code=$?
    if [[ -n "$SAVED_PAIDEIA_AS_BRANCH" ]] && [[ "$SAVED_PAIDEIA_AS_BRANCH" != "detached" ]]; then
        (cd "$PAIDEIA_AS_REPO" && git checkout "$SAVED_PAIDEIA_AS_BRANCH" 2>/dev/null || true)
    fi
    if [[ -n "$SAVED_PAIDEIA_OS_SUBMODULE_SHA" ]]; then
        (cd "$PAIDEIA_OS_REPO/tools/paideia-as" && git checkout "$SAVED_PAIDEIA_OS_SUBMODULE_SHA" 2>/dev/null || true)
    fi
    return $exit_code
}
trap cleanup EXIT

# ========== PREFLIGHT ==========
echo "[PREFLIGHT] Checking repo cleanliness..." >&2

cd "$PAIDEIA_AS_REPO"
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "ERROR: paideia-as working tree is dirty. Commit or stash changes." >&2
    exit 1
fi

cd "$PAIDEIA_OS_REPO"
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "ERROR: PaideiaOS working tree is dirty. Commit or stash changes." >&2
    exit 1
fi

# ========== SNAPSHOT ==========
echo "[SNAPSHOT] Saving current state..." >&2
cd "$PAIDEIA_AS_REPO"
SAVED_PAIDEIA_AS_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [[ "$SAVED_PAIDEIA_AS_BRANCH" == "HEAD" ]]; then
    SAVED_PAIDEIA_AS_BRANCH="detached"
fi

cd "$PAIDEIA_OS_REPO/tools/paideia-as"
SAVED_PAIDEIA_OS_SUBMODULE_SHA=$(git rev-parse HEAD)
echo "  paideia-as branch: $SAVED_PAIDEIA_AS_BRANCH" >&2
echo "  PaideiaOS submodule @ $SAVED_PAIDEIA_OS_SUBMODULE_SHA" >&2

# ========== BUILD PAIDEIA-AS @ v0.7.0 ==========
echo "[BUILD] Testing paideia-as v0.7.0..." >&2
cd "$PAIDEIA_AS_REPO"
git fetch --tags 2>/dev/null || true
git checkout v0.7.0 2>&1 | grep -v "^Note:" || true

if ! cargo build --release -p paideia-as 2>&1 | tail -5; then
    echo "ERROR: Failed to build paideia-as v0.7.0." >&2
    exit 1
fi

VERSION_OUTPUT=$("$PAIDEIA_AS_REPO/target/release/paideia-as" --version 2>&1 || echo "unknown")
if ! echo "$VERSION_OUTPUT" | grep -q "0\.7\.0"; then
    echo "WARNING: --version did not report 0.7.0: $VERSION_OUTPUT" >&2
fi
echo "  paideia-as v0.7.0 built successfully" >&2

# ========== UPDATE PAIDEIA-OS SUBMODULE POINTER (WORKTREE ONLY) ==========
echo "[UPDATE] Updating PaideiaOS submodule pointer (worktree only)..." >&2
cd "$PAIDEIA_OS_REPO/tools/paideia-as"
git fetch --tags 2>/dev/null || true
git checkout v0.7.0 2>&1 | grep -v "^Note:" || true
cargo build --release -p paideia-as 2>&1 | tail -3 || true
echo "  Submodule @ v0.7.0 (worktree only - not staged)" >&2

# ========== BUILD PAIDEIA-OS ==========
echo "[BUILD] Building PaideiaOS with updated paideia-as..." >&2
cd "$PAIDEIA_OS_REPO"
rm -rf build 2>/dev/null || true

BUILDLOG="${PAIDEIA_OS_REPO}/.plans/pa8-m1-001-build.log"
mkdir -p "$(dirname "$BUILDLOG")"

BUILD_EXIT=0
./tools/build.sh > "$BUILDLOG" 2>&1 || BUILD_EXIT=$?

# Check build result
KERNEL_ELF="${PAIDEIA_OS_REPO}/build/kernel/kernel.elf"
if [[ $BUILD_EXIT -eq 0 ]] && [[ -f "$KERNEL_ELF" ]] && [[ $(stat -f%z "$KERNEL_ELF" 2>/dev/null || stat -c%s "$KERNEL_ELF" 2>/dev/null || echo 0) -gt 4096 ]]; then
    REGRESS_DECISION="NO REGRESSION"
    echo "[DECISION] Build succeeded and kernel.elf > 4096 bytes → NO REGRESSION" >&2
else
    REGRESS_DECISION="REGRESSION CONFIRMED"
    echo "[DECISION] Build failed or kernel.elf invalid → REGRESSION CONFIRMED" >&2
    echo "[BISECT] Running bisect to locate breaking commit..." >&2

    BISECT_LOG="${PAIDEIA_OS_REPO}/.plans/pa8-m1-001-bisect.log"

    # Create ephemeral bisect wrapper
    BISECT_WRAPPER=$(mktemp)
    cat > "$BISECT_WRAPPER" << 'BISECT_SCRIPT'
#!/usr/bin/env bash
set -euo pipefail

cd "$PAIDEIA_OS_REPO"
cd tools/paideia-as
git checkout "$1" 2>&1 | grep -v "^Note:" || true
cargo build --release -p paideia-as 2>&1 | tail -1 || true

cd "$PAIDEIA_OS_REPO"
rm -rf build 2>/dev/null || true
./tools/build.sh > /dev/null 2>&1 || exit 1

KERNEL_ELF="$PAIDEIA_OS_REPO/build/kernel/kernel.elf"
if [[ -f "$KERNEL_ELF" ]] && [[ $(stat -f%z "$KERNEL_ELF" 2>/dev/null || stat -c%s "$KERNEL_ELF" 2>/dev/null || echo 0) -gt 4096 ]]; then
    exit 0  # good
else
    exit 1  # bad
fi
BISECT_SCRIPT
    chmod +x "$BISECT_WRAPPER"

    (
        cd "$PAIDEIA_OS_REPO/tools/paideia-as"
        git bisect start v0.7.0 4059d87 >> "$BISECT_LOG" 2>&1 || true
        git bisect run "$BISECT_WRAPPER" >> "$BISECT_LOG" 2>&1 || true
        BISECT_SHA=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
        git bisect reset >> "$BISECT_LOG" 2>&1 || true
    ) || true

    rm -f "$BISECT_WRAPPER"
    echo "  Bisect results saved to $BISECT_LOG" >&2
fi

# ========== DECISION DOCUMENT ==========
DECISION_DOC="${PAIDEIA_OS_REPO}/.plans/pa8-m1-001-decision.md"
mkdir -p "$(dirname "$DECISION_DOC")"

cat > "$DECISION_DOC" << EOF
# PA8-m1-001: PA7C-m5-002 Regression Verification

**Date**: $(date -u +"%Y-%m-%dT%H:%M:%SZ")
**Verification Result**: $REGRESS_DECISION

## Summary

Verified PA7C-m5-002 regression claim by building paideia-as @ v0.7.0 against PaideiaOS submodule.

## Evidence

- paideia-as tag: v0.7.0
- paideia-as commit: $(cd "$PAIDEIA_AS_REPO" && git describe --tags --always 2>/dev/null || echo "unknown")
- PaideiaOS submodule prior: $SAVED_PAIDEIA_OS_SUBMODULE_SHA
- Build log: .plans/pa8-m1-001-build.log

## Result

$REGRESS_DECISION

$(if [[ "$REGRESS_DECISION" == "REGRESSION CONFIRMED" ]]; then
    echo "Bisect identified breaking commit: $BISECT_SHA"
    echo "See .plans/pa8-m1-001-bisect.log for full bisect output."
    echo ""
    echo "**Action**: File PA8-m1-001a to investigate root cause."
    echo "**Recommendation**: Keep submodule pinned at 4059d87 pending fix."
else
    echo "Build completed successfully with kernel.elf > 4096 bytes."
    echo ""
    echo "**Possible causes of misdiagnosis**:"
    echo "- Previous test environment issue"
    echo "- Workerbee checkpoint-2 test was not from v0.7.0"
    echo "- Transient build state fixed by clean rebuild"
fi)

## Next Steps

$(if [[ "$REGRESS_DECISION" == "NO REGRESSION" ]]; then
    echo "- Bump submodule to v0.7.0: \`cd $PAIDEIA_OS_REPO/tools/paideia-as && git checkout v0.7.0 && cd $PAIDEIA_OS_REPO && git add tools/paideia-as && git commit -m 'Bump paideia-as to v0.7.0'\`"
else
    echo "- Investigate commit $BISECT_SHA"
    echo "- File follow-up issue PA8-m1-001a"
fi)
EOF

echo "  Decision doc written to $DECISION_DOC" >&2

# ========== FINAL OUTPUT ==========
echo "$REGRESS_DECISION"
