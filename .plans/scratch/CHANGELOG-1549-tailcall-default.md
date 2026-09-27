# paideia-as #1549 — enable tailcall in default requested_passes

## Change
- `crates/paideia-as/src/cmd_build/mod.rs:330-331`: added `"tailcall"` alongside `"peephole"` in default passes.

## Risk
- Test corpus goldens under `crates/paideia-as/tests/build_emit/` may diff if tail-call opportunities exist in fixtures.
- Downstream: paideia-os workspace `.pdx` files with `call foo; ret` sequences will now be tail-called by default.

## Verification hooks for main
- `cargo test --workspace` — expect either full green or fingerprint diffs in build_emit/ that should be reviewed for semantic neutrality.
- Downstream paideia-os build should still produce a bootable kernel (bumped submodule test).

## Version bump
- Bump workspace.version from 0.36.56 → 0.36.57.
