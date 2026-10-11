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
- Isolated branch: `research/range-read/concurrent-route-refresh`.
- RED test: `3e91512d5d948d83af4585608299850ca84420ca`, tree `e8207cde6d9e38a3d58cd4cc3e66d42fc7e82fb7`.
- GREEN correction: `b95f626939463f8e61fae55613dcfc1207553452`, tree `fb1a84c703d6c56faa20697f2a2f20c7bbb89452`.

The original packet was published to the downstream `kvnloo/objectKV` branch at
[commit `1638923ab3b3e04a2f79cd2efb48cf55a44867fb`](https://github.com/kvnloo/objectKV/commit/1638923ab3b3e04a2f79cd2efb48cf55a44867fb),
tree `7dd77956021aac2f85f9dd5b0121bd433cc48e7e`. No upstream pull request or
merge was made as part of this work.

## Preparation and review provenance

dot, an OpenAI AI assistant, prepared the regression, implementation, report,
and this documentation correction under Kevin Rajan's authorization. A separate
OpenAI AI critic performed the independent review described below. Wiley Jones
authored the original routed-read design and RFC-0062; the design credit remains
his.

The original RED, GREEN, and report commits used Kevin's configured Git author
identity. That metadata does not establish human preparation or independent
human code review. This correction uses an explicit AI author identity and
makes no DCO attestation on Kevin's behalf.

## Reproduction and gates

Machine: Debian 13, Linux 6.18.44 x86_64, 5 CPUs, 17 GiB RAM; Rust 1.88.0.
Cargo lockfile SHA-256:
`f66b9357906170be3cc49feb85383b8219acdab94b27331d117eb31008cc6aa0`.
The locked dependency graph includes pinned SlateDB `e0161973`. Runs used
dedicated Rustup and Cargo homes and a new Cargo target directory distinct from
prior count-conversion builds. RED and GREEN logs each show recompilation of
`okv-object`; the independent AI critic also forced separate RED and GREEN
builds. Missing dependencies were not treated as passes.

For portable reproduction, set the following variables to caller-selected
locations. Use a target directory dedicated to the revision being tested, and
verify the checkout's commit before running the command suffixes below.

```sh
: "${OKV_CHECKOUT:?Set the path to the checkout of the recorded revision}"
: "${OKV_RUSTUP_HOME:?Set the path to a Rustup home with Rust 1.88.0}"
: "${OKV_CARGO_HOME:?Set the path to the Cargo home}"
: "${OKV_CARGO_TARGET_DIR:?Set the path to a target directory for this revision}"
export RUSTUP_HOME="$OKV_RUSTUP_HOME"
export CARGO_HOME="$OKV_CARGO_HOME"
export CARGO_TARGET_DIR="$OKV_CARGO_TARGET_DIR"
cd "$OKV_CHECKOUT"
git rev-parse HEAD
```

Invoke `"$CARGO_HOME/bin/cargo"` with each command suffix below.

| Commit | Command suffix after `cargo` | Result |
| --- | --- | --- |
| RED | `test -p okv-object --locked range_read_client::tests::accepts_another_readers_newer_map_when_source_reply_is_stale -- --exact --nocapture` | Exit 101; 0 passed, 1 failed; `RouteRefreshDidNotAdvance { current: 1, refreshed: 1 }` |
| GREEN | `test -p okv-object --locked range_read_client::tests:: -- --nocapture` | Exit 0; 3 passed, 0 failed, including unchanged stale-refusal and fixed-version fan-out tests |
| GREEN | `fmt --all -- --check` | Exit 0 |
| GREEN | `clippy --workspace --all-targets --locked -- -D warnings` | Exit 0 |
| GREEN | `test --workspace --locked` | Exit 0; 182 passed, 0 failed across 34 test-result groups |
| GREEN | `run -p okv-eval --locked -- smoke` | Exit 0; `status: pass`, `correctness_failures: 0` |

The RED, focused GREEN, Clippy, workspace test, and smoke outputs in `receipts/`
are sanitized copies: absolute checkout and target directory prefixes are
replaced by `<CHECKOUT>` and `<TARGET_DIR>`. Assertions, command arguments,
dependency revisions, test names, counts, timings, and results are preserved.
Original raw logs and their SHA-256 hashes are retained privately outside the
repository. The independent AI critic approved the narrow change after its own
forced RED/GREEN replay and format check. It independently confirmed that the
test models a reachable shared-cache interleaving; the absence of a two-reader
public TCP test is the material limitation.

The 182 workspace test passes apply to this independent branch from upstream
`4d4fc69`; they do not cover a composition with the count-conversion branch.
No benchmark, performance, cloud, or production-availability claim follows from
this regression.
