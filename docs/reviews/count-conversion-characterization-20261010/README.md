# Count-conversion characterization, 2026-10-10

**Review evidence only. No production correction or upstream communication.**

At pinned source `4d4fc69ca7dea24acdbc9da8997b07498e1d02f5`, the unchanged
`bounded_count(u64) -> f64` returns at most **4,294,967,295**. The eight-fixture
native characterization compiles successfully and produces **3 passes and
5 failures (exit 101)** against an exact-integer reporting hypothesis. Those
failures are retained. Whether that hypothesis is the intended numeric contract
remains an owner-review question.

AI authorship: prepared by **dot**, an OpenAI-powered AI assistant.

Credit: **Wiley Jones** owns objectKV's architecture, evaluation contracts, and
current evaluation work. This downstream contribution supplies a bounded numeric
observation for his review and makes no competing implementation decision.

## Scope and provenance

- Upstream and downstream main were both pinned to the revision above in the
  public ownership read at **2026-10-10 16:38 UTC**.
- [Exact source helper, lines 22077–22079](https://github.com/Doss-com/objectKV/blob/4d4fc69ca7dea24acdbc9da8997b07498e1d02f5/crates/okv-eval/src/main.rs#L22077-L22079).
- Source Git blob: `38e9c9bc380454a6007613c64ee1be71ff6e1c6a`.
- [Wiley's PR 16](https://github.com/Doss-com/objectKV/pull/16) was at
  `4cfc4a46ed5b7ae628dd44b99a22d4da470329be`; its helper was byte-identical.
- Pure numeric reporting examples include
  [operation throughput](https://github.com/Doss-com/objectKV/blob/4d4fc69ca7dea24acdbc9da8997b07498e1d02f5/crates/okv-eval/src/main.rs#L3074-L3085)
  and [read/write byte emission](https://github.com/Doss-com/objectKV/blob/4d4fc69ca7dea24acdbc9da8997b07498e1d02f5/crates/okv-eval/src/main.rs#L3140-L3157).
- The [byte metric entry](https://github.com/Doss-com/objectKV/blob/4d4fc69ca7dea24acdbc9da8997b07498e1d02f5/evals/metrics.toml#L200-L207)
  describes transferred bytes with unit `By`, without a numeric ceiling.
  The examined [result schema](https://github.com/Doss-com/objectKV/blob/4d4fc69ca7dea24acdbc9da8997b07498e1d02f5/evals/schema/result.schema.json)
  allows numeric metrics without a maximum. These observations do not establish
  historical intent or replace owner interpretation.

## Frozen exact fixtures

All inputs are exactly representable by binary64. Assertions compare bit
patterns against explicit f64 literals; no epsilon or replacement converter
participates.

| u64 input | Expected exact numeric value | Current helper |
| ---: | ---: | --- |
| 0 | 0 | pass |
| 1 | 1 | pass |
| 4,294,967,295 | 4,294,967,295 | pass |
| 4,294,967,296 | 4,294,967,296 | fails: 4,294,967,295 |
| 4,294,967,297 | 4,294,967,297 | fails: 4,294,967,295 |
| 10,737,418,240 | 10,737,418,240 | fails: 4,294,967,295 |
| 9,007,199,254,740,991 | 9,007,199,254,740,991 | fails: 4,294,967,295 |
| 9,007,199,254,740,992 | 9,007,199,254,740,992 | fails: 4,294,967,295 |

The synthetic 10 GiB-valued fixture differs by 6,442,450,945 numeric units,
about 60%. **This is arithmetic, not a measured workload, physical-byte deficit,
performance result, or architectural conclusion.** The current ceiling occurs
far below binary64's exact-integer boundary.

## Portable replay

Requirements: Python 3, Bash, and an **already installed** Rust compiler.
Use the absolute path to the installed toolchain's actual `rustc` binary,
not a rustup proxy or wrapper. This avoids rustup-triggered downloads. Run from
this repository's root, using a new output directory:

```sh
RUSTC=/absolute/path/to/already-installed/rustc \
  bash docs/reviews/count-conversion-characterization-20261010/replay.sh \
  /tmp/objectkv-count-characterization
```

The extractor verifies the complete pinned source SHA-256, the byte-identical
helper, the frozen portable oracle, and the exact original harness. It refuses
a changed source or an existing output directory. Replay calls only `rustc`
against the standalone harness, then runs eight tests. The current helper is
expected to return **exit 101**, preserving its five failed assertions. A shell
caller using `set -e` should account for this deliberately red characterization.
Compiler identity, commands, raw outputs, and exit statuses are written to the
output directory. No Cargo dependencies, downloads, full-crate build, or workload
are involved. Generated binaries must not be committed.

## Receipts and normalization

`receipts/original-native` and `receipts/independent-native` retain both native
runs, including all five assertion failures. `receipts/portable-native` records
one execution of the final public replay command. The existing compiler was
Rust **1.97.1**, commit `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, host
`x86_64-unknown-linux-gnu`, LLVM **22.1.6**.

`witness.rs` and `bounded_count.extracted.rs` are byte-identical to the original
frozen inputs. For public delivery only, absolute local harness paths in raw
logs are replaced with `witness.rs`; public commands use portable path labels.
The oracle's internal reviewer identifier is removed and replaced with a public
verification description. **Its fixture array is unchanged.** `normalization.json`
records original-byte and public-copy hashes separately. Original private
receipt bytes remain preserved; normalized copies are not represented as those
original bytes. `SHA256SUMS` covers all shipped files except itself.

The full okv-eval crate and workspace were **not built or tested**. Repository-
pinned Rust **1.88.0** was **not qualified**. `cargo fmt`, workspace Clippy,
workspace tests, and eval smoke were **not run** for this evidence-only packet.
No suite, registry, result schema, held-out seed, production oracle, measured
artifact, or production source is modified. There is **no GREEN production-fix
claim** and no comparable benchmark or champion selection.

## Unsent owner-review question

Is the u32 ceiling an intentional evaluation numeric contract? If so, where is
that bound defined, and how should larger counts be surfaced without silently
appearing as exact byte totals? If not, this evidence supports a separately
reviewed correction while leaving frozen measurements intact.

This question has not been sent upstream. Any production decision or upstream
communication remains with the owners.
