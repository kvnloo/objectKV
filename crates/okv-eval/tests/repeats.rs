use okv_eval::repeats::{classify, NoisePolicy, NoiseVerdict};

#[test]
fn stable_samples_are_comparable_not_a_benchmark_winner() {
    let summary = classify(&[100.0, 101.0, 99.0, 100.0, 100.0], NoisePolicy::default()).unwrap();
    assert_eq!(summary.verdict, NoiseVerdict::Comparable);
    assert_eq!(summary.median, Some(100.0));
    assert_eq!(summary.mad, Some(0.0));
}

#[test]
fn zero_mad_does_not_hide_an_outlier() {
    let summary = classify(
        &[100.0, 100.0, 100.0, 100.0, 1000.0],
        NoisePolicy::default(),
    )
    .unwrap();
    assert_eq!(summary.verdict, NoiseVerdict::Noisy);
}

#[test]
fn bimodal_and_insufficient_controls_cannot_be_comparable() {
    assert_eq!(
        classify(&[10.0, 10.0, 100.0, 100.0], NoisePolicy::default())
            .unwrap()
            .verdict,
        NoiseVerdict::Noisy
    );
    assert_eq!(
        classify(&[10.0], NoisePolicy::default()).unwrap().verdict,
        NoiseVerdict::Insufficient
    );
    assert_eq!(classify(&[], NoisePolicy::default()).unwrap().median, None);
}

#[test]
fn invalid_policy_and_nonfinite_samples_are_rejected() {
    assert!(classify(&[f64::NAN], NoisePolicy::default()).is_err());
    assert!(classify(&[f64::MAX, -f64::MAX, -f64::MAX], NoisePolicy::default()).is_err());
    assert!(classify(
        &[1.0],
        NoisePolicy {
            relative_tolerance: -1.0,
            absolute_tolerance: 0.0
        }
    )
    .is_err());
}

#[test]
fn stable_zero_metric_and_deterministic_receipt() {
    let a = classify(&[0.0, 0.0, 0.0], NoisePolicy::default()).unwrap();
    assert_eq!(a.verdict, NoiseVerdict::Comparable);
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(classify(&[0.0, 0.0, 0.0], NoisePolicy::default()).unwrap()).unwrap()
    );
    assert_eq!(
        classify(&[0.0, 0.0, 1.0], NoisePolicy::default())
            .unwrap()
            .verdict,
        NoiseVerdict::Noisy
    );
}

#[cfg(unix)]
mod processes {
    use okv_eval::config::load_suite;
    use okv_eval::repeats::{run_repeats, NoisePolicy, RepeatOptions};
    use okv_eval::result::validate_result;
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::Duration;

    fn fixture(mode: &str) -> (tempfile::TempDir, RepeatOptions) {
        let temp = tempfile::tempdir().unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let suite = root.join("evals/suites/smoke.toml");
        let loaded = load_suite(&suite).unwrap();
        let mut hasher = Sha256::new();
        for bytes in [
            &loaded.suite_bytes,
            &fs::read(&loaded.registry_path).unwrap(),
            &fs::read(&loaded.result_schema_path).unwrap(),
        ] {
            hasher.update(u64::try_from(bytes.len()).unwrap().to_be_bytes());
            hasher.update(bytes);
        }
        let template = json!({
            "schema_version": 1, "run_id": "FIXTURE_RUN", "created_at": "2026-10-10T00:00:00Z",
            "lane": "correctness", "suite": loaded.suite.id, "suite_hash": format!("{:x}", hasher.finalize()),
            "profile": {"id": "dev", "hash": format!("{:x}", Sha256::digest(toml::to_string(&loaded.suite.profiles["dev"]).unwrap().as_bytes())), "machine": "fixture", "rustc": "fixture", "lockfile_hash": "fixture"},
            "candidate_commit": "abcdef0123456789", "parent_commit": "1234567890abcdef", "backend": "model", "seeds": [1103],
            "budget": {"kind": "operations", "limit": 10.0, "observed": 1.0},
            "hard_gates": [{"id": "fixture", "status": "pass"}],
            "primary_metric": {"name": "okv.correctness.failures", "unit": "{failure}", "direction": "lower", "samples": [987_654_321], "median": 987_654_321, "mad": 0},
            "secondary_metrics": {}, "verdict": "keep", "reason": "fixture only", "artifact_refs": []
        });
        // Use the actual registry contract, without redefining any instrument.
        let mut template = template;
        let metric = loaded
            .registry
            .metrics
            .iter()
            .find(|m| m.id == "correctness.failures")
            .unwrap();
        template["primary_metric"]["name"] = json!(metric.otel_name);
        template["primary_metric"]["unit"] = json!(metric.unit);
        fs::write(
            temp.path().join("template.json"),
            serde_json::to_vec(&template).unwrap(),
        )
        .unwrap();
        fs::write(temp.path().join("mode"), mode).unwrap();
        let executable = temp.path().join("child.sh");
        fs::write(&executable, r#"#!/bin/sh
set -eu
base=$(dirname "$0")
mode=$(cat "$base/mode")
while [ "$1" != "--output" ]; do shift; done
output=$2
value=100
case "$output" in *warmup*) value=100000;; esac
case "$mode:$output" in
  outlier:*measurement-0004*) value=1000;;
  bimodal:*measurement-000[34]*) value=1000;;
  failed:*measurement-0001*) echo deliberate-failure >&2; exit 7;;
  invalid:*measurement-0001*) echo '{bad json' > "$output"; exit 0;;
  timeout:*) sleep 1;;
esac
sed "s/FIXTURE_RUN/$$/g; s/987654321/$value/g" "$base/template.json" > "$output"
case "$mode:$output" in
  identity:*measurement-0001*) sed 's/abcdef0123456789/abcdef0987654321/' "$output" > "$output.tmp"; mv "$output.tmp" "$output";;
  discard:*measurement-0001*) sed 's/"keep"/"discard"/' "$output" > "$output.tmp"; mv "$output.tmp" "$output";;
esac
if [ "$mode" = executable ]; then printf "\n# changed executable\n" >> "$0"; fi
"#).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let options = RepeatOptions {
            executable,
            suite,
            profile: "dev".into(),
            workload: "model-smoke".into(),
            backend: "model".into(),
            output_dir: temp.path().join("results"),
            repeats: Some(5),
            warmups: 1,
            timeout: Duration::from_secs(5),
            policy: NoisePolicy::default(),
            allow_dirty: false,
        };
        (temp, options)
    }

    #[test]
    fn isolated_processes_keep_identity_and_separate_warmup() {
        let (_temp, options) = fixture("stable");
        let receipt = run_repeats(&options).unwrap();
        assert!(receipt.comparable, "{receipt:#?}");
        assert_eq!(receipt.verdict, "inconclusive");
        assert_eq!(receipt.summary.samples, vec![100.0; 5]);
        let pids: BTreeSet<_> = receipt
            .attempts
            .iter()
            .map(|a| a.process_id.unwrap())
            .collect();
        assert_eq!(pids.len(), 6);
        assert_eq!(receipt.attempts.len(), 6);
        assert!(receipt
            .attempts
            .iter()
            .all(|a| options.output_dir.join(&a.result_ref).is_file()));
        let loaded = load_suite(&options.suite).unwrap();
        validate_result(
            &loaded.result_schema_path,
            receipt.aggregate.as_ref().unwrap(),
        )
        .unwrap();
        assert_eq!(receipt.aggregate.unwrap()["budget"]["observed"], 1.0);
        assert!(
            run_repeats(&options).is_err(),
            "must not overwrite raw attempts"
        );
    }

    #[test]
    fn noisy_fixtures_discard_without_dropping_outliers() {
        for mode in ["outlier", "bimodal"] {
            let (_temp, options) = fixture(mode);
            let receipt = run_repeats(&options).unwrap();
            assert!(!receipt.comparable);
            assert_eq!(receipt.verdict, "discard");
            assert_eq!(receipt.summary.samples.len(), 5);
            assert_eq!(receipt.attempts.len(), 6);
        }
    }

    #[test]
    fn failures_invalid_results_identity_drift_and_discards_are_retained() {
        for mode in ["failed", "invalid", "identity", "discard", "executable"] {
            let (_temp, options) = fixture(mode);
            let receipt = run_repeats(&options).unwrap();
            assert_eq!(receipt.verdict, "discard", "{mode}");
            assert_eq!(receipt.attempts.len(), 6);
            let failed = &receipt.attempts[2];
            assert_eq!(failed.index, 1);
            if mode != "discard" {
                assert!(failed.error.is_some());
            }
            assert!(options.output_dir.join(&failed.stderr_ref).is_file());
            assert!(options.output_dir.join("attempts.json").is_file());
            assert!(receipt.attempts.last().unwrap().result.is_some());
        }
    }

    #[test]
    fn timeouts_and_spawn_errors_preserve_receipt_without_invented_aggregate() {
        for mode in ["timeout", "spawn"] {
            let (_temp, mut options) = fixture(mode);
            options.warmups = 0;
            options.repeats = Some(2);
            options.timeout = Duration::from_millis(20);
            if mode == "spawn" {
                options.executable = options.executable.with_file_name("missing");
            }
            let receipt = run_repeats(&options).unwrap();
            assert_eq!(receipt.verdict, "discard");
            assert!(receipt.aggregate.is_none());
            assert_eq!(receipt.attempts.len(), 2);
            assert!(receipt.attempts.iter().all(|a| a.error.is_some()));
        }
    }

    #[test]
    fn profile_count_is_reused_and_zero_override_rejected() {
        let (_temp, mut options) = fixture("stable");
        options.repeats = Some(0);
        assert!(run_repeats(&options).is_err());
        assert!(!options.output_dir.exists());
        options.repeats = None;
        let receipt = run_repeats(&options).unwrap();
        assert_eq!(receipt.repeats, 1);
        assert!(!receipt.comparable);
        assert_eq!(receipt.verdict, "inconclusive");
    }
}

#[test]
fn repeated_observations_reuse_metric_series_and_reject_attempt_labels() {
    use okv_eval::config::load_suite;
    use okv_eval::telemetry::{RunResource, Telemetry};
    use std::collections::BTreeMap;
    use std::path::Path;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut loaded = load_suite(&root.join("evals/suites/smoke.toml")).unwrap();
    loaded.suite.telemetry.endpoint_env = "OKV_REPEAT_TEST_UNCONFIGURED_ENDPOINT".into();
    loaded.registry.cardinality.max_series_per_run = 1;
    let resource = RunResource {
        service_version: "test".into(),
        environment: "dev".into(),
        run_id: "repeat-test".into(),
        suite_id: loaded.suite.id.clone(),
        suite_hash: "test".into(),
        profile_id: "dev".into(),
        profile_hash: "test".into(),
        candidate_commit: "abcdef0".into(),
        backend: "model".into(),
    };
    let telemetry = Telemetry::init(&loaded.suite.telemetry, "dev", &resource).unwrap();
    let mut recorder = telemetry.recorder(&loaded.registry);
    let attributes = BTreeMap::from([
        ("lane".into(), "correctness".into()),
        ("workload".into(), "model-smoke".into()),
    ]);
    for _ in 0..100 {
        recorder
            .record("correctness.failures", 0.0, attributes.clone())
            .unwrap();
    }
    for key in ["attempt", "process_id", "seed", "artifact_path"] {
        let mut invalid = attributes.clone();
        invalid.insert(key.into(), "1".into());
        assert!(recorder
            .record("correctness.failures", 0.0, invalid)
            .is_err());
    }
    assert_eq!(recorder.samples("correctness.failures").len(), 100);
    telemetry.shutdown();
}
