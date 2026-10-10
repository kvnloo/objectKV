# objectKV: two bounded correctness fixes for owner review

Prepared 2026-10-10. Review requests are drafts; no upstream PR, comment or owner message was sent.

## Ownership and exact targets

Wiley Jones authored the existing architecture, RFC-0013, streaming source, and evaluation contract. Kevin Rajan contributed the regression tests and these bounded source fixes.

Upstream `Doss-com/objectKV/main` and fork `kvnloo/objectKV/main` both refreshed to `4d4fc69ca7dea24acdbc9da8997b07498e1d02f5`. The current issue snapshot at 12:52:20 UTC lists five open contributor issues. An explicit read at 12:55 UTC returned all ten open upstream PRs (#7–16, provider/cache/assigned-range/physical-NVMe work), with no HTAP overlap, and no existing fork PRs. Both exact fork HTAP branches were returned. @wileyjones is the verified upstream author identity. The two bounded fixes remain unpublished as upstream PRs. No claim of upstream acceptance, hosted validation of these forks, or deployed production behavior is made.

Both fixes independently descend from the same upstream base. They are separate review targets, not a combined tested candidate. Do not sum their workspace test counts or claim composed validation. If both are accepted, integration must get its own validation.

## Review request 1: enforce the existing logical-group bound before further input

Suggested recipient: @wileyjones (GitHub identity verified from current upstream PR #16), objectKV source/RFC owner. Suggested title: `fix(htap): enforce logical-group cap before further input`.

Exact source: [ec01f3a](https://github.com/kvnloo/objectKV/commit/ec01f3a38f29e25fd24e8a29f8213916996074f8), complete source tree `0fc982e45d36f369689f003573f47d02ddffc7e8`. Fork branch tip and receipt: [603b604](https://github.com/kvnloo/objectKV/commit/603b60470457472598d2022df60240b57c4da255).

Draft review text:

Building on your existing RFC-0013 and streaming implementation, this moves the existing 16-contributing-row guard into `StreamingMergeState::next_group`. The previous path drained a group and only then refused it. The candidate refuses at the first known seventeenth contributing row before another source poll, including 16 fetched base rows plus a same-ID tail lookahead. Exactly 16 remains accepted.

Please review whether the base/tail accounting at [streaming.rs lines 767–858](https://github.com/kvnloo/objectKV/blob/ec01f3a38f29e25fd24e8a29f8213916996074f8/crates/okv-htap/src/streaming.rs#L767-L858) preserves your intended contributing-row semantics and the existing duplicate/order checks. The frozen one-row Arrow-batch tests use actual ordered cursors and poison poll sentinels. The published receipt records four expected baseline failures, then 15 passing focused/package checks against source-fresh fixed code, 192 passing workspace tests, strict workspace clippy, format, smoke, and 16 workload invocations across the three frozen HTAP suites. These are retained earlier results; no new execution was performed for this review packet.

The [portable receipt](https://github.com/kvnloo/objectKV/blob/603b60470457472598d2022df60240b57c4da255/docs/research/streaming-group-cap-early-receipt-2026-10-10.json) includes source/tree identity and the default-link attempt marked NOT_RUN before a bounded official-LLD leaf rebuild succeeded. This fix establishes early row-count refusal for one logical-ID group. It makes no whole-plan memory, retained-byte, arbitrary-batch/payload, excluded-version cursor-work, spill, or performance claim.

## Review request 2: refuse unsupported child partition shapes before execution

Suggested recipient: @wileyjones (GitHub identity verified from current upstream PR #16), objectKV source/RFC owner. Suggested title: `fix(htap): refuse unsupported input partition shapes`.

Exact minimal source: [e03823f](https://github.com/kvnloo/objectKV/commit/e03823fc406f563fcad54fc0f751ebc047ce69ec), complete source tree `ada496d6b4480a48b57a2af760e2223c32afd236`. Public frozen RED: [f0f77e6](https://github.com/kvnloo/objectKV/commit/f0f77e6b792286f449b4163db0802566975adf6e). Complete receipt: [7bd3b2a](https://github.com/kvnloo/objectKV/commit/7bd3b2a3863aa011d7aeb813d6dc055753511d70); final branch tip/publication map: [aa6ec07](https://github.com/kvnloo/objectKV/commit/aa6ec07fee0d08bc2cc253d28e017af3ee3eeb4f).

Draft review text:

This tests an integration gap in your existing single-logical-range RFC-0013 adapter. `StreamingSnapshotOverlayExec` exposes one output partition and executes partition zero of each child, but normal DataFusion planning can supply more than one input partition. The previous path silently omitted nonzero partitions, yielding a stale updated row, resurrected delete, or missing insert.

The [17-line execution guard](https://github.com/kvnloo/objectKV/blob/e03823fc406f563fcad54fc0f751ebc047ce69ec/crates/okv-htap/src/streaming.rs#L401-L417) checks both child counts and refuses anything other than one base/one tail before either child executes. Please review that fail-closed shape boundary and whether it matches your intended near-term adapter contract. It adds no coalescing, sorting, repartitioning, or multi-range support.

The [published evidence](https://github.com/kvnloo/objectKV/blob/aa6ec07fee0d08bc2cc253d28e017af3ee3eeb4f/evidence/htap-input-partitions/README.md) records unchanged primary tests: baseline six failures/one positive, candidate seven passes. A separate frozen critic records baseline 11 failures/two passes and candidate 13 passes, including zero/one/two input shapes, both modes, child replacement, optimizer expansion, and a two-file Parquet shape. The Parquet fixture proves reachability of an unsupported shape; it is not a Parquet row-loss witness.

Retained integration results are format, strict workspace clippy, 188 workspace tests, smoke, and six frozen streaming workload invocations containing 18 per-seed samples. The 13 independent critic checks are separate from the 188 total. Source-fresh execution used Rust 1.88.0 and the existing official bundled LLD; public/local complete Git trees match despite different commit metadata. This packet rechecks those identities without rerunning tests. This is local adapter correctness evidence, with no performance, deployed production, hosted-CI, or upstream-adoption claim.

## Verification and bounded next-gap inspection

`receipts.json` includes the original verified-git-pins.json and records fresh main/source/receipt pins, exact complete-tree equality to the local reviewed checkpoints, source/test blob identity through receipt-only commits, and SHA-256 values. Existing worktrees and all graders/results were preserved. A new detached upstream checkout was used for static inspection only.

A bounded read of RFC-0062 and the existing routed-read client/server checked fixed-version retry, half-open scan fan-out, and result validation. No independently established production correctness defect was found in that pass. Missing broader routing/security/operating evidence is already declared in RFC-0062; it is not relabeled as a new bug. No additional source edit, test, build, installation, push, issue, PR, or comment was made.

Environment observation: Cargo/rustc/rustup are absent from the current PATH, `<missing-shared-workspace>` is absent, and retained `/tmp` binary links refer to that missing workspace. Previous binary receipts remain historical evidence, not executable proof of a fresh run here. No build slot or toolchain recovery is needed to complete this static review preparation.

## Downstream packet identity

This evidence-only packet adds no source or test changes to its main parent. It is not an integration of the two independent fixes. Original receipt documents are bundled by filename in receipts.json; static audit and its machine-readable checks are alongside this report. No new build or test was executed for this packet.
