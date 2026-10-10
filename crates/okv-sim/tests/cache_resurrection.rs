use okv_sim::{run_cache_resurrection, CacheMode};

const SEEDS: [u64; 3] = [1103, 2027, 4099];

#[test]
fn correct_cache_histories_replay_and_never_return_wrong_bytes() {
    for seed in SEEDS {
        let report = run_cache_resurrection(seed, "fixture", CacheMode::Correct).unwrap();
        assert!(report.passed(), "{report:#?}");
        assert_eq!(
            report,
            run_cache_resurrection(seed, "fixture", CacheMode::Correct).unwrap()
        );
        assert_eq!(report.histories.len(), 8);
        assert!(report
            .histories
            .iter()
            .flat_map(|h| &h.events)
            .any(|e| e.result == "cache_hit"));
        assert!(report
            .histories
            .iter()
            .flat_map(|h| &h.events)
            .any(|e| e.result == "backend_repair"));
    }
}

#[test]
fn independently_poisoned_subjects_have_bounded_minimal_counterexamples() {
    for mode in [
        CacheMode::SkipPublicationFence,
        CacheMode::SkipUseFence,
        CacheMode::TrustCacheBytes,
        CacheMode::RepairWrongVersion,
    ] {
        let report = run_cache_resurrection(1103, "fixture", mode).unwrap();
        assert!(!report.passed(), "undetected {mode:?}");
        assert_eq!(
            report,
            run_cache_resurrection(1103, "fixture", mode).unwrap()
        );
        let witness = report
            .histories
            .iter()
            .find(|h| !h.failures.is_empty())
            .unwrap();
        assert!(witness.events.len() <= 8);
        assert!(!witness.minimized_actions.is_empty());
        assert!(witness.minimized_is_one_minimal);
    }
}

#[test]
fn authority_change_between_authorize_and_publish_is_observable() {
    for mode in [CacheMode::Correct, CacheMode::SkipPublicationFence] {
        let report = run_cache_resurrection(1103, "fixture", mode).unwrap();
        let history = report
            .histories
            .iter()
            .find(|h| h.name == "stale_generation_population")
            .unwrap();
        assert_eq!(history.events[0].generation, 1);
        assert_eq!(history.events[1].generation, 2);
        assert_eq!(history.events[2].generation, 2);
        assert_eq!(
            history.events[2].result,
            if mode == CacheMode::Correct {
                "refused"
            } else {
                "published"
            }
        );
        assert_eq!(
            history.events[2].failure.is_some(),
            mode != CacheMode::Correct
        );
        assert_eq!(history.events[3].result, "refused");
        assert!(history.events[3].returned_sha256.is_none());
    }
}

#[test]
fn fresh_process_replay_and_poison_exit_status_match_receipts() {
    use std::process::Command;
    let invoke = |mode: &str| {
        Command::new(env!("CARGO_BIN_EXE_okv-sim"))
            .args([
                "cache-replay",
                "--seed",
                "1103",
                "--source-revision",
                "fixture",
                "--mode",
                mode,
            ])
            .output()
            .unwrap()
    };
    let first = invoke("correct");
    let second = invoke("correct");
    assert!(first.status.success());
    assert!(second.status.success());
    assert_eq!(first.stdout, second.stdout);
    let poison = invoke("skip-publication-fence");
    assert_eq!(poison.status.code(), Some(2));
    let receipt: serde_json::Value = serde_json::from_slice(&poison.stdout).unwrap();
    assert!(receipt["histories"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| !h["failures"].as_array().unwrap().is_empty()));
}
