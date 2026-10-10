//! Frozen independently specified receipt expectations. This test does not use
//! candidate verdicts, failure messages, minimization claims, or admission code.
use okv_sim::{run_cache_resurrection, CacheMode};
use serde_json::Value;

const VALUE_ONE_SHA256: &str = "fe6096ef976f8695e97b4e1227e2e97b8899fbf03c936c81265221f0d4feda8b";

struct Reference {
    name: &'static str,
    actions: &'static [&'static str],
    results: &'static [&'static str],
    generations: &'static [u64],
    roots: &'static [u64],
    leases: &'static [bool],
}

const REFERENCES: &[Reference] = &[
    Reference {
        name: "delete_then_read",
        actions: &["Authorize", "Populate", "Delete", "Read"],
        results: &["authorized", "published", "applied", "refused"],
        generations: &[1, 1, 1, 1],
        roots: &[1, 1, 2, 2],
        leases: &[true, true, true, true],
    },
    Reference {
        name: "overwrite_during_stream",
        actions: &[
            "Authorize",
            "Populate",
            "BeginStream",
            "Overwrite",
            "FinishStream",
        ],
        results: &[
            "authorized",
            "published",
            "stream_started",
            "applied",
            "refused",
        ],
        generations: &[1, 1, 1, 1, 1],
        roots: &[1, 1, 1, 2, 2],
        leases: &[true, true, true, true, true],
    },
    Reference {
        name: "delayed_population",
        actions: &["Authorize", "Overwrite", "Populate", "Read"],
        results: &["authorized", "applied", "refused", "refused"],
        generations: &[1, 1, 1, 1],
        roots: &[1, 2, 2, 2],
        leases: &[true, true, true, true],
    },
    Reference {
        name: "expired_lease_cache_hit",
        actions: &["Authorize", "Populate", "ExpireLease", "Read"],
        results: &["authorized", "published", "applied", "refused"],
        generations: &[1, 1, 1, 1],
        roots: &[1, 1, 1, 1],
        leases: &[true, true, false, false],
    },
    Reference {
        name: "released_lease_cache_hit",
        actions: &["Authorize", "Populate", "ReleaseLease", "Read"],
        results: &["authorized", "published", "applied", "refused"],
        generations: &[1, 1, 1, 1],
        roots: &[1, 1, 1, 1],
        leases: &[true, true, false, false],
    },
    Reference {
        name: "cache_loss_and_corruption",
        actions: &[
            "Authorize",
            "Populate",
            "Read",
            "LoseCache",
            "Read",
            "CorruptCache",
            "Read",
        ],
        results: &[
            "authorized",
            "published",
            "cache_hit",
            "applied",
            "backend_repair",
            "applied",
            "backend_repair",
        ],
        generations: &[1, 1, 1, 1, 1, 1, 1],
        roots: &[1, 1, 1, 1, 1, 1, 1],
        leases: &[true, true, true, true, true, true, true],
    },
    Reference {
        name: "stale_generation_population",
        actions: &["Authorize", "AdvanceGeneration", "Populate", "Read"],
        results: &["authorized", "applied", "refused", "refused"],
        generations: &[1, 2, 2, 2],
        roots: &[1, 1, 1, 1],
        leases: &[true, true, true, true],
    },
    Reference {
        name: "stale_root_resurrection",
        actions: &[
            "Authorize",
            "Populate",
            "Overwrite",
            "LoseCache",
            "RestoreOldRoot",
            "Read",
        ],
        results: &[
            "authorized",
            "published",
            "applied",
            "applied",
            "applied",
            "refused",
        ],
        generations: &[1, 1, 1, 1, 1, 1],
        roots: &[1, 1, 2, 2, 2, 2],
        leases: &[true, true, true, true, true, true],
    },
    Reference {
        name: "successful_stream_completion",
        actions: &["Authorize", "Populate", "BeginStream", "FinishStream"],
        results: &[
            "authorized",
            "published",
            "stream_started",
            "stream_returned",
        ],
        generations: &[1, 1, 1, 1],
        roots: &[1, 1, 1, 1],
        leases: &[true, true, true, true],
    },
];

fn matches_reference(receipt: &Value) -> Result<(), String> {
    let histories = receipt["histories"].as_array().ok_or("missing histories")?;
    if histories.len() != REFERENCES.len() {
        return Err("history coverage differs".to_owned());
    }
    for expected in REFERENCES {
        let matches: Vec<_> = histories
            .iter()
            .filter(|h| h["name"] == expected.name)
            .collect();
        if matches.len() != 1 {
            return Err(format!("missing or duplicate {}", expected.name));
        }
        let events = matches[0]["events"].as_array().ok_or("missing events")?;
        if events.len() != expected.actions.len() {
            return Err(format!("{} coverage differs", expected.name));
        }
        for (i, event) in events.iter().enumerate() {
            let returns_bytes = matches!(
                expected.results[i],
                "cache_hit" | "backend_repair" | "stream_returned"
            );
            let expected_hash = if returns_bytes {
                Value::from(VALUE_ONE_SHA256)
            } else {
                Value::Null
            };
            if event["action"] != expected.actions[i]
                || event["result"] != expected.results[i]
                || event["generation"] != expected.generations[i]
                || event["root"] != expected.roots[i]
                || event["lease_active"] != expected.leases[i]
                || event["step"] != i
                || event["returned_sha256"] != expected_hash
            {
                return Err(format!("{} step {i} differs: {event}", expected.name));
            }
        }
    }
    Ok(())
}

#[test]
fn correct_subject_satisfies_external_reference_including_stream_progress() {
    for seed in [1103, 2027, 4099] {
        let report = run_cache_resurrection(seed, "frozen-reference", CacheMode::Correct).unwrap();
        let receipt = serde_json::to_value(report).unwrap();
        assert_eq!(matches_reference(&receipt), Ok(()));
    }
}

#[test]
fn external_reference_rejects_each_unsafe_subject_without_trusting_verdict() {
    for mode in [
        CacheMode::SkipPublicationFence,
        CacheMode::SkipUseFence,
        CacheMode::TrustCacheBytes,
        CacheMode::RepairWrongVersion,
    ] {
        let report = run_cache_resurrection(1103, "frozen-reference", mode).unwrap();
        assert!(matches_reference(&serde_json::to_value(report).unwrap()).is_err());
    }
}

#[test]
fn external_reference_rejects_always_refuse_mutant() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_okv-sim"))
        .args([
            "cache-replay",
            "--source-revision",
            "frozen-reference",
            "--mode",
            "always-refuse",
        ])
        .output()
        .unwrap();
    let receipt: Value = serde_json::from_slice(&output.stdout)
        .expect("mutant must execute and emit a receipt, not fail argument parsing");
    assert_eq!(output.status.code(), Some(2));
    assert!(matches_reference(&receipt).is_err());
}

#[test]
fn oracle_rejects_forged_verdicts_and_each_missing_progress_observation() {
    let report = run_cache_resurrection(1103, "frozen-reference", CacheMode::Correct).unwrap();
    let original = serde_json::to_value(report).unwrap();
    assert!(matches_reference(&original).is_ok());
    for (history_index, history) in original["histories"].as_array().unwrap().iter().enumerate() {
        for (event_index, event) in history["events"].as_array().unwrap().iter().enumerate() {
            if event["returned_sha256"].is_null() {
                continue;
            }
            let mut refused = original.clone();
            let target = &mut refused["histories"][history_index]["events"][event_index];
            target["result"] = Value::from("refused");
            target["returned_sha256"] = Value::Null;
            target["failure"] = Value::Null;
            assert!(matches_reference(&refused).is_err());
            let mut wrong = original.clone();
            wrong["histories"][history_index]["events"][event_index]["returned_sha256"] =
                Value::from("wrong");
            assert!(matches_reference(&wrong).is_err());
        }
    }
}
