use okv_sim::{run_cache_resurrection, CacheMode};

const SEEDS: [u64; 3] = [1103, 2027, 4099];

#[test]
fn correct_cache_histories_replay_and_never_return_wrong_bytes() {
    for seed in SEEDS {
        let report = run_cache_resurrection(seed, "fixture", CacheMode::Correct).unwrap();
        assert!(report.passed(), "{report:#?}");
        assert_eq!(report, run_cache_resurrection(seed, "fixture", CacheMode::Correct).unwrap());
        assert_eq!(report.histories.len(), 8);
        assert!(report.histories.iter().flat_map(|h| &h.events).any(|e| e.result == "cache_hit"));
        assert!(report.histories.iter().flat_map(|h| &h.events).any(|e| e.result == "backend_repair"));
    }
}

#[test]
fn independently_poisoned_subjects_have_bounded_minimal_counterexamples() {
    for mode in [CacheMode::SkipPublicationFence, CacheMode::SkipUseFence,
                 CacheMode::TrustCacheBytes, CacheMode::RepairWrongVersion] {
        let report = run_cache_resurrection(1103, "fixture", mode).unwrap();
        assert!(!report.passed(), "undetected {mode:?}");
        assert_eq!(report, run_cache_resurrection(1103, "fixture", mode).unwrap());
        let witness = report.histories.iter().find(|h| !h.failures.is_empty()).unwrap();
        assert!(witness.events.len() <= 8);
        assert!(!witness.minimized_actions.is_empty());
        assert!(witness.minimized_is_one_minimal);
    }
}
