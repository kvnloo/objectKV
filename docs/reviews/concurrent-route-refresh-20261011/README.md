# Concurrent route refresh: real crate regression

## Claim and boundary

`KvReadClient::refresh_or_limit` could return `RouteRefreshDidNotAdvance`
after another reader had already installed a newer tenant `RangeMap` in the
shared cache. The failing caller captured map v1; its source returned v1 while
the cache had advanced to v2. The fix validates the source response, acquires
the cache write lock, installs a newer response if present, and accepts the
refresh if the locked cache is newer than the caller's captured map. The
existing test still requires refusal when neither source nor cache advances.

This is a narrow implementation correction under Wiley Jones's RFC-0062 routed
exact-read design. The requested read version is unchanged. The frozen eval
suites, oracles, metrics, result schema, and `Cargo.lock` are untouched.
RFC-0062 explicitly leaves concurrent map publication outside its completed
process evidence, so this finding does not claim an observed production
failure or a complete concurrent client proof.

The new regression executes the actual `okv-object` crate's private refresh
method. It deterministically installs the newer cache state that a second
reader can publish during the first reader's await. It does not schedule two
public point or scan calls over TCP.

## Ownership and commits

- Public upstream `Doss-com/objectKV` main: `4d4fc69ca7dea24acdbc9da8997b07498e1d02f5`, tree `fb6387c85c3c22b026ca37602ac50bfd1a46a4be`.
- Public issue/PR lists checked on 2026-10-10; no route-refresh issue or PR was open. The upstream route RFC and source originated with Wiley Jones.
- Isolated branch: `research/range-read/concurrent-route-refresh`, worktree `/workspace/objectKV-route-refresh`.
- RED test: `3e91512d5d948d83af4585608299850ca84420ca`, tree `e8207cde6d9e38a3d58cd4cc3e66d42fc7e82fb7`.
- GREEN correction: `b95f626939463f8e61fae55613dcfc1207553452`, tree `fb1a84c703d6c56faa20697f2a2f20c7bbb89452`.

## Reproduction and gates

Machine: Debian 13, Linux 6.18.44 x86_64, 5 CPUs, 17 GiB RAM; Rust 1.88.0.
Cargo lockfile SHA-256:
`f66b9357906170be3cc49feb85383b8219acdab94b27331d117eb31008cc6aa0`.
Dependencies, including pinned SlateDB `e0161973`, came from the approved
public dependency cache. Runs used
`RUSTUP_HOME=/workspace/objectkv-rustup`,
`CARGO_HOME=/workspace/objectkv-cargo`, and a new
`CARGO_TARGET_DIR=/workspace/objectkv-route-red-target` distinct from prior
count-conversion builds. RED and GREEN logs each show recompilation of
`okv-object`; the independent critic also forced separate RED and GREEN
builds. Missing dependencies were not treated as passes.

| Commit | Command suffix after `cargo` | Result |
| --- | --- | --- |
| RED | `test -p okv-object --locked range_read_client::tests::accepts_another_readers_newer_map_when_source_reply_is_stale -- --exact --nocapture` | Exit 101; 0 passed, 1 failed; `RouteRefreshDidNotAdvance { current: 1, refreshed: 1 }` |
| GREEN | `test -p okv-object --locked range_read_client::tests:: -- --nocapture` | Exit 0; 3 passed, 0 failed, including unchanged stale-refusal and fixed-version fan-out tests |
| GREEN | `fmt --all -- --check` | Exit 0 |
| GREEN | `clippy --workspace --all-targets --locked -- -D warnings` | Exit 0 |
| GREEN | `test --workspace --locked` | Exit 0; 182 passed, 0 failed across 34 test-result groups |
| GREEN | `run -p okv-eval --locked -- smoke` | Exit 0; `status: pass`, `correctness_failures: 0` |

The exact RED, focused GREEN, Clippy, workspace test, and smoke outputs are in
`receipts/`. The independent skeptical critic approved the narrow change after
its own forced RED/GREEN replay and format check. It independently confirmed
that the test models a reachable shared-cache interleaving; the absence of a
two-reader public TCP test is the material limitation.

No benchmark, performance, cloud, publication, or production-availability
claim follows from this regression. This branch remains local pending a
separate publication lease.
