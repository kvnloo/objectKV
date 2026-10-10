# RFC-0072: Proposed Redis string/key semantic subset over objectKV

- Status: **[PROPOSED]** — downstream research; not an accepted upstream contract
- Tracked by: [Doss-com/objectKV #1](https://github.com/Doss-com/objectKV/issues/1), contributor-board T8/T12
- Original kernel/version/isolation contracts: **DOSS / Wiley Jones and contributors** ([RFC-0002](0002-version-and-mvcc-model.md), [RFC-0008](0008-transaction-isolation.md))
- Purpose: define a bounded Redis **serving-model** pressure test without turning `okv-model` into a Redis implementation

## Evidence and compatibility boundary

The present kernel specifies cell-scoped `CommitVersion = (generation, sequence)`, exact `read_at(R)`, point/range tombstones, read floors, and committed-batch idempotency. It targets strict serializability *within one cell*, but that does **not** establish Redis command support, end-to-end cross-range transactions, cluster parity, TTL, or a deployed RESP endpoint.

Every **accept** below means *proposed for the first oracle/prototype*, **not** implemented or production-compatible. The first slice operates within one tenant database and one cell, stores opaque binary **string** keys and values, has no expiration or eviction, and obtains the read version from cell authority. Unsupported forms must fail explicitly; silently ignoring flags would be a semantic bug.

## Command matrix (first slice)

| Category | Commands/forms | Decision | Proof obligation or blocking primitive |
| --- | --- | --- | --- |
| Stateless connection | `PING`, `ECHO` | **Accept** | RESP request/response framing; no storage effects |
| Single-key read | `GET key` | **Accept** | Latest authoritative read version; absent/tombstoned key returns null bulk, never a stale value |
| Single-key write | `SET key value` with **no options** | **Accept** | One durable, idempotent committed mutation; return OK only on known successful commit |
| Single-key delete | `DEL key` (one key) | **Accept** | One point tombstone; return 1 iff key existed immediately before this serialized deletion, else 0 |
| Single-key membership/type | `EXISTS key`, `TYPE key` (one key) | **Accept** | One authoritative snapshot; return 0/1, or `none`/`string` respectively |
| Conditional/read-modify-write | `SET ... NX/XX/GET/IFEQ/IFNE/IFDEQ/IFDNE`, `SETNX`, `GETSET`, `GETDEL`, `APPEND`, `INCR*`, `DECR*` | **Defer** | Atomic compare/conflict and previous-value semantics; Redis numeric grammar, overflow, and reply contracts need separate negative controls |
| Multi-key snapshot reads | `MGET`, multi-key `EXISTS` | **Defer** | One causally valid read version across ranges and exact reply ordering; repeated existing keys in `EXISTS` must count **per occurrence** |
| Multi-key mutations | `MSET`, multi-key `DEL`, `RENAME`/`RENAMENX` | **Defer** | Cross-range atomicity, conflict ranges, and response counts; no partial successful write |
| Temporal/TTL | `EXPIRE`, `PEXPIRE`, `EXPIREAT`, `PEXPIREAT`, `TTL`, `PTTL`, `PERSIST`, `SET ... EX/PX/EXAT/PXAT/KEEPTTL` | **Defer** | Persisted logical deadline, authority-clock policy, expiry observation order, and TTL/overwrite/retry semantics; commit versions are **not** timestamps |
| Transaction protocol | `WATCH`, `MULTI`, `EXEC`, `DISCARD`, `UNWATCH` | **Defer** | Connection-local command queue, atomic ordered replies, watch invalidation (including expiry), failure handling, and durable outcomes |
| Iteration/streaming | `SCAN`, `KEYS`, `HSCAN`, `SSCAN`, `ZSCAN`, `XADD`, `XREAD`, `XRANGE` | **Defer** | Snapshot-consistent iteration/cursors, typed collection representations, and bounded continuation |
| Blocking operations | `BLPOP`, `BRPOP`, `BLMOVE`, blocking stream reads | **Defer** | Durable waiter registration, cancellation/failover, fairness, and ownership semantics |
| Data structures | `HSET`, `LPUSH`, `SADD`, `ZADD` and related operations | **Reject from v0** | No collection codecs/type invariants; do not report fabricated string-compatible responses |
| Scripting/functions | `EVAL`, `EVALSHA`, `FCALL`, `SCRIPT`, `FUNCTION` | **Reject from v0** | No sandboxed script execution, deterministic script effects, or Redis function runtime |
| Pub/sub | `PUBLISH`, `SUBSCRIBE`, `PSUBSCRIBE`, `SSUBSCRIBE` | **Reject from v0** | No connection fan-out/delivery model or pub/sub lifecycle |
| Eviction/configuration | `CONFIG SET maxmemory-policy`, `MEMORY` and eviction-sensitive semantics | **Reject from v0** | Disposable cache eviction is *not* Redis key eviction; policy must be separately specified |
| Cluster/admin | `CLUSTER`, `READONLY`, `MIGRATE`, `SELECT`, global `DBSIZE` | **Reject from v0** | No Redis Cluster topology/slot protocol, multiple logical DBs, or global key-count contract |

`HELLO`/RESP3 negotiation and authentication are separate protocol implementation milestones, not implied by this semantic oracle.

## Proposed invariants for accepted stateful commands

1. **Identity and bytes:** keys and values are arbitrary binary strings, not UTF-8 identifiers. A key containing `0x00` or `0xff` must round-trip. No unrequested namespace or case normalization.
2. **Authoritative latest:** obtaining latest version `R` must obey the session's causal minimum. A worker with `applied_version < R` may wait/retry or return a typed unavailable error, but cannot return an older value or manufacture Redis null. An expired historical version also produces a typed unavailable error, not null.
3. **Single effect:** `SET` applies exactly one canonical point `Set`; `DEL` applies `Clear` only as specified by the serialized operation. An acknowledged command is durably committed. A lost reply with matching idempotency identity must recover its original outcome; no second logical commit.
4. **Delete/overwrite:** a tombstone hides all earlier values at or after its version; a later `SET` can create the key anew. Old snapshots stay exact while retained.
5. **No cross-cell claim:** the oracle assumes one cell; multi-cell snapshots, distributed Redis Cluster compatibility, and failover behavior need independent contracts.
6. **Error boundary:** unsupported command variants fail explicitly before mutation. No option may be accepted and silently dropped. RESP reply types and arity validation must be tested separately from storage correctness.

These invariants reuse [RFC-0002](0002-version-and-mvcc-model.md) and [RFC-0008](0008-transaction-isolation.md); they do not add Redis-specific mutation kinds to the transaction kernel.

## Independent differential-history fixture (proposed)

A reference oracle keeps a map of immutable snapshots indexed by `(generation, sequence)`, built from canonical committed events *without calling the candidate's MVCC implementation*. A query at version `R` reads only that oracle snapshot. The serving layer obtains `R` from the authority; test-only historical reads are used to grade storage invariants and are **not** a claimed Redis command.

| Step | Action / observation | Expected |
| --- | --- | --- |
| 1 | Commit `SET 0x00ff = 0x610062` at `(1,1)` | one committed effect, `OK` |
| 2 | Read same key at `(1,1)` | exact bytes `0x610062` |
| 3 | Commit `SET 0x00ff = 0x63` at `(1,2)` | `OK`; snapshot `(1,1)` still returns old bytes |
| 4 | Read latest at session causal floor `(1,2)` | `0x63`, never the prior bytes |
| 5 | Commit `DEL 0x00ff` at `(1,3)` | `1`; historical read at `(1,2)` still returns `0x63` |
| 6 | Read at `(1,3)` | null (actual tombstone); *not* an unavailable/error fallback |
| 7 | Commit `DEL 0x00ff` at `(1,4)` | `0` |
| 8 | Commit `SET 0x00ff = 0x64` at `(1,5)` | `OK`; current read `0x64`, previous tombstone stays effective at `(1,3)` |
| 9 | Drop response after a known committed `SET`; retry original client identity | original outcome replayed; exactly one committed effect |
| 10 | Ask a lagging worker for `(1,5)` when applied frontier is `(1,4)` | typed `version_not_applied`/retry, **never** null or `0x63` |
| 11 | Advance read floor to `(1,3)`, then ask for `(1,2)` | typed `version_too_old`; `(1,3)` remains readable |
| 12 | Send unsupported `SET key val EX 10` | explicit unsupported-command/option error **and zero writes** |

### Poisoned subjects (each must be detected)

- **Stale latest:** after step 3, serve the `(1,1)` cache entry as latest; fail at step 4.
- **Delete resurrection:** after step 5, expose older bytes from cache; fail at step 6.
- **Unavailable-to-null:** convert `version_not_applied` / `version_too_old` into Redis null; fail at steps 10/11.
- **Lost-reply double effect:** retry step 9 under a fresh identity or append twice; fail durable-outcome/effect-count comparison even if final value matches.
- **Unsupported-option acceptance:** silently ignore expiry flag; fail step 12 on protocol refusal and effect count.
- **Conflicting replay:** same commit version and identity with different fingerprint must halt rather than choose a winner, per RFC-0002.

The harness should record exact per-event input, selected version, reply/error class, accepted commit identity, and first divergence. Use deterministic seed/replay and freeze this grader before implementing a serving-model adapter.

## First bounded implementation slice

1. Review and accept the *matrix and oracle contract* first; reject or narrow any command whose Redis reply semantics are not proven.
2. Add a Redis-command adapter **above** `okv-model` for only `GET`, optionless `SET`, single-key `DEL`/`EXISTS`/`TYPE` and stateless `PING`/`ECHO`.
3. Grade against an independent full-snapshot oracle, including all poisoned subjects. Record schema/OTel artifacts under an isolated serving-model eval suite.
4. Only then evaluate conditional writes and one-version multi-key reads. Do not claim Redis compatibility from kernel tests or from a RESP parser alone.

## Primary references

- [Redis GET](https://redis.io/docs/latest/commands/get/), [SET](https://redis.io/docs/latest/commands/set/), [EXISTS](https://redis.io/docs/latest/commands/exists/)
- [Redis transactions (MULTI / EXEC / WATCH)](https://redis.io/docs/latest/develop/using-commands/transactions/)
- DOSS objectKV: [RFC-0002](0002-version-and-mvcc-model.md), [RFC-0008](0008-transaction-isolation.md), [Contributor Board](../docs/CONTRIBUTOR-BOARD.md) T8/T12
- Provenance: objectKV architectural contracts and issue framing credited to Wiley Jones/DOSS; this downstream RFC is a proposed review slice, not an upstream decision.
