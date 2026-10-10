//! Bounded cache histories on the existing Turmoil substrate.
//!
//! This seam models an immutable version-addressed object, a disposable cache,
//! and a serialized authority. It does not prove a production provider adapter.
use crate::{sha256, take_mutex};
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    Correct,
    SkipPublicationFence,
    SkipUseFence,
    TrustCacheBytes,
    RepairWrongVersion,
}

/// Explicit interleaving points; finish operations must revalidate authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum CacheAction {
    Authorize,
    Populate,
    Read,
    BeginStream,
    FinishStream,
    Delete,
    Overwrite,
    AdvanceGeneration,
    ExpireLease,
    ReleaseLease,
    LoseCache,
    CorruptCache,
    RestoreOldRoot,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CacheEvent {
    pub step: usize,
    pub virtual_time_ms: u128,
    pub action: CacheAction,
    pub generation: u64,
    pub root: u64,
    pub lease_active: bool,
    pub result: String,
    pub returned_sha256: Option<String>,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CacheHistory {
    pub name: String,
    pub events: Vec<CacheEvent>,
    pub failures: Vec<String>,
    pub minimized_actions: Vec<CacheAction>,
    pub minimized_is_one_minimal: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CacheReport {
    pub contract_version: u32,
    pub framework: String,
    pub seed: u64,
    pub source_revision: String,
    pub mode: CacheMode,
    pub lockfile_sha256: String,
    pub profile_sha256: String,
    pub histories: Vec<CacheHistory>,
    pub trace_sha256: String,
}
impl CacheReport {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.histories.iter().all(|h| h.failures.is_empty())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Token {
    generation: u64,
    root: u64,
    lease_epoch: u64,
}

struct Authority {
    token: Token,
    present: bool,
    lease_active: bool,
    // Immutable object content is retained even when no longer authorized.
    objects: BTreeMap<u64, Vec<u8>>,
}
impl Authority {
    fn new() -> Self {
        Self {
            token: Token {
                generation: 1,
                root: 1,
                lease_epoch: 1,
            },
            present: true,
            lease_active: true,
            objects: BTreeMap::from([
                (1, b"version-one-exact".to_vec()),
                (2, b"version-two-exact".to_vec()),
            ]),
        }
    }
    fn admits(&self, token: Token) -> bool {
        self.present && self.lease_active && self.token == token
    }
}

struct Subject {
    mode: CacheMode,
    authorized: Option<Token>,
    pending: Option<(Token, Vec<u8>)>,
    streaming: Option<(Token, Vec<u8>)>,
    cache: BTreeMap<u64, Vec<u8>>,
}

#[derive(Default)]
struct Outcome {
    result: &'static str,
    used: Option<Token>,
    returned: Option<Vec<u8>>,
    published: Option<Token>,
}

impl Subject {
    fn new(mode: CacheMode) -> Self {
        Self {
            mode,
            authorized: None,
            pending: None,
            streaming: None,
            cache: BTreeMap::new(),
        }
    }
    fn use_allowed(&self, authority: &Authority, token: Token) -> bool {
        self.mode == CacheMode::SkipUseFence || authority.admits(token)
    }
    fn execute(&mut self, authority: &Authority, action: CacheAction) -> Outcome {
        let mut out = Outcome {
            result: "applied",
            ..Outcome::default()
        };
        match action {
            CacheAction::Authorize => {
                if authority.admits(authority.token) {
                    self.authorized = Some(authority.token);
                    self.pending = Some((
                        authority.token,
                        authority.objects[&authority.token.root].clone(),
                    ));
                    out.result = "authorized";
                } else {
                    out.result = "refused";
                }
            }
            CacheAction::Populate => {
                out.result = "refused";
                if let Some((token, bytes)) = self.pending.take() {
                    if self.mode == CacheMode::SkipPublicationFence || authority.admits(token) {
                        self.cache.insert(token.root, bytes);
                        out.published = Some(token);
                        out.result = "published";
                    }
                }
            }
            CacheAction::Read | CacheAction::BeginStream => {
                out.result = "refused";
                if let Some(token) = self.authorized {
                    if self.use_allowed(authority, token) {
                        let expected = &authority.objects[&token.root];
                        let cached = self.cache.get(&token.root);
                        let bytes = if cached.is_some_and(|v| v == expected)
                            || (cached.is_some() && self.mode == CacheMode::TrustCacheBytes)
                        {
                            out.result = "cache_hit";
                            cached.cloned().expect("checked cache presence")
                        } else {
                            out.result = "backend_repair";
                            let root = if self.mode == CacheMode::RepairWrongVersion {
                                2
                            } else {
                                token.root
                            };
                            let bytes = authority.objects[&root].clone();
                            self.cache.insert(token.root, bytes.clone());
                            bytes
                        };
                        if action == CacheAction::BeginStream {
                            self.streaming = Some((token, bytes));
                            out.result = "stream_started";
                        } else {
                            out.used = Some(token);
                            out.returned = Some(bytes);
                        }
                    }
                }
            }
            CacheAction::FinishStream => {
                out.result = "refused";
                if let Some((token, bytes)) = self.streaming.take() {
                    if self.use_allowed(authority, token) {
                        out.result = "stream_returned";
                        out.used = Some(token);
                        out.returned = Some(bytes);
                    }
                }
            }
            CacheAction::LoseCache => self.cache.clear(),
            CacheAction::CorruptCache => {
                self.cache.insert(1, b"wrong-cache-bytes".to_vec());
            }
            CacheAction::RestoreOldRoot => {
                self.cache.insert(1, authority.objects[&1].clone());
            }
            _ => {}
        }
        out
    }
}

// The grader observes authoritative transitions and returned bytes independently
// of subject admission helpers. Poison switches never alter this oracle.
fn step(
    authority: &mut Authority,
    subject: &mut Subject,
    action: CacheAction,
    index: usize,
    time: u128,
) -> CacheEvent {
    match action {
        CacheAction::Delete => {
            authority.present = false;
            authority.token.root += 1;
        }
        CacheAction::Overwrite => authority.token.root = 2,
        CacheAction::AdvanceGeneration => authority.token.generation += 1,
        CacheAction::ExpireLease | CacheAction::ReleaseLease => {
            authority.lease_active = false;
            authority.token.lease_epoch += 1;
        }
        _ => {}
    }
    let out = subject.execute(authority, action);
    let mut failure = None;
    if let Some(token) = out.published.or(out.used) {
        if !authority.present
            || !authority.lease_active
            || token.generation != authority.token.generation
            || token.root != authority.token.root
            || token.lease_epoch != authority.token.lease_epoch
        {
            failure = Some("stale authority became published or returned bytes".to_owned());
        }
    }
    if let Some(bytes) = &out.returned {
        if authority.objects.get(&authority.token.root) != Some(bytes) {
            failure = Some("wrong bytes returned".to_owned());
        }
    }
    CacheEvent {
        step: index,
        virtual_time_ms: time,
        action,
        generation: authority.token.generation,
        root: authority.token.root,
        lease_active: authority.lease_active,
        result: out.result.to_owned(),
        returned_sha256: out.returned.as_deref().map(sha256),
        failure,
    }
}

fn replay_fails(mode: CacheMode, actions: &[CacheAction]) -> bool {
    let mut authority = Authority::new();
    let mut subject = Subject::new(mode);
    actions.iter().enumerate().any(|(i, action)| {
        step(&mut authority, &mut subject, *action, i, 0)
            .failure
            .is_some()
    })
}

fn minimize(mode: CacheMode, actions: &[CacheAction]) -> Vec<CacheAction> {
    if !replay_fails(mode, actions) {
        return Vec::new();
    }
    let mut minimal = actions.to_vec();
    let mut i = 0;
    while i < minimal.len() {
        let mut candidate = minimal.clone();
        candidate.remove(i);
        if replay_fails(mode, &candidate) {
            minimal = candidate;
            i = 0;
        } else {
            i += 1;
        }
    }
    minimal
}

fn histories() -> Vec<(&'static str, Vec<CacheAction>)> {
    use CacheAction::{
        AdvanceGeneration, Authorize, BeginStream, CorruptCache, Delete, ExpireLease, FinishStream,
        LoseCache, Overwrite, Populate, Read, ReleaseLease, RestoreOldRoot,
    };
    vec![
        ("delete_then_read", vec![Authorize, Populate, Delete, Read]),
        (
            "overwrite_during_stream",
            vec![Authorize, Populate, BeginStream, Overwrite, FinishStream],
        ),
        (
            "delayed_population",
            vec![Authorize, Overwrite, Populate, Read],
        ),
        (
            "expired_lease_cache_hit",
            vec![Authorize, Populate, ExpireLease, Read],
        ),
        (
            "released_lease_cache_hit",
            vec![Authorize, Populate, ReleaseLease, Read],
        ),
        (
            "cache_loss_and_corruption",
            vec![
                Authorize,
                Populate,
                Read,
                LoseCache,
                Read,
                CorruptCache,
                Read,
            ],
        ),
        (
            "stale_generation_population",
            vec![Authorize, AdvanceGeneration, Populate, Read],
        ),
        (
            "stale_root_resurrection",
            vec![
                Authorize,
                Populate,
                Overwrite,
                LoseCache,
                RestoreOldRoot,
                Read,
            ],
        ),
    ]
}

/// Run fixed bounded interleavings with seed-controlled virtual-time delays.
/// No wall clock, provider connection, or external credentials are used.
///
/// # Errors
/// Returns simulator or report serialization failures.
pub fn run_cache_resurrection(
    seed: u64,
    source_revision: &str,
    mode: CacheMode,
) -> Result<CacheReport, String> {
    let scenarios = histories();
    let profile_sha256 = sha256(&serde_json::to_vec(&scenarios).map_err(|e| e.to_string())?);
    let results = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::clone(&results);
    let mut builder = turmoil::Builder::new();
    builder
        .rng_seed(seed)
        .tick_duration(Duration::from_millis(1))
        .simulation_duration(Duration::from_secs(10))
        .enable_random_order();
    let mut sim = builder.build();
    sim.client("cache-history", async move {
        for (name, actions) in scenarios {
            let mut authority = Authority::new();
            let mut subject = Subject::new(mode);
            let mut events = Vec::new();
            for (i, action) in actions.iter().enumerate() {
                tokio::time::sleep(Duration::from_millis(1 + seed.wrapping_add(i as u64) % 5))
                    .await;
                events.push(step(
                    &mut authority,
                    &mut subject,
                    *action,
                    i,
                    turmoil::sim_elapsed().unwrap_or_default().as_millis(),
                ));
            }
            let failures = events.iter().filter_map(|e| e.failure.clone()).collect();
            let minimized_actions = minimize(mode, &actions);
            let minimized_is_one_minimal = !minimized_actions.is_empty()
                && (0..minimized_actions.len()).all(|i| {
                    let mut reduced = minimized_actions.clone();
                    reduced.remove(i);
                    !replay_fails(mode, &reduced)
                });
            output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(CacheHistory {
                    name: name.to_owned(),
                    events,
                    failures,
                    minimized_actions,
                    minimized_is_one_minimal,
                });
        }
        Ok(())
    });
    sim.run().map_err(|e| e.to_string())?;
    let mut report = CacheReport {
        contract_version: 1,
        framework: "turmoil-0.7.2".to_owned(),
        seed,
        source_revision: source_revision.to_owned(),
        mode,
        lockfile_sha256: sha256(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../Cargo.lock"
        ))),
        profile_sha256,
        histories: take_mutex(results)?,
        trace_sha256: String::new(),
    };
    report.trace_sha256 = sha256(&serde_json::to_vec(&report).map_err(|e| e.to_string())?);
    Ok(report)
}
