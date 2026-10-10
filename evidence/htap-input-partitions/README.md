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

- Local test-only RED commit: `9291740736c67075aa689f73b6fb8c6f6cfa695e`
- Local minimal implementation commit: `8ab940a8e1912c8bcb71878508db7217a945584c`
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

## Final repository and adjacency gates

- Workspace format: pass
- Strict workspace all-target clippy: pass
- Workspace tests: 188 passed, 0 failed, 0 ignored, 0 filtered out; 34 result
  groups include 10 empty doctest groups
- The 188 workspace tests include the seven primary tests. The 13 independent
  critic checks were a separate unchanged-harness run and are not added to that
  workspace total
- Model/eval smoke: pass, zero correctness failures
- Frozen streaming suite: six final workload invocations, with three actual
  per-seed samples retained in each result (18 samples total). The correct
  workload keeps and all five negative workloads discard. All exit zero;
  schema and exact replay gates pass. This is not a count of independent
  process-isolated performance runs

The frozen final suite uses seeds 1103, 2207, and 3301, the unchanged 30-event
budget, and the local-fs profile. The verdicts and gate evidence are retained in
`streaming-workload-summary.json`.

The direct initial suite invocations reported their runtime rustc field as
`unknown` because the runtime shell lacked the toolchain PATH. Those raw local
receipts were preserved; their hashes and the observation are recorded in
`initial-runtime-profile-observation.json`. The final unchanged suite was rerun
with the matching Rust PATH and explicit `RUSTUP_TOOLCHAIN=1.88.0`, and records
`rustc 1.88.0 (6b00bc388 2025-06-23)` in every result. No source, oracle, suite,
profile budget, or schema changed for that metadata repair.

The eval executable was freshly built from source checkpoint
`4ca824ac5d2a990cd5f2cec04f2ecc4d8f51d5b2` (implementation unchanged from
`8ab940a`) with the existing official Rust 1.88.0 bundled LLD, passed directly
as a leaf link argument. This avoids re-linking dependencies or retrying the
previously problematic GNU-link path. The executable SHA-256 is
`71a2ceae542da1d1085125f9c3dfd2955e7dd7b7b28a5a2569b384024ae40822`.
The official bundled linker SHA-256 is
`f8caf232fdaf43adf032d0aff6fdbac45fe978645c37acc094592a575d7fd8dc`.

Exact commands and environment settings are in `commands-and-profile.json`.
Repository-wide logs and source/executable identity receipts are alongside this
file. These are local correctness gates; hosted CI and upstream adoption have
not been established. No benchmark, cloud, production availability, or
multi-execution-range claim is made.

## Public reproducible revisions

The public frozen RED is `f0f77e6b792286f449b4163db0802566975adf6e`; the minimal
GREEN implementation is `e03823fc406f563fcad54fc0f751ebc047ce69ec`.
The executed source checkpoint is `45b1da7aa629222ca3e993d5465122f35794b657`,
and the complete validation receipt is `7bd3b2a3863aa011d7aeb813d6dc055753511d70`.
Use these public revisions for separate RED/GREEN checkouts.

`publication-map.json` maps the original local build/evidence commit IDs to
these public API-created commits. Their complete Git trees are identical;
API-created commit timestamps change commit IDs but do not change source or
frozen grader bytes. The final upstream/owner check at 05:08 UTC still found
main at the pinned revision and no overlapping partition-shape fix.
