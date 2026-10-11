# Candidate result

The RED crate regression is commit
`6812b7780889b67c57852f3579353583dd35b815`, based on Kevin Rajan's
characterization commit `bbbd61df49c6c665723e4b6d0b2b3455d2e1125f`.
The separate implementation candidate is
`f861bfa0d4aae16257fd21e2f987f5eebe9a190c`. It changes only
`bounded_count` in `crates/okv-eval/src/main.rs`: a `u64` is converted directly
to the schema's `f64` value instead of silently saturating to `u32::MAX`.

Commands below used the same `CARGO_HOME`, `RUSTUP_HOME`, `PATH`,
`CARGO_TARGET_DIR`, test/dev debug settings, incremental setting, and build-job
setting documented in `README.md`, with Rust 1.88.0 and unchanged `Cargo.lock`.

| Check | Baseline | Candidate |
| --- | --- | --- |
| `cargo test -p okv-eval --bin okv-eval count_conversion_tests --locked -- --nocapture` | exit 101; 3 passed, 5 failed | exit 0; 8 passed, 0 failed |
| `cargo fmt --all -- --check` | exit 0 | exit 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | not run | exit 0 |
| `cargo test --workspace --locked` | not run | exit 0; 189 passed, 0 failed across 34 test summaries |
| `cargo run -p okv-eval --locked -- smoke` | not run | exit 0; `status=pass`, `correctness_failures=0` |
| `cargo run -p okv-eval --locked -- validate-suite evals/suites/smoke.toml` | not run | exit 0; 143 metrics, one lane, one workload, one profile |

`receipts/red-test.txt` and `receipts/green-test.txt` retain the test-run
sections of the two real crate outputs; only a terminal blank line was removed
from the GREEN receipt. The original eight-fixture
oracle remains byte-for-byte unchanged at SHA-256
`6803ea237b39c318390d75bf1bc6ace5e63dabad15bd07548b4fa627a49bdfe0`.
The result schema remains byte-for-byte unchanged at SHA-256
`609bb372d51c18fa3cbb8b704a4eb7b037440892be03ca5988755f86ab36b501`.

This test verifies numeric reporting in compiled crate code; it does not run a
workload with multi-gigabyte transfers or establish a performance/economics
result. The `f64` result contract still rounds some integers above 2^53.
Wiley Jones's historical intent for the previous `u32` cap is not known.
After independent review by a separate OpenAI AI critic, the original packet
was published to the downstream `kvnloo/objectKV` branch
`research/eval/count-conversion-real-crate` at
[commit `69fa56f3eec37c871422862573b700905b8bc0f4`](https://github.com/kvnloo/objectKV/commit/69fa56f3eec37c871422862573b700905b8bc0f4),
tree `9b38894303f1b601b936af6ad13523c01def309a`. The earlier evidence branch
remained at `bbbd61d`. No upstream post, PR, or merge was made as part of this
work. The 189 workspace test passes apply to this count-conversion branch;
they do not cover a composition with the separate route-refresh branch.

Wiley Jones is credited for objectKV architecture and frozen evaluation
contracts; Kevin Rajan supplied the earlier exact-source characterization.
This crate test, correction, and report were prepared by Codex, an OpenAI AI
assistant.
The original RED, GREEN, and report commits identify both author and committer
as `Codex (AI assistant)`.
This receipt correction was prepared by dot, an OpenAI AI assistant, with an
explicit AI author identity and no DCO attestation on Kevin's behalf.
