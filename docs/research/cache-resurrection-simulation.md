# Cache-resurrection simulation seam

Status: implemented bounded prototype; semantic review pending. Issue: [#3](https://github.com/Doss-com/objectKV/issues/3).

## Attribution and boundary

This extends Wiley Jones's existing objectKV simulator, generation fencing,
process-cache controls and authority-root handoff. The owning lease/root
invariants are the DOSS-authored RFC 0060. Git history identifies Wiley Jones as
the original author of the simulator files. Turmoil remains the existing pinned
simulation substrate; no new framework or provider integration is introduced.

This is a reviewable deterministic seam, not a production cache fix or a cloud
provider correctness claim. The range stream is buffered until completion;
progressive chunk emission and production adapters remain outside this proof.
No upstream comment, pull request or third-party outreach is part of this work.

## Invariant

A cache is disposable, version-addressed bytes, never publication authority.
Authorize captures generation, root and lease epoch. Both delayed publication
and value delivery compare that token with live authority. Delete, overwrite,
generation activation and lease expiry/release invalidate the old token.
Corrupt or missing bytes are repaired from the exact immutable backend version.
A stale token is refused even when its bytes still exist in a retained cache.

The independent receipt grader observes authority transitions and emitted bytes;
it does not use the subject's admission helper and poison switches cannot alter
its decisions. Publication itself is checked, so accepting a stale population
fails even if a later read correctly refuses it.

## Fixed histories and controls

Eight bounded histories cover delete/read, overwrite during an in-flight range,
delayed population, expired lease, released lease, cache loss plus corruption,
stale-generation population and retained stale-root resurrection. Successful
cache hits, repairs, refusals, stream completion, byte hashes and each detected
invariant failure appear in the correctness history, not only counters.

Four independent controls omit publication fencing, omit use fencing, trust
corrupt cached bytes, or repair from the wrong immutable version. The publication
control's minimal witness is `Authorize, AdvanceGeneration, Populate`: generation
1 is authorized, authority advances to generation 2, and the in-flight generation
1 population is incorrectly accepted. The correct subject refuses step 2.

Each scenario runs explicit interleaving points under seeded Turmoil virtual time.
Seeds change bounded delays; they do not claim exhaustive scheduling exploration.
Fixed seeds are 1103, 2027 and 4099. Counterexamples are reduced by deterministic
single-action deletion and checked for one-deletion minimality. They are not
claimed globally shortest. Reduction replays the same semantic transitions
without virtual delays, which do not influence authority or lease transitions.

## Validation and reproduction

The test/grader commits precede the candidate. Baseline 4d4fc69 plus the test-only
change fails because the cache API is absent (E0432); this is an absent-feature
RED, not evidence of a production bug. Each poison supplies a separate runtime
RED, exit 2. Correct subject runs exit 0.

Use the repository-pinned Rust 1.88.0, jobs 2, debug information disabled and
incremental compilation disabled:

```sh
cargo test -p okv-sim
cargo clippy -p okv-sim --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p okv-sim -- cache-replay --seed 1103 --source-revision <candidate>
cargo run -p okv-sim -- cache-replay --seed 1103 --source-revision <candidate> --mode skip-publication-fence
```

The compact JSON receipt records the candidate/tree, binary, lockfile and profile
hashes and all fixed-seed outcomes. Repeating each CLI run in a fresh process
produces byte-identical JSON for the same seed/mode/source identity. The trace
hash covers the canonical report with its own hash field empty.

Focused validation covers nine existing tests and four new integration tests.
Full-workspace tests, workspace Clippy and okv-eval smoke were not run in this
resource-bounded lane. Strict Clippy 1.97.1 separately fails on eight existing
`manual_is_multiple_of` diagnostics in untouched okv-model; this is not hidden or
fixed by unrelated changes here.

## Next gate

HOLD for semantic review and downstream publication access. The issue explicitly
places frozen eval and OTel receipts after semantic review. This change does not
modify okv-eval, existing result schemas, frozen suites, or reference models, and
does not fabricate telemetry. After review, freeze the seam contract in a
separate change and connect the accepted history to the existing eval/OTel path.
