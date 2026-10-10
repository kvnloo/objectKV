//! Opt-in process isolation for eligible single-pass evaluation workloads.
//!
//! This orchestration grades repeat comparability, never a benchmark winner.
use crate::config::{load_suite, LoadedSuite};
use crate::result::{median, median_absolute_deviation, validate_result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::error::Error;
use std::fs::{self, File};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct NoisePolicy {
    pub relative_tolerance: f64,
    pub absolute_tolerance: f64,
}

impl Default for NoisePolicy {
    fn default() -> Self {
        Self {
            relative_tolerance: 0.1,
            absolute_tolerance: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoiseVerdict {
    Comparable,
    Noisy,
    Insufficient,
}

#[derive(Debug, Serialize)]
pub struct NoiseSummary {
    pub samples: Vec<f64>,
    pub median: Option<f64>,
    pub mad: Option<f64>,
    pub max_deviation: Option<f64>,
    pub verdict: NoiseVerdict,
}

/// Classify all samples without trimming outliers or selecting a winner.
///
/// # Errors
/// Returns an error for non-finite samples or invalid tolerances.
pub fn classify(samples: &[f64], policy: NoisePolicy) -> Result<NoiseSummary, Box<dyn Error>> {
    if [policy.relative_tolerance, policy.absolute_tolerance]
        .iter()
        .any(|n| !n.is_finite() || *n < 0.0)
        || samples.iter().any(|n| !n.is_finite())
    {
        return Err("samples must be finite and tolerances finite and non-negative".into());
    }
    let center = (!samples.is_empty()).then(|| median(samples));
    let mad = center.map(|m| median_absolute_deviation(samples, m));
    let max_deviation = center.map(|m| samples.iter().map(|n| (n - m).abs()).fold(0.0, f64::max));
    if center
        .into_iter()
        .chain(mad)
        .chain(max_deviation)
        .any(|value| !value.is_finite())
    {
        return Err("derived repeat statistics must remain finite".into());
    }
    let noisy = center.zip(max_deviation).is_some_and(|(m, d)| {
        d > policy
            .absolute_tolerance
            .max(m.abs() * policy.relative_tolerance)
    });
    Ok(NoiseSummary {
        samples: samples.to_vec(),
        median: center,
        mad,
        max_deviation,
        verdict: if noisy {
            NoiseVerdict::Noisy
        } else if samples.len() < 3 {
            NoiseVerdict::Insufficient
        } else {
            NoiseVerdict::Comparable
        },
    })
}

#[derive(Debug)]
pub struct RepeatOptions {
    pub executable: PathBuf,
    pub suite: PathBuf,
    pub profile: String,
    pub workload: String,
    pub backend: String,
    pub output_dir: PathBuf,
    /// None uses the existing profile's repeat count, without rewriting it.
    pub repeats: Option<u32>,
    pub warmups: u32,
    pub timeout: Duration,
    pub policy: NoisePolicy,
    pub allow_dirty: bool,
}

#[derive(Debug, Serialize)]
pub struct Attempt {
    pub phase: &'static str,
    pub index: u32,
    pub process_id: Option<u32>,
    pub result_ref: String,
    pub stdout_ref: String,
    pub stderr_ref: String,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
    pub result: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RepeatReceipt {
    pub contract_version: u32,
    pub executable_sha256: Option<String>,
    pub timeout_seconds: f64,
    pub workload: String,
    pub repeats: u32,
    pub warmups: u32,
    pub policy: NoisePolicy,
    pub summary: NoiseSummary,
    pub comparable: bool,
    pub verdict: &'static str,
    pub reason: &'static str,
    pub attempts: Vec<Attempt>,
    pub aggregate: Option<Value>,
}

/// Run each attempt in a fresh OS process; retain warmups, failures and discards.
/// Existing output directories are refused, never overwritten.
///
/// # Errors
/// Returns an error on invalid configuration or artifact I/O. Individual child
/// failures remain in the receipt and do not prevent subsequent attempts.
#[allow(clippy::too_many_lines)]
pub fn run_repeats(options: &RepeatOptions) -> Result<RepeatReceipt, Box<dyn Error>> {
    classify(&[], options.policy)?;
    if options.timeout.is_zero() {
        return Err("timeout must be positive".into());
    }
    let loaded = load_suite(&options.suite)?;
    let profile = loaded
        .suite
        .profiles
        .get(&options.profile)
        .ok_or("unknown profile")?;
    let workload = loaded
        .suite
        .workloads
        .iter()
        .find(|w| w.id == options.workload)
        .ok_or("unknown workload")?;
    // Other lanes may already repeat internally or blend lifecycle phases.
    if !matches!(
        workload.operation.as_str(),
        "model_smoke" | "deterministic_generation_recovery" | "commit_envelope_contract"
    ) {
        return Err("workload is not admitted for generic single-pass repeats".into());
    }
    if options.warmups > 0
        && loaded
            .suite
            .telemetry
            .required_for_profiles
            .contains(&options.profile)
    {
        return Err("warmups require a local profile with optional telemetry".into());
    }
    let repeats = options.repeats.unwrap_or(profile.repeats);
    if repeats == 0 {
        return Err("repeat count must be positive".into());
    }
    let lane = loaded
        .suite
        .lanes
        .iter()
        .find(|l| l.id == workload.lane)
        .ok_or("unknown lane")?;
    let metric = loaded
        .registry
        .metrics
        .iter()
        .find(|m| m.id == lane.primary_metric)
        .ok_or("unknown metric")?;
    let seeds = &loaded
        .suite
        .dataset
        .get(&options.profile)
        .or_else(|| loaded.suite.dataset.values().next())
        .ok_or("missing dataset")?
        .seeds;
    let expected = json!({
        "suite": loaded.suite.id, "suite_hash": contract_hash(&loaded)?,
        "profile_id": options.profile, "profile_hash": format!("{:x}", Sha256::digest(toml::to_string(profile)?.as_bytes())),
        "seeds": seeds, "backend": options.backend, "lane": lane.id,
        "metric_name": metric.otel_name, "metric_unit": metric.unit, "metric_direction": lane.direction,
        "budget_kind": profile.budget_kind, "budget_limit": profile.budget_limit,
    });
    let executable_sha256 = executable_digest(&options.executable);
    fs::create_dir(&options.output_dir)?;
    let mut attempts = Vec::new();
    let mut run_ids = BTreeSet::new();
    let mut identity = None;
    for (phase, count) in [("warmup", options.warmups), ("measurement", repeats)] {
        for index in 0..count {
            let mut attempt = run_attempt(options, &loaded, phase, index)?;
            if executable_digest(&options.executable) != executable_sha256 {
                attempt.error = Some("executable changed during repeat execution".to_owned());
            }
            if attempt.error.is_none() {
                if let Some(value) = &attempt.result {
                    let validation = check_result(value, &loaded, &expected);
                    if let Err(error) = validation {
                        attempt.error = Some(error.to_string());
                    } else {
                        let current = run_identity(value);
                        if identity
                            .as_ref()
                            .is_some_and(|previous| previous != &current)
                        {
                            attempt.error =
                                Some("repeat source or environment identity changed".to_owned());
                        } else {
                            identity = Some(current);
                        }
                        if !run_ids.insert(value["run_id"].as_str().unwrap_or_default().to_owned())
                        {
                            attempt.error = Some("child reused a run identity".to_owned());
                        }
                    }
                }
            }
            attempts.push(attempt);
            // Checkpoint after every attempt, so completed work survives an interrupted parent.
            fs::write(
                options.output_dir.join("attempts.json"),
                serde_json::to_vec_pretty(&attempts)?,
            )?;
        }
    }
    let samples: Vec<f64> = attempts
        .iter()
        .filter(|a| a.phase == "measurement" && a.error.is_none())
        .filter_map(|a| {
            a.result
                .as_ref()?
                .get("primary_metric")?
                .get("median")?
                .as_f64()
        })
        .collect();
    let summary = classify(&samples, options.policy)?;
    let failed = attempts.iter().any(|a| {
        a.error.is_some()
            || a.result.as_ref().is_some_and(|r| {
                r["verdict"] == "discard"
                    || r["verdict"] == "crash"
                    || r["hard_gates"]
                        .as_array()
                        .is_some_and(|g| g.iter().any(|g| g["status"] != "pass"))
            })
    });
    let diagnostic = attempts.iter().any(|a| {
        a.result.as_ref().is_some_and(|r| {
            r["verdict"] == "inconclusive"
                || r["candidate_commit"]
                    .as_str()
                    .is_some_and(|s| s.ends_with("+dirty"))
        })
    });
    let comparable = !failed && !diagnostic && summary.verdict == NoiseVerdict::Comparable;
    let (verdict, reason) = if failed {
        (
            "discard",
            "at least one attempt failed; every attempt is retained",
        )
    } else if summary.verdict == NoiseVerdict::Noisy {
        ("discard", "noisy repeat distribution; no samples trimmed")
    } else if diagnostic {
        ("inconclusive", "diagnostic or non-comparable child result")
    } else if summary.verdict == NoiseVerdict::Insufficient {
        (
            "inconclusive",
            "at least three measured processes are required",
        )
    } else {
        (
            "inconclusive",
            "comparable repeats; winner selection requires a separate incumbent comparison",
        )
    };
    let mut aggregate = attempts
        .iter()
        .find(|a| a.phase == "measurement" && a.error.is_none())
        .and_then(|a| a.result.clone());
    if let Some(result) = aggregate.as_mut() {
        result["run_id"] = json!(uuid::Uuid::new_v4().to_string());
        result["primary_metric"]["samples"] = json!(summary.samples);
        result["primary_metric"]["median"] = json!(summary.median);
        result["primary_metric"]["mad"] = json!(summary.mad);
        result["primary_metric"]
            .as_object_mut()
            .ok_or("missing primary metric")?
            .remove("incumbent_median");
        result["secondary_metrics"] = json!({});
        result["verdict"] = json!(verdict);
        result["reason"] = json!(reason);
        result["hard_gates"].as_array_mut().ok_or("missing hard gates")?.push(json!({"id": "repeat_comparability", "status": if comparable { "pass" } else { "fail" }, "detail": reason}));
        // Budget remains per process, not a blended warmup + measurement budget.
        let observed = attempts
            .iter()
            .filter(|a| a.phase == "measurement")
            .filter_map(|a| a.result.as_ref()?["budget"]["observed"].as_f64())
            .fold(0.0, f64::max);
        result["budget"]["observed"] = json!(observed);
        result["artifact_refs"] = json!(["receipt.json", "attempts.json"]);
        validate_result(&loaded.result_schema_path, result)?;
        fs::write(
            options.output_dir.join("aggregate.json"),
            serde_json::to_vec_pretty(result)?,
        )?;
    }
    let receipt = RepeatReceipt {
        contract_version: 1,
        executable_sha256,
        timeout_seconds: options.timeout.as_secs_f64(),
        workload: options.workload.clone(),
        repeats,
        warmups: options.warmups,
        policy: options.policy,
        summary,
        comparable,
        verdict,
        reason,
        attempts,
        aggregate,
    };
    fs::write(
        options.output_dir.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

fn run_identity(value: &Value) -> Value {
    json!({"candidate": value["candidate_commit"], "parent": value["parent_commit"], "profile": value["profile"], "budget_kind": value["budget"]["kind"], "budget_limit": value["budget"]["limit"]})
}

fn check_result(
    value: &Value,
    loaded: &LoadedSuite,
    expected: &Value,
) -> Result<(), Box<dyn Error>> {
    validate_result(&loaded.result_schema_path, value)?;
    let actual = json!({
        "suite": value["suite"], "suite_hash": value["suite_hash"],
        "profile_id": value["profile"]["id"], "profile_hash": value["profile"]["hash"],
        "seeds": value["seeds"], "backend": value["backend"], "lane": value["lane"],
        "metric_name": value["primary_metric"]["name"], "metric_unit": value["primary_metric"]["unit"], "metric_direction": value["primary_metric"]["direction"],
        "budget_kind": value["budget"]["kind"], "budget_limit": value["budget"]["limit"],
    });
    if &actual != expected {
        return Err("child suite/profile/workload lane/backend/seed identity mismatch".into());
    }
    let samples = value["primary_metric"]["samples"]
        .as_array()
        .ok_or("missing samples")?
        .iter()
        .map(|v| v.as_f64().ok_or("invalid sample"))
        .collect::<Result<Vec<_>, _>>()?;
    if samples.is_empty() {
        return Err("empty sample set".into());
    }
    let center = median(&samples);
    if value["primary_metric"]["median"].as_f64() != Some(center)
        || value["primary_metric"]["mad"].as_f64()
            != Some(median_absolute_deviation(&samples, center))
    {
        return Err("child summary disagrees with raw samples".into());
    }
    Ok(())
}

fn run_attempt(
    options: &RepeatOptions,
    loaded: &LoadedSuite,
    phase: &'static str,
    index: u32,
) -> Result<Attempt, Box<dyn Error>> {
    let relative = format!("{phase}-{index:04}");
    let dir = options.output_dir.join(&relative);
    fs::create_dir(&dir)?;
    let output_path = dir.join("result.json");
    let mut command = Command::new(&options.executable);
    command
        .arg("run")
        .arg(&options.suite)
        .args([
            "--profile",
            &options.profile,
            "--workload",
            &options.workload,
            "--backend",
            &options.backend,
        ])
        .arg("--output")
        .arg(&output_path)
        .stdin(Stdio::null())
        .stdout(File::create(dir.join("stdout.log"))?)
        .stderr(File::create(dir.join("stderr.log"))?);
    if options.allow_dirty {
        command.arg("--allow-dirty");
    }
    if phase == "warmup" {
        // Warmup observations never enter the measurement telemetry stream.
        command.env_remove(&loaded.suite.telemetry.endpoint_env);
    }
    let mut attempt = Attempt {
        phase,
        index,
        process_id: None,
        result_ref: format!("{relative}/result.json"),
        stdout_ref: format!("{relative}/stdout.log"),
        stderr_ref: format!("{relative}/stderr.log"),
        exit_code: None,
        error: None,
        result: None,
    };
    match command.spawn() {
        Err(error) => attempt.error = Some(format!("spawn failed: {error}")),
        Ok(mut child) => {
            attempt.process_id = Some(child.id());
            let started = Instant::now();
            loop {
                if let Some(status) = child.try_wait()? {
                    attempt.exit_code = status.code();
                    if !status.success() {
                        attempt.error = Some(format!("child exited with {status}"));
                    }
                    break;
                }
                if started.elapsed() >= options.timeout {
                    let _ = child.kill();
                    let status = child.wait()?;
                    attempt.exit_code = status.code();
                    attempt.error = Some("child timed out".to_owned());
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
    match fs::read(&output_path)
        .map_err(|e| e.to_string())
        .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
    {
        Ok(result) => attempt.result = Some(result),
        Err(error) => {
            if attempt.error.is_none() {
                attempt.error = Some(format!("missing or invalid result: {error}"));
            }
        }
    }
    Ok(attempt)
}

fn contract_hash(loaded: &LoadedSuite) -> Result<String, Box<dyn Error>> {
    let mut hasher = Sha256::new();
    let mut files = vec![
        loaded.suite_bytes.clone(),
        fs::read(&loaded.registry_path)?,
        fs::read(&loaded.result_schema_path)?,
    ];
    for path in &loaded.contract_paths {
        files.push(fs::read(path)?);
    }
    for bytes in files {
        hasher.update(u64::try_from(bytes.len())?.to_be_bytes());
        hasher.update(bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn executable_digest(path: &std::path::Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
}
