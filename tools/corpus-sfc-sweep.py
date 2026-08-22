#!/usr/bin/env python3
"""Corpus S/F/C diagnostic sweep — deferred artefact from issue #1245.

Walks every `.pdx` fixture under the configured roots (default: `examples/`
and `tests/`), invokes `paideia-as build --emit placeholder --sarif <tmp>`
on each, and aggregates the Substructural (S09xx-S10xx), Effect-row
(F11xx-F12xx), and Capability (C13xx-C14xx) diagnostic firings.

Purpose
-------

Per design/paideia-as/non-milestone-issue-1237-cmd-build-root-walker.md
§4.2 (and the follow-up captured in GitHub issue #1245), the root-walker
fix (#1237) surfaces LinearityWalker / EffectRowWalker / CapWalker
correctly, but their firing is dormant while Phase-1 lowering sets
`lin_class = LinClass::Unrestricted` everywhere and the CLI does not
inject `perform_ops` / `handle_effects` / `lambda_declared` payloads
(only test fixtures do). Once the m3/m5 payload wiring lands, this
script drives the corpus sweep the issue's acceptance criteria call for:

  1. Walk the corpus (`examples/*.pdx` + `tests/**/*.pdx`).
  2. Count fired S/F/C diagnostics per code (S0900 family, F1100+, C1300).
  3. Report a table so newly-eligible codes are visible for authoring
     expect-failure fixtures + per-code assertions.

Today, with payload wiring not yet in place, running this script yields
zero S/F/C firings across the corpus — the expected pre-wiring baseline.
Once payload wiring lands, re-run to surface the first eligible diagnostics.

Usage
-----

  tools/corpus-sfc-sweep.py                        # default sweep
  tools/corpus-sfc-sweep.py --roots examples       # examples only
  tools/corpus-sfc-sweep.py --jobs 8               # parallel
  tools/corpus-sfc-sweep.py --json out.json        # machine-readable
  tools/corpus-sfc-sweep.py --binary target/debug/paideia-as
  tools/corpus-sfc-sweep.py --verbose              # per-file details

Exit code:
  0 — sweep completed (regardless of firing counts)
  2 — `paideia-as` binary not found (needs `cargo build`)
  3 — user-supplied path invalid
"""

from __future__ import annotations

import argparse
import concurrent.futures
import dataclasses
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Iterable

# Categories the issue's acceptance criteria enumerate. Any diagnostic with
# a ruleId starting with S/F/C is aggregated; the finer per-code counters
# below drive the newly-eligible surface table.
S_F_C_LETTERS = ("S", "F", "C")

# Diagnostic codes explicitly mentioned in issue #1245's acceptance
# criteria (§1 "each newly-eligible diagnostic"). Reported prominently.
FEATURED_CODES = (
    "S0900", "S0901", "S0902", "S0904", "S0907",   # LinearityWalker
    "F1100", "F1101", "F1105", "F1106",             # EffectRowWalker
    "C1300",                                         # CapWalker
)

RULE_ID_RE = re.compile(r"^([SFC])(\d{4})$")


@dataclasses.dataclass
class FixtureResult:
    """Per-fixture sweep outcome."""
    path: Path
    codes: dict[str, int]        # ruleId → count (S/F/C only)
    invoked: bool                 # True if `paideia-as` was invoked
    sarif_parsed: bool            # True if SARIF was produced + parsed
    exit_code: int | None         # `paideia-as` exit code (None if skipped)
    error: str | None             # error message if anything went wrong


def find_binary(explicit: str | None) -> Path:
    """Locate the `paideia-as` binary.

    Preference order: explicit CLI flag > release > debug. Exits 2 with a
    helpful message if none are present.
    """
    if explicit:
        p = Path(explicit)
        if p.is_file():
            return p
        print(f"error: --binary {explicit} not found", file=sys.stderr)
        sys.exit(2)

    for candidate in ("target/release/paideia-as", "target/debug/paideia-as"):
        p = Path(candidate)
        if p.is_file():
            return p

    print(
        "error: paideia-as binary not found; run `cargo build --release -p paideia-as` first",
        file=sys.stderr,
    )
    sys.exit(2)


def collect_fixtures(roots: Iterable[Path]) -> list[Path]:
    """Enumerate every .pdx file below the given roots, sorted for stable output."""
    fixtures: set[Path] = set()
    for root in roots:
        if not root.exists():
            print(f"warning: root {root} does not exist, skipping", file=sys.stderr)
            continue
        for path in root.rglob("*.pdx"):
            fixtures.add(path)
    return sorted(fixtures)


def sweep_fixture(binary: Path, fixture: Path, timeout: float) -> FixtureResult:
    """Invoke `paideia-as build --emit placeholder --sarif <tmp>` and parse the SARIF."""
    codes: dict[str, int] = {}

    with tempfile.NamedTemporaryFile(
        prefix="paideia-sfc-", suffix=".sarif.json", delete=False
    ) as tmp:
        sarif_path = Path(tmp.name)

    try:
        try:
            proc = subprocess.run(
                [
                    str(binary),
                    "build",
                    "--emit",
                    "placeholder",
                    "--sarif",
                    str(sarif_path),
                    str(fixture),
                ],
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return FixtureResult(
                path=fixture,
                codes=codes,
                invoked=True,
                sarif_parsed=False,
                exit_code=None,
                error=f"timeout after {timeout}s",
            )
        except FileNotFoundError as exc:
            return FixtureResult(
                path=fixture,
                codes=codes,
                invoked=False,
                sarif_parsed=False,
                exit_code=None,
                error=f"binary not found: {exc}",
            )

        if not sarif_path.exists() or sarif_path.stat().st_size == 0:
            # `paideia-as` may not have progressed far enough to emit SARIF
            # (e.g., unreadable input); the exit-code + stderr snippet suffice.
            return FixtureResult(
                path=fixture,
                codes=codes,
                invoked=True,
                sarif_parsed=False,
                exit_code=proc.returncode,
                error=(proc.stderr or "").strip().splitlines()[-1] if proc.stderr else None,
            )

        try:
            sarif = json.loads(sarif_path.read_text())
        except json.JSONDecodeError as exc:
            return FixtureResult(
                path=fixture,
                codes=codes,
                invoked=True,
                sarif_parsed=False,
                exit_code=proc.returncode,
                error=f"invalid SARIF: {exc}",
            )

        for run in sarif.get("runs", []):
            for result in run.get("results", []):
                rule_id = result.get("ruleId", "")
                if RULE_ID_RE.match(rule_id) and rule_id[0] in S_F_C_LETTERS:
                    codes[rule_id] = codes.get(rule_id, 0) + 1

        return FixtureResult(
            path=fixture,
            codes=codes,
            invoked=True,
            sarif_parsed=True,
            exit_code=proc.returncode,
            error=None,
        )
    finally:
        try:
            sarif_path.unlink()
        except FileNotFoundError:
            pass


def render_report(
    results: list[FixtureResult],
    featured_only: bool,
    verbose: bool,
) -> None:
    """Human-readable report to stdout."""
    totals: dict[str, int] = {}
    per_file_hits: list[FixtureResult] = []
    invocation_failures: list[FixtureResult] = []

    for r in results:
        for code, count in r.codes.items():
            totals[code] = totals.get(code, 0) + count
        if r.codes:
            per_file_hits.append(r)
        if not r.sarif_parsed and (r.error or r.exit_code not in (None, 0, 1, 2)):
            invocation_failures.append(r)

    print()
    print("=" * 72)
    print("Corpus S/F/C sweep — issue #1245 deferred artefact")
    print("=" * 72)
    print(f"Fixtures scanned:      {len(results)}")
    print(f"Fixtures with S/F/C:   {len(per_file_hits)}")
    print(f"Invocation failures:   {len(invocation_failures)}")
    print()

    print("Featured codes (acceptance-criteria enumerated in #1245):")
    for code in FEATURED_CODES:
        print(f"  {code}: {totals.get(code, 0)}")
    print()

    other_codes = sorted(c for c in totals if c not in FEATURED_CODES)
    if other_codes:
        print("Other S/F/C codes observed:")
        for code in other_codes:
            print(f"  {code}: {totals[code]}")
        print()

    if per_file_hits:
        print("Per-fixture breakdown (fixtures with ≥1 S/F/C diagnostic):")
        for r in per_file_hits:
            bits = ", ".join(f"{c}={n}" for c, n in sorted(r.codes.items()))
            print(f"  {r.path}: {bits}")
        print()
    else:
        print(
            "No S/F/C diagnostics fired across the corpus.\n"
            "  This is the expected pre-wiring baseline — LinearityWalker /\n"
            "  EffectRowWalker / CapWalker traverse the IR (per #1237), but\n"
            "  diagnostics stay dormant until Phase-1 lowering ships\n"
            "  syntax-driven lin_class assignment and cmd_build injects\n"
            "  perform_ops / handle_effects / lambda_declared payloads.\n"
            "  Re-run this sweep once m3/m5 payload wiring lands.\n"
        )

    if invocation_failures and verbose:
        print("Invocation failures (verbose):")
        for r in invocation_failures:
            print(f"  {r.path}: exit={r.exit_code} error={r.error}")
        print()


def write_json(path: Path, results: list[FixtureResult]) -> None:
    """Machine-readable sweep summary."""
    payload = {
        "issue": 1245,
        "featured_codes": list(FEATURED_CODES),
        "fixtures": [
            {
                "path": str(r.path),
                "codes": r.codes,
                "invoked": r.invoked,
                "sarif_parsed": r.sarif_parsed,
                "exit_code": r.exit_code,
                "error": r.error,
            }
            for r in results
        ],
        "totals": {},
    }
    totals: dict[str, int] = {}
    for r in results:
        for code, count in r.codes.items():
            totals[code] = totals.get(code, 0) + count
    payload["totals"] = totals
    path.write_text(json.dumps(payload, indent=2, sort_keys=True))
    print(f"wrote machine-readable summary: {path}", file=sys.stderr)


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Corpus S/F/C diagnostic sweep for issue #1245.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    ap.add_argument(
        "--roots",
        nargs="+",
        default=["examples", "tests"],
        help="Roots to walk for .pdx fixtures (default: examples tests)",
    )
    ap.add_argument(
        "--binary",
        default=None,
        help="Explicit paideia-as binary path (default: release, then debug)",
    )
    ap.add_argument(
        "--json",
        dest="json_out",
        default=None,
        help="Write machine-readable summary to this path",
    )
    ap.add_argument(
        "--jobs",
        type=int,
        default=4,
        help="Parallel invocations of `paideia-as build` (default: 4)",
    )
    ap.add_argument(
        "--timeout",
        type=float,
        default=30.0,
        help="Per-fixture timeout in seconds (default: 30)",
    )
    ap.add_argument(
        "--featured-only",
        action="store_true",
        help="Report only the featured codes from #1245's acceptance criteria",
    )
    ap.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Print per-fixture invocation failures",
    )
    ap.add_argument(
        "--limit",
        type=int,
        default=None,
        help="Cap number of fixtures processed (for quick sanity runs)",
    )
    args = ap.parse_args()

    binary = find_binary(args.binary)

    roots = [Path(r) for r in args.roots]
    for r in roots:
        if not r.exists():
            print(f"error: root path {r} does not exist", file=sys.stderr)
            return 3

    fixtures = collect_fixtures(roots)
    if args.limit is not None:
        fixtures = fixtures[: args.limit]

    if not fixtures:
        print("error: no .pdx fixtures found under configured roots", file=sys.stderr)
        return 3

    print(
        f"sweeping {len(fixtures)} fixtures with {args.jobs} workers "
        f"(binary: {binary})",
        file=sys.stderr,
    )

    results: list[FixtureResult] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        future_to_fixture = {
            pool.submit(sweep_fixture, binary, fx, args.timeout): fx
            for fx in fixtures
        }
        done = 0
        for future in concurrent.futures.as_completed(future_to_fixture):
            result = future.result()
            results.append(result)
            done += 1
            if done % 50 == 0 or done == len(fixtures):
                print(f"  progress: {done}/{len(fixtures)}", file=sys.stderr)

    results.sort(key=lambda r: r.path)
    render_report(results, featured_only=args.featured_only, verbose=args.verbose)

    if args.json_out:
        write_json(Path(args.json_out), results)

    return 0


if __name__ == "__main__":
    sys.exit(main())
