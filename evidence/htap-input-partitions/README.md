# Streaming input partition-shape correctness

Status: local internal-adapter correctness evidence. No deployed production or
new multi-range support claim.

## Provenance and scope

The original objectKV architecture, implementation, and RFC-0013 are by Wiley
Jones and the DOSS contributors. This contribution tests one independently
identified integration gap in that existing adapter.

A fresh GitHub ownership review on 2026-10-10 at 04:41 UTC found upstream main
`4d4fc69ca7dea24acdbc9da8997b07498e1d02f5` and the unchanged streaming source
blob `8b71665b82fa62c81f70523ceca8af4f7e6baadc`. All five issue and ten PR listings
were checked; no partition-shape fix or overlapping owner was found. Issue #5
is the separately owned analytical-lag benchmark. PRs #7 through #16 address
provider, cache, range-image, and NVMe work. None is modified here.

This work starts from that main revision. It does not change the separately
published group-cap candidate, frozen benchmark graders, cache-resurrection
simulation, generic-repeat runner, model, suite, result schema, or dependencies.

## Defect and bounded fix

The streaming execution plan exposes one output partition and executes partition
zero of each child. It does not declare a single-partition input distribution,
and normal DataFusion plans can supply multiple input partitions. Previously,
nonzero child partitions were silently omitted. In a normal SQL query, that
returned an old updated row, resurrected a deleted row, and omitted a new insert.

RFC-0013 admits one logical-id range and defers multiple execution ranges. The
candidate therefore reads both child partition counts and refuses any shape
other than one base partition and one tail partition before either child is
executed. It adds no coalescing, sorting, repartitioning, or new merge semantics.
The diagnostic names both partition counts. Existing single-partition behavior
remains exact.

## Separately frozen primary oracle

- Test-only RED commit: `9291740736c67075aa689f73b6fb8c6f6cfa695e`
- Minimal implementation commit: `8ab940a8e1912c8bcb71878508db7217a945584c`
- Frozen grader file: `crates/okv-htap/src/streaming/tests/input_partitions.rs`
- Grader SHA-256: `1ad1cd83cfdca7d4ae9aeaddbaace32a3e450e353e15708ea83562c9a60b025c`

The seven cases use actual DataFusion MemTable execution plans:

1. INSERT only in tail partition one
2. UPDATE only in tail partition one
3. DELETE only in tail partition one
4. Base row only in base partition one
5. Both inputs have two partitions
6. Ordinary SQL/physical planning with two target partitions and tail changes
   only in partition one
7. Exact single-partition SQL positive

For unsupported shapes the oracle accepts only the complete independently
authored snapshot or the specific unsupported-shape diagnostic before either
child execute counter increments. Unrelated errors cannot satisfy the oracle.

RED: six incorrect-result assertions fail; the single-partition positive passes.
GREEN: all seven unchanged primary cases pass. All 11 existing and new HTAP
package tests and its doctest phase pass. Strict HTAP all-target clippy passes.
The primary grader hash is unchanged between RED and GREEN.

## Reproduction

Rust 1.88.0, Cargo 1.88.0, Linux x86_64; DataFusion 54.0.0 and Arrow/Parquet
58.3.0 remain locked. Builds use two jobs, debug info zero, incremental disabled,
and existing cached dependencies. Repository tokio_unstable configuration is
unchanged. These are correctness tests, not latency measurements.

```sh
export CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
cargo +1.88.0 test --offline --locked -p okv-htap --lib input_partitions -- --nocapture --test-threads=1
cargo +1.88.0 clippy --offline --locked -p okv-htap --all-targets -- -D warnings
cargo +1.88.0 test --offline --locked -p okv-htap
```

Use separate checkouts of the RED and GREEN revisions to reproduce the exact
contrast. The logs in this directory retain assertion outcomes, compiler
rebuild lines, and commands' terminal results. Checkout and target paths are
replaced with portable placeholders only.

## Independent frozen critic

The separately frozen harness is included unchanged as
`independent/partition_shape_critic.rs`, SHA-256
`2252818e1b2da06ffdb5c2f6b691a374e3ba33d313c2f110ebf05dced347c429`.
It covers the zero/one/two input-count matrix in both normal and materializing
modes, empty one/one positives, child replacement, exact update/delete/insert,
normal optimizer expansion of initially one/one inputs, and real two-file
Parquet input shape.

Fresh-build RED: 11 failures, 2 passes. Fresh-build GREEN: all 13 pass. Source
and executable hashes, terminal exits, and raw logs are in `independent/`.
The unchanged grader's optimizer control independently witnesses omitted inserts
when the planner changes an originally single-partition tail to two partitions.

Important Parquet bound: the two-file fixture exposes a two-partition base, but
the baseline happened to return its complete rows. It proves unsupported-shape
reachability, not a Parquet row-loss witness. The fixed adapter refuses that
unsupported shape explicitly.

To reproduce the independent harness, copy it unchanged into
`crates/okv-htap/tests/partition_shape_critic.rs` in the chosen RED or GREEN
checkout, creating that directory if needed, then run:

```sh
cargo +1.88.0 test -p okv-htap --test partition_shape_critic --offline --locked independent_partition_critic -- --nocapture --test-threads=1
```

## Remaining validation

Repository-wide format, strict clippy, tests, and smoke are pending. This receipt
is not a full-workspace pass until those checks are recorded.
