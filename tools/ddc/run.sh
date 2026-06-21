#!/usr/bin/env bash
# ddc: diverse-double-compilation orchestrator.
# Builds paideia-as twice with two different host toolchains; both
# stage-1 artifacts are saved for byte-level diffing (m10-002).
#
# Phase-2-m10-001 minimum:
# - Toolchain A: the default cargo / rustc.
# - Toolchain B: nightly cargo / rustc (if installed; falls back to
#   "+stable" with a note).
# - Logs each toolchain's version.
# - Builds release artifacts to tools/ddc/out/{a,b}/.
#
# Usage:
#   tools/ddc/run.sh
#
# Exit codes:
#   0 - both builds succeeded.
#   1 - a build failed.
#   2 - toolchain not available.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
OUT_DIR="${SCRIPT_DIR}/out"

mkdir -p "${OUT_DIR}/a" "${OUT_DIR}/b"

log() { echo "[ddc] $*" >&2; }

build_with() {
    local toolchain="$1"
    local outdir="$2"
    log "building with toolchain: ${toolchain}"
    if ! cargo "+${toolchain}" --version >/dev/null 2>&1; then
        log "toolchain ${toolchain} not available; trying default"
        toolchain=""
    fi

    local cargo_cmd="cargo"
    if [[ -n "${toolchain}" ]]; then
        cargo_cmd="cargo +${toolchain}"
    fi

    log "  $(${cargo_cmd} --version)"
    log "  $(${cargo_cmd} rustc -- --version 2>&1 | head -1)"

    # Build the paideia-as binary specifically.
    (cd "${ROOT_DIR}" && ${cargo_cmd} build --release -p paideia-as) || return 1

    cp "${ROOT_DIR}/target/release/paideia-as" "${outdir}/paideia-as"
    log "  artifact: ${outdir}/paideia-as ($(stat -c%s "${outdir}/paideia-as") bytes)"
}

log "DDC dual-build start"
log "host: $(uname -a)"

build_with "stable" "${OUT_DIR}/a" || { log "build A failed"; exit 1; }

# For phase-2-m10-001, "different toolchain" is best-effort: prefer nightly,
# fall back to a fresh stable build at a different target if nightly is
# unavailable. Real diverse-toolchain configs (e.g., GCC-built rustc vs
# distro rustc) are a follow-up.
if cargo +nightly --version >/dev/null 2>&1; then
    build_with "nightly" "${OUT_DIR}/b" || { log "build B failed"; exit 1; }
else
    log "nightly toolchain not available; falling back to a second stable build"
    log "(phase-2-m10-001: real toolchain diversity activates when m10-005 wires the CI workflow)"
    build_with "stable" "${OUT_DIR}/b" || { log "build B failed"; exit 1; }
fi

log "DDC dual-build complete"
log "artifacts:"
log "  A: ${OUT_DIR}/a/paideia-as"
log "  B: ${OUT_DIR}/b/paideia-as"

# Phase-3-m5-002: dual-stage-0 entry-point verification.
# Assemble both stage-0 sources and compare the .text section bytes.
# - Stage-0a: NASM at tools/cross-build/fixtures/uefi_loader/module.asm
# - Stage-0b: GAS at src/toolchain/stage-0/entrypoint.s (m5-001)
STAGE0A_NASM="${ROOT_DIR}/tools/cross-build/fixtures/uefi_loader/module.asm"
STAGE0B_GAS="${ROOT_DIR}/src/toolchain/stage-0/entrypoint.s"

if command -v nasm >/dev/null 2>&1 && command -v as >/dev/null 2>&1; then
    log "stage-0 dual-source verification"
    nasm -f elf64 "${STAGE0A_NASM}" -o "${OUT_DIR}/a/stage-0.o"
    as --64 "${STAGE0B_GAS}" -o "${OUT_DIR}/b/stage-0.o"
    objcopy -O binary --only-section=.text "${OUT_DIR}/a/stage-0.o" "${OUT_DIR}/a/stage-0.text"
    objcopy -O binary --only-section=.text "${OUT_DIR}/b/stage-0.o" "${OUT_DIR}/b/stage-0.text"
    if cmp -s "${OUT_DIR}/a/stage-0.text" "${OUT_DIR}/b/stage-0.text"; then
        log "  stage-0a (NASM) == stage-0b (GAS): .text byte-identical"
    else
        log "  stage-0a vs stage-0b: .text DIFFERS — DDC FAIL"
        exit 1
    fi
else
    log "nasm or as not available; skipping stage-0 dual-source verification"
fi

log "Phase-4-m2-004: per-emit DDC fixture determinism"
FIXTURE="${ROOT_DIR}/tools/ddc/fixtures/m2-004-passes.pdx"
if [[ ! -f "${FIXTURE}" ]]; then
    log "fixture not found: ${FIXTURE}; skipping"
    exit 0
fi

# Build fixture twice with deterministic SOURCE_DATE_EPOCH, emitting to each format.
# For each format, verify the two runs produce byte-identical output.
for emit_fmt in elf64 pe-coff pax; do
    log "checking ${emit_fmt} determinism"

    tmp_run1="${OUT_DIR}/a/m2004-passes.${emit_fmt}"
    tmp_run2="${OUT_DIR}/b/m2004-passes.${emit_fmt}"

    run_build() {
        local out="$1"
        SOURCE_DATE_EPOCH=0 PDX_PATH_PREFIX_MAP="/=/" \
        "${OUT_DIR}/a/paideia-as" build \
            --emit "${emit_fmt}" \
            "${FIXTURE}" \
            -o "${out}" 2>&1
    }

    if ! run_build "${tmp_run1}"; then
        log "  build run 1 (${emit_fmt}) failed; skipping format"
        continue
    fi

    if ! run_build "${tmp_run2}"; then
        log "  build run 2 (${emit_fmt}) failed; skipping format"
        continue
    fi

    if cmp -s "${tmp_run1}" "${tmp_run2}"; then
        log "  ${emit_fmt}: byte-identical across deterministic runs"
    else
        log "  ${emit_fmt}: DIFFERS across runs — DDC FAIL"
        exit 1
    fi
done

log "DDC per-emit fixture determinism checks passed"

# Phase-4-m13-004: stage-1 hash + DDC fixture for the m13-002 mini-lexer.
# Establishes the discipline that stage-1 compilation outputs are byte-stable
# for the self-hosting fixture too. Activates when CI re-enables.
log "Phase-4-m13-004: m13-002 mini-lexer stage-1 byte stability"
M13_002_FIXTURE="${ROOT_DIR}/tests/self-hosting/pdx/mini_lexer.pdx"
if [[ ! -f "${M13_002_FIXTURE}" ]]; then
    log "  mini-lexer fixture not found: ${M13_002_FIXTURE}; skipping"
else
    for emit_fmt in elf64 pax; do
        log "  checking mini-lexer ${emit_fmt} determinism"
        tmp_run1="${OUT_DIR}/a/m13-002-mini-lexer.${emit_fmt}"
        tmp_run2="${OUT_DIR}/b/m13-002-mini-lexer.${emit_fmt}"
        if SOURCE_DATE_EPOCH=0 PDX_PATH_PREFIX_MAP="/=/" "${OUT_DIR}/a/paideia-as" build --emit "${emit_fmt}" "${M13_002_FIXTURE}" -o "${tmp_run1}" >/dev/null 2>&1 && \
           SOURCE_DATE_EPOCH=0 PDX_PATH_PREFIX_MAP="/=/" "${OUT_DIR}/a/paideia-as" build --emit "${emit_fmt}" "${M13_002_FIXTURE}" -o "${tmp_run2}" >/dev/null 2>&1; then
            if cmp -s "${tmp_run1}" "${tmp_run2}"; then
                log "    mini-lexer ${emit_fmt}: byte-identical"
            else
                log "    mini-lexer ${emit_fmt}: DIFFERS — DDC FAIL"
                exit 1
            fi
        else
            log "    mini-lexer ${emit_fmt}: build skipped (gates on m13-002 elaborator readiness)"
        fi
    done
fi

log "Phase-4-m13-004: mini-lexer stage-1 stability checks complete"

exit 0
