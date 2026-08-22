# ML-DSA-65 ACVP Vectors — Provenance

These files are the NIST ACVP FIPS-204 (ML-DSA) internal-projection test
vectors, trimmed to the ML-DSA-65 parameter set only (paideia does not
expose ML-DSA-44 or ML-DSA-87).

## Pinned upstream commit

```
usnistgov/ACVP-Server @ 65370b8
```

The same commit RustCrypto's `ml-dsa` crate tracks in its
`ml-dsa/tests/README.md`. The published `ml-dsa` crate (v0.1.1, latest as of
2026-08-22) still `exclude`s these files from crates.io — see
`../ML_DSA_ACVP_STATUS.md` for the audit trail. This vendoring is the
"path (b)" trigger from that document: paideia decided to vendor the
NIST JSON directly rather than wait for upstream to expose it.

## File map

| Local file                     | Upstream path                                                                              |
| ------------------------------ | ------------------------------------------------------------------------------------------ |
| `ml_dsa_65_keygen.json`        | `gen-val/json-files/ML-DSA-keyGen-FIPS204/internalProjection.json` (ML-DSA-65 group only)  |
| `ml_dsa_65_siggen.json`        | `gen-val/json-files/ML-DSA-sigGen-FIPS204/internalProjection.json` (ML-DSA-65 groups only) |
| `ml_dsa_65_sigver.json`        | `gen-val/json-files/ML-DSA-sigVer-FIPS204/internalProjection.json` (ML-DSA-65 group only)  |

Vector counts (ML-DSA-65 only):

- `ml_dsa_65_keygen.json`: 1 group × 25 tests = **25 keyGen vectors**.
- `ml_dsa_65_siggen.json`: 2 groups × 10 tests = **20 sigGen vectors**
  (10 deterministic + 10 hedged).
- `ml_dsa_65_sigver.json`: 1 group × 15 tests = **15 sigVer vectors**
  (3 valid, 12 tampered/malformed with ACVP-supplied `reason`).

Total: **60 vectors**, ~830 KB on disk.

## Regenerating

The trimming step is a mechanical filter over the upstream JSON — no
domain interpretation. To refresh (e.g. when NIST reissues vectors, or
`ml-dsa` bumps the pinned commit):

```sh
COMMIT=<new-commit-sha>
BASE=https://raw.githubusercontent.com/usnistgov/ACVP-Server/$COMMIT/gen-val/json-files
mkdir -p /tmp/acvp-refresh && cd /tmp/acvp-refresh
curl -sSL "$BASE/ML-DSA-keyGen-FIPS204/internalProjection.json" -o keygen.json
curl -sSL "$BASE/ML-DSA-sigGen-FIPS204/internalProjection.json" -o siggen.json
curl -sSL "$BASE/ML-DSA-sigVer-FIPS204/internalProjection.json" -o sigver.json

python3 - <<'PY'
import json, os
OUT = "<path>/paideia-as/tests/pq-corpus/vectors"
def slim(name_in, name_out):
    d = json.load(open(name_in))
    d["testGroups"] = [g for g in d["testGroups"] if g.get("parameterSet") == "ML-DSA-65"]
    json.dump({k: d[k] for k in ("vsId","algorithm","mode","revision","isSample","testGroups") if k in d},
              open(os.path.join(OUT, name_out), "w"), separators=(",", ":"))
slim("keygen.json", "ml_dsa_65_keygen.json")
slim("siggen.json", "ml_dsa_65_siggen.json")
slim("sigver.json", "ml_dsa_65_sigver.json")
PY
```

Then update the pinned commit in this README and in
`../ML_DSA_ACVP_STATUS.md`, and run `cargo test -p pq-corpus --test
acvp_ml_dsa_65`.

## Format

Each file is a JSON object with:

```
{
  "vsId":       <int>,
  "algorithm":  "ML-DSA",
  "mode":       "keyGen" | "sigGen" | "sigVer",
  "revision":   "FIPS204",           // may be absent
  "isSample":   false,
  "testGroups": [ ... ]              // ML-DSA-65 only, after trimming
}
```

Group and test schemas mirror the ACVP spec (see
<https://github.com/usnistgov/ACVP/tree/master/src/ml-dsa/sections>).
Rust representations live in `../src/acvp.rs`.

## License

NIST-authored content. NIST test vectors are US-Government public-domain
per `usnistgov/ACVP-Server` LICENSE.md.
