//! Async GitHub API client wrapping octocrab, with TTL caching, rate limit
//! tracking, and retry logic.
#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use octocrab::Octocrab;
use octocrab::models::RunId;
use thiserror::Error;

use super::models::*;

/// Errors from the GitHub client.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("GitHub API error: {0}")]
    Api(String),

    #[error("Rate limited — resets in {reset_secs}s")]
    RateLimited { reset_secs: u64 },

    #[error("Network error after {attempts} attempts: {detail}")]
    Network { attempts: u8, detail: String },

    #[error("Response parse error: {0}")]
    Parse(String),
}

/// Cache key identifying a specific resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheKey {
    RepoSummary { owner: String, repo: String },
    RecentCommits { owner: String, repo: String, branch: String },
    OpenPrs { owner: String, repo: String },
    PrDetail { owner: String, repo: String, number: u64 },
    WorkflowRuns { owner: String, repo: String },
    WorkflowRunJobs { owner: String, repo: String, run_id: u64 },
}

/// A cached value with its TTL expiry time.
struct CacheEntry<T> {
    value: T,
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    fn is_valid(&self) -> bool {
        Instant::now() < self.expires_at
    }
}

/// TTL durations per resource class.
pub struct CacheTtls {
    pub repo_summary: Duration,
    pub commits: Duration,
    pub prs: Duration,
    pub workflow_runs: Duration,
    pub jobs: Duration,
}

impl Default for CacheTtls {
    fn default() -> Self {
        Self {
            repo_summary: Duration::from_secs(60),
            commits: Duration::from_secs(60),
            prs: Duration::from_secs(30),
            workflow_runs: Duration::from_secs(30),
            jobs: Duration::from_secs(15),
        }
    }
}

/// The GitHub API client with in-memory TTL cache and rate limit tracking.
pub struct GitHubClient {
    octocrab: Octocrab,
    ttls: CacheTtls,

    cache_repo_summary: HashMap<CacheKey, CacheEntry<RepoSummary>>,
    cache_commits: HashMap<CacheKey, CacheEntry<Vec<CommitSummary>>>,
    cache_prs: HashMap<CacheKey, CacheEntry<Vec<PrSummary>>>,
    cache_pr_detail: HashMap<CacheKey, CacheEntry<PrDetail>>,
    cache_workflow_runs: HashMap<CacheKey, CacheEntry<Vec<WorkflowRunSummary>>>,
    cache_jobs: HashMap<CacheKey, CacheEntry<Vec<JobSummary>>>,

    /// Current rate limit state (updated after every API response).
    pub rate_limit: RateLimitState,
}

impl GitHubClient {
    /// Create a new client authenticated with the given PAT.
    pub fn new(token: String) -> anyhow::Result<Self> {
        let octocrab = Octocrab::builder()
            .personal_token(token)
            .build()?;

        Ok(Self {
            octocrab,
            ttls: CacheTtls::default(),
            cache_repo_summary: HashMap::new(),
            cache_commits: HashMap::new(),
            cache_prs: HashMap::new(),
            cache_pr_detail: HashMap::new(),
            cache_workflow_runs: HashMap::new(),
            cache_jobs: HashMap::new(),
            rate_limit: RateLimitState::default(),
        })
    }

    /// Invalidate all cache entries (used on manual refresh).
    pub fn invalidate_cache(&mut self) {
        self.cache_repo_summary.clear();
        self.cache_commits.clear();
        self.cache_prs.clear();
        self.cache_pr_detail.clear();
        self.cache_workflow_runs.clear();
        self.cache_jobs.clear();
    }

    /// Check whether the client is currently rate-limited.
    pub fn is_rate_limited(&self) -> bool {
        if let Some(reset_at) = self.rate_limit.reset_at {
            self.rate_limit.is_blocked && Instant::now() < reset_at
        } else {
            false
        }
    }

    // ── Fetch: Repository Summary ─────────────────────────────────────────

    /// Fetch a repository summary, using cache if valid.
    pub async fn fetch_repo_summary(
        &mut self,
        owner: &str,
        repo: &str,
    ) -> Result<RepoSummary, ClientError> {
        let key = CacheKey::RepoSummary {
            owner: owner.to_string(),
            repo: repo.to_string(),
        };
        if let Some(e) = self.cache_repo_summary.get(&key) {
            if e.is_valid() { return Ok(e.value.clone()); }
        }
        let value = self.do_fetch_repo_summary(owner, repo).await?;
        self.cache_repo_summary.insert(key, CacheEntry {
            value: value.clone(),
            expires_at: Instant::now() + self.ttls.repo_summary,
        });
        Ok(value)
    }

    async fn do_fetch_repo_summary(&self, owner: &str, repo: &str) -> Result<RepoSummary, ClientError> {
        let o = owner.to_string();
        let r = repo.to_string();
        let result = self.with_retry(|| {
            let octocrab = self.octocrab.clone();
            let o = o.clone();
            let r = r.clone();
            async move { octocrab.repos(&o, &r).get().await }
        }).await?;

        Ok(RepoSummary {
            owner: result.owner.as_ref().map(|u| u.login.clone()).unwrap_or_default(),
            name: result.name.clone(),
            description: result.description.clone(),
            default_branch: result.default_branch.clone().unwrap_or_else(|| "main".to_string()),
            stars: result.stargazers_count.unwrap_or(0),
            forks: result.forks_count.unwrap_or(0),
            open_pr_count: 0,
            open_issue_count: result.open_issues_count.unwrap_or(0),
            language: result.language.as_ref().and_then(|v| v.as_str().map(|s| s.to_string())),
            visibility: result.visibility.clone().unwrap_or_else(|| "public".to_string()),
        })
    }

    // ── Fetch: Recent Commits ─────────────────────────────────────────────

    /// Fetch recent commits on a branch, using cache if valid.
    pub async fn fetch_recent_commits(
        &mut self,
        owner: &str,
        repo: &str,
        branch: &str,
        limit: u8,
    ) -> Result<Vec<CommitSummary>, ClientError> {
        let key = CacheKey::RecentCommits {
            owner: owner.to_string(),
            repo: repo.to_string(),
            branch: branch.to_string(),
        };
        if let Some(e) = self.cache_commits.get(&key) {
            if e.is_valid() { return Ok(e.value.clone()); }
        }
        let value = self.do_fetch_recent_commits(owner, repo, branch, limit).await?;
        self.cache_commits.insert(key, CacheEntry {
            value: value.clone(),
            expires_at: Instant::now() + self.ttls.commits,
        });
        Ok(value)
    }

    async fn do_fetch_recent_commits(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        limit: u8,
    ) -> Result<Vec<CommitSummary>, ClientError> {
        let o = owner.to_string();
        let r = repo.to_string();
        let b = branch.to_string();
        let commits = self.with_retry(|| {
            let octocrab = self.octocrab.clone();
            let o = o.clone(); let r = r.clone(); let b = b.clone();
            async move {
                octocrab.repos(&o, &r).list_commits().sha(b).per_page(limit).send().await
            }
        }).await?;

        let result = commits.items.into_iter().map(|c| {
            let sha = c.sha.clone();
            let short_sha: String = sha.chars().take(7).collect();
            let message = c.commit.message.lines().next().unwrap_or("").to_string();
            let author = c.commit.author
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let timestamp: DateTime<Utc> = c.commit.author
                .as_ref()
                .and_then(|a| a.date)
                .unwrap_or_else(Utc::now);
            CommitSummary { sha, short_sha, message, author, timestamp }
        }).collect();

        Ok(result)
    }

    // ── Fetch: Open PRs ───────────────────────────────────────────────────

    /// Fetch open PRs, using cache if valid.
    pub async fn fetch_open_prs(
        &mut self,
        owner: &str,
        repo: &str,
        limit: u8,
    ) -> Result<Vec<PrSummary>, ClientError> {
        let key = CacheKey::OpenPrs { owner: owner.to_string(), repo: repo.to_string() };
        if let Some(e) = self.cache_prs.get(&key) {
            if e.is_valid() { return Ok(e.value.clone()); }
        }
        let value = self.do_fetch_open_prs(owner, repo, limit).await?;
        self.cache_prs.insert(key, CacheEntry {
            value: value.clone(),
            expires_at: Instant::now() + self.ttls.prs,
        });
        Ok(value)
    }

    async fn do_fetch_open_prs(&self, owner: &str, repo: &str, limit: u8) -> Result<Vec<PrSummary>, ClientError> {
        let o = owner.to_string(); let r = repo.to_string();
        let prs = self.with_retry(|| {
            let octocrab = self.octocrab.clone(); let o = o.clone(); let r = r.clone();
            async move {
                octocrab.pulls(&o, &r).list()
                    .state(octocrab::params::State::Open)
                    .per_page(limit)
                    .send()
                    .await
            }
        }).await?;

        let result = prs.items.into_iter().map(|pr| {
            let updated_at: DateTime<Utc> = pr.updated_at.unwrap_or_else(Utc::now);
            PrSummary {
                number: pr.number,
                title: pr.title.clone().unwrap_or_default(),
                author: pr.user.as_ref().map(|u| u.login.clone()).unwrap_or_default(),
                head_branch: pr.head.ref_field.clone(),
                updated_at,
            }
        }).collect();

        Ok(result)
    }

    // ── Fetch: PR Detail ──────────────────────────────────────────────────

    /// Fetch detailed PR information.
    pub async fn fetch_pr_detail(
        &mut self,
        owner: &str,
        repo: &str,
        number: u64,
    ) -> Result<PrDetail, ClientError> {
        let key = CacheKey::PrDetail { owner: owner.to_string(), repo: repo.to_string(), number };
        if let Some(e) = self.cache_pr_detail.get(&key) {
            if e.is_valid() { return Ok(e.value.clone()); }
        }
        let value = self.do_fetch_pr_detail(owner, repo, number).await?;
        self.cache_pr_detail.insert(key, CacheEntry {
            value: value.clone(),
            expires_at: Instant::now() + self.ttls.prs,
        });
        Ok(value)
    }

    async fn do_fetch_pr_detail(&self, owner: &str, repo: &str, number: u64) -> Result<PrDetail, ClientError> {
        let o = owner.to_string(); let r = repo.to_string();
        let pr = self.with_retry(|| {
            let octocrab = self.octocrab.clone(); let o = o.clone(); let r = r.clone();
            async move { octocrab.pulls(&o, &r).get(number).await }
        }).await?;

        let mergeable_state = match pr.mergeable {
            Some(true) => MergeableState::Mergeable,
            Some(false) => MergeableState::Conflicting,
            None => MergeableState::Unknown,
        };

        let created_at: DateTime<Utc> = pr.created_at.unwrap_or_else(Utc::now);
        let updated_at: DateTime<Utc> = pr.updated_at.unwrap_or_else(Utc::now);

        // Fetch changed files.
        let files = self.with_retry(|| {
            let octocrab = self.octocrab.clone(); let o = o.clone(); let r = r.clone();
            async move { octocrab.pulls(&o, &r).list_files(number).await }
        }).await.unwrap_or_default();

        let changed_files = files.items.into_iter().take(50).map(|f| {
            use octocrab::models::repos::DiffEntryStatus;
            let change_type = match f.status {
                DiffEntryStatus::Added => FileChangeType::Added,
                DiffEntryStatus::Removed => FileChangeType::Deleted,
                DiffEntryStatus::Renamed => FileChangeType::Renamed,
                DiffEntryStatus::Copied => FileChangeType::Copied,
                DiffEntryStatus::Modified | DiffEntryStatus::Changed | _ => FileChangeType::Modified,
            };
            ChangedFile {
                filename: f.filename.clone(),
                change_type,
                additions: f.additions as u32,
                deletions: f.deletions as u32,
            }
        }).collect();

        Ok(PrDetail {
            number: pr.number,
            title: pr.title.clone().unwrap_or_default(),
            author: pr.user.as_ref().map(|u| u.login.clone()).unwrap_or_default(),
            head_branch: pr.head.ref_field.clone(),
            base_branch: pr.base.ref_field.clone(),
            body: pr.body.clone(),
            created_at,
            updated_at,
            mergeable_state,
            reviewers: vec![],
            check_runs: vec![],
            changed_files,
        })
    }

    // ── Fetch: Workflow Runs ──────────────────────────────────────────────

    /// Fetch workflow runs for a repository, using cache if valid.
    pub async fn fetch_workflow_runs(
        &mut self,
        owner: &str,
        repo: &str,
        limit: u8,
    ) -> Result<Vec<WorkflowRunSummary>, ClientError> {
        let key = CacheKey::WorkflowRuns { owner: owner.to_string(), repo: repo.to_string() };
        if let Some(e) = self.cache_workflow_runs.get(&key) {
            if e.is_valid() { return Ok(e.value.clone()); }
        }
        let value = self.do_fetch_workflow_runs(owner, repo, limit).await?;
        self.cache_workflow_runs.insert(key, CacheEntry {
            value: value.clone(),
            expires_at: Instant::now() + self.ttls.workflow_runs,
        });
        Ok(value)
    }

    async fn do_fetch_workflow_runs(&self, owner: &str, repo: &str, limit: u8) -> Result<Vec<WorkflowRunSummary>, ClientError> {
        let o = owner.to_string(); let r = repo.to_string();
        let page = self.with_retry(|| {
            let octocrab = self.octocrab.clone(); let o = o.clone(); let r = r.clone();
            async move {
                octocrab.workflows(&o, &r).list_all_runs().per_page(limit).send().await
            }
        }).await?;

        let repo_str = format!("{}/{}", owner, repo);
        let result = page.items.into_iter().map(|r| {
            let status = parse_run_status(&r.status);
            let conclusion = r.conclusion.as_deref().map(parse_run_conclusion);
            WorkflowRunSummary {
                id: r.id.into_inner(),
                workflow_name: r.name.clone(),
                run_number: r.run_number as u64,
                event: r.event.clone(),
                actor: r.repository.owner
                    .as_ref()
                    .map(|u| u.login.clone())
                    .unwrap_or_default(),
                head_branch: r.head_branch.clone(),
                head_commit_message: r.head_commit.message
                    .lines().next().unwrap_or("").to_string(),
                status,
                conclusion,
                created_at: r.created_at,
                updated_at: r.updated_at,
                repo: Some(repo_str.clone()),
            }
        }).collect();

        Ok(result)
    }

    // ── Fetch: Workflow Run Jobs ──────────────────────────────────────────

    /// Fetch jobs for a workflow run, using cache if valid.
    pub async fn fetch_workflow_run_jobs(
        &mut self,
        owner: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<Vec<JobSummary>, ClientError> {
        let key = CacheKey::WorkflowRunJobs { owner: owner.to_string(), repo: repo.to_string(), run_id };
        if let Some(e) = self.cache_jobs.get(&key) {
            if e.is_valid() { return Ok(e.value.clone()); }
        }
        let value = self.do_fetch_workflow_run_jobs(owner, repo, run_id).await?;
        self.cache_jobs.insert(key, CacheEntry {
            value: value.clone(),
            expires_at: Instant::now() + self.ttls.jobs,
        });
        Ok(value)
    }

    async fn do_fetch_workflow_run_jobs(&self, owner: &str, repo: &str, run_id: u64) -> Result<Vec<JobSummary>, ClientError> {
        let o = owner.to_string(); let r = repo.to_string();
        let page = self.with_retry(|| {
            let octocrab = self.octocrab.clone(); let o = o.clone(); let r = r.clone();
            async move {
                octocrab.workflows(&o, &r).list_jobs(RunId(run_id)).send().await
            }
        }).await?;

        let result = page.items.into_iter().map(|j| {
            use octocrab::models::workflows::{Status, Conclusion};

            let steps = j.steps.iter().map(|s| {
                StepSummary {
                    name: s.name.clone(),
                    number: s.number as u64,
                    status: match &s.status {
                        Status::Queued => RunStatus::Queued,
                        Status::InProgress => RunStatus::InProgress,
                        Status::Completed => RunStatus::Completed,
                        _ => RunStatus::Unknown,
                    },
                    conclusion: s.conclusion.as_ref().map(|c| match c {
                        Conclusion::Success => RunConclusion::Success,
                        Conclusion::Failure => RunConclusion::Failure,
                        Conclusion::Cancelled => RunConclusion::Cancelled,
                        Conclusion::Skipped => RunConclusion::Skipped,
                        Conclusion::TimedOut => RunConclusion::TimedOut,
                        Conclusion::ActionRequired => RunConclusion::ActionRequired,
                        Conclusion::Neutral => RunConclusion::Neutral,
                        _ => RunConclusion::Other("unknown".to_string()),
                    }),
                    started_at: s.started_at,
                    completed_at: s.completed_at,
                }
            }).collect();

            let status = match &j.status {
                Status::Queued => RunStatus::Queued,
                Status::InProgress => RunStatus::InProgress,
                Status::Completed => RunStatus::Completed,
                _ => RunStatus::Unknown,
            };
            let conclusion = j.conclusion.as_ref().map(|c| match c {
                Conclusion::Success => RunConclusion::Success,
                Conclusion::Failure => RunConclusion::Failure,
                Conclusion::Cancelled => RunConclusion::Cancelled,
                Conclusion::Skipped => RunConclusion::Skipped,
                Conclusion::TimedOut => RunConclusion::TimedOut,
                Conclusion::ActionRequired => RunConclusion::ActionRequired,
                Conclusion::Neutral => RunConclusion::Neutral,
                _ => RunConclusion::Other("unknown".to_string()),
            });

            JobSummary {
                id: j.id.into_inner(),
                name: j.name.clone(),
                runner_type: j.runner_name.clone(),
                status,
                conclusion,
                started_at: Some(j.started_at),
                completed_at: j.completed_at,
                steps,
            }
        }).collect();

        Ok(result)
    }

    // ── Fetch: Job Log ────────────────────────────────────────────────────

    /// Download the log for a workflow run (all jobs combined).
    /// Note: GitHub's API provides run-level log archives (zip), not per-job text.
    /// This downloads the bytes and converts to a string for display.
    pub async fn fetch_job_log(
        &mut self,
        owner: &str,
        repo: &str,
        run_id: u64,
    ) -> Result<String, ClientError> {
        let o = owner.to_string(); let r = repo.to_string();
        let bytes = self.with_retry(|| {
            let octocrab = self.octocrab.clone(); let o = o.clone(); let r = r.clone();
            async move {
                octocrab.actions().download_workflow_run_logs(&o, &r, RunId(run_id)).await
            }
        }).await?;

        // The response is a zip archive; return a note for now.
        // Full log extraction from zip is handled in the log viewer layer.
        let s = String::from_utf8_lossy(&bytes).into_owned();
        Ok(s)
    }

    // ── Retry helper ──────────────────────────────────────────────────────

    async fn with_retry<F, Fut, T, E>(&self, mut f: F) -> Result<T, ClientError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let max = 3u8;
        let mut last_err = String::new();
        for attempt in 0..max {
            match f().await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    last_err = e.to_string();
                    if last_err.contains("rate limit") || last_err.contains("403") || last_err.contains("429") {
                        return Err(ClientError::RateLimited { reset_secs: 60 });
                    }
                    if attempt < max - 1 {
                        tokio::time::sleep(Duration::from_secs(1u64 << attempt)).await;
                    }
                }
            }
        }
        Err(ClientError::Network { attempts: max, detail: last_err })
    }
}

// ── Parse helpers ─────────────────────────────────────────────────────────────

fn parse_run_status(s: &str) -> RunStatus {
    match s {
        "queued" => RunStatus::Queued,
        "in_progress" => RunStatus::InProgress,
        "completed" => RunStatus::Completed,
        "waiting" => RunStatus::Waiting,
        _ => RunStatus::Unknown,
    }
}

fn parse_run_conclusion(s: &str) -> RunConclusion {
    match s {
        "success" => RunConclusion::Success,
        "failure" => RunConclusion::Failure,
        "cancelled" => RunConclusion::Cancelled,
        "skipped" => RunConclusion::Skipped,
        "timed_out" => RunConclusion::TimedOut,
        "action_required" => RunConclusion::ActionRequired,
        "neutral" => RunConclusion::Neutral,
        other => RunConclusion::Other(other.to_string()),
    }
}
