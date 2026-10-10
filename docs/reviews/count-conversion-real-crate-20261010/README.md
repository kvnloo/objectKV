# Real crate count-conversion reproduction, 2026-10-10

This is a diagnostic RED test of the actual `okv-eval` binary crate, based on
Kevin Rajan's evidence commit `bbbd61df49c6c665723e4b6d0b2b3455d2e1125f`
and upstream main `4d4fc69ca7dea24acdbc9da8997b07498e1d02f5`. The test
module calls the crate's `bounded_count` through `super::bounded_count` and
mirrors the eight unmodified fixtures in the earlier review's `oracle.json`.

## Baseline

Run from this worktree with the repository-pinned Rust 1.88.0 toolchain:

```sh
CARGO_HOME=/workspace/objectkv-cargo \
RUSTUP_HOME=/workspace/objectkv-rustup \
PATH=/workspace/objectkv-cargo/bin:$PATH \
CARGO_TARGET_DIR=/workspace/objectkv-target \
CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 \
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=3 \
cargo test -p okv-eval --bin okv-eval count_conversion_tests --locked -- --nocapture
```

Result: compiled and ran the actual crate tests; exit **101**, **3 passed,
5 failed**. The failing fixtures are `four_gib`, `four_gib_plus_one`, `ten_gib`,
`f64_exact_integer_limit_minus_one`, and `f64_exact_integer_limit`. All report
`u32::MAX` instead of the fixture's exactly representable numeric value. The
unmodified test output is in `receipts/red-test.txt`. The formatting check
(`cargo fmt --all -- --check`) passed. The cold build took 3m 15s.

Environment: Debian GNU/Linux 13.6, x86_64, five CPUs, 17 GiB RAM, 32 GB
workspace overlay with approximately 30 GB initially available. `rustc 1.88.0`
(`6b00bc388`, LLVM 20.1.5); `Cargo.lock` SHA-256
`f66b9357906170be3cc49feb85383b8219acdab94b27331d117eb31008cc6aa0`.
`cargo fetch --locked` succeeded, including pinned SlateDB
`e0161973d8d7ffdede7c44725729838811674e99`; locked offline Cargo metadata
then resolved all ten workspace packages.

## Contract and ownership check

The unchanged helper feeds object-store request and byte measurements,
secondary totals, and budget units in `crates/okv-eval/src/main.rs`. Those
values enter `WorkloadExecution`, then the schema-validated `EvalResult`.
`evals/metrics.toml` describes transferred bytes in `By`, and
`evals/schema/result.schema.json` has no numeric maximum on result metrics or
the observed budget. RFC-0004 requires exported request and byte counts; the
evaluation decisions and RFCs found no `u32` reporting ceiling. These support
the exact-count hypothesis but do not establish the owner's historical intent.

At this review, the public upstream main still matched `4d4fc69`. The open
upstream PRs 7–16 belong to Wiley Jones; PR 16's current head
`4cfc4a46ed5b7ae628dd44b99a22d4da470329be` retains the identical helper.
Issue 4 addresses repeat orchestration, not this conversion. The fork's
evidence branch still points to `bbbd61d`. No upstream post or PR was made.

This test uses synthetic counts. It does not transfer multi-gigabyte objects,
run an evaluation workload, qualify performance, or decide a champion. Wiley
Jones retains credit for objectKV architecture and evaluation contracts. This
crate reproduction and tests were prepared by Codex, an OpenAI AI assistant.
