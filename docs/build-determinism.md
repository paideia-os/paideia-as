# Build determinism

paideia-as produces reproducible binaries when build inputs are
canonicalised. This document describes the env-var contract.

## SOURCE_DATE_EPOCH

Per the Reproducible Builds spec, SOURCE_DATE_EPOCH sets the canonical
timestamp embedded in build artifacts (e.g., PE/COFF `time_date_stamp`,
ELF `.note.gnu.build-id`).

```sh
SOURCE_DATE_EPOCH=1700000000 cargo build --release -p paideia-as
```

If unset, paideia-as uses 0 as the timestamp.

## PDX_PATH_PREFIX_MAP

Rewrites absolute source paths to canonical build-relative forms for
embedding in debug info. Format: "OLD=NEW".

```sh
PDX_PATH_PREFIX_MAP="/home/builder/=/build/" cargo build --release
```

After mapping, the artifact contains "/build/..." instead of
"/home/builder/..." regardless of where the build was launched from.

## Determinism guarantees

With both env vars set to canonical values, two builds in different
times AND directories produce byte-identical output (modulo the
non-determinism allowlist documented in tools/ddc/allowlist.toml).

The DDC harness (tools/ddc/) verifies this property end-to-end.
