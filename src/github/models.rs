//! Data models returned by the GitHub API client.
#![allow(dead_code)]

use std::time::Instant;

use chrono::{DateTime, Utc};

// ── Repository ────────────────────────────────────────────────────────────────

/// Summary of a repository shown on the dashboard.
#[derive(Debug, Clone)]
pub struct RepoSummary {
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub stars: u32,
    pub forks: u32,
    pub open_pr_count: u32,
    pub open_issue_count: u32,
    pub language: Option<String>,
    pub visibility: String,
}

// ── Commits ───────────────────────────────────────────────────────────────────

/// A single commit summary.
#[derive(Debug, Clone)]
pub struct CommitSummary {
    pub sha: String,
    pub short_sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: DateTime<Utc>,
}

// ── Pull Requests ─────────────────────────────────────────────────────────────

/// Lightweight PR summary for list views.
#[derive(Debug, Clone)]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_branch: String,
    pub updated_at: DateTime<Utc>,
}

/// Detailed PR data for the PR view.
#[derive(Debug, Clone)]
pub struct PrDetail {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub head_branch: String,
    pub base_branch: String,
    pub body: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub mergeable_state: MergeableState,
    pub reviewers: Vec<ReviewerStatus>,
    pub check_runs: Vec<CheckRun>,
    pub changed_files: Vec<ChangedFile>,
}

/// Mergeable state of a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeableState {
    Mergeable,
    Conflicting,
    Unknown,
}

/// A reviewer and their review state.
#[derive(Debug, Clone)]
pub struct ReviewerStatus {
    pub login: String,
    pub state: ReviewState,
}

/// Review state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    Commented,
    Pending,
    Dismissed,
}

/// A check run on a PR.
#[derive(Debug, Clone)]
pub struct CheckRun {
    pub name: String,
    pub status: RunStatus,
    pub conclusion: Option<RunConclusion>,
}

/// A file changed in a PR.
#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub filename: String,
    pub change_type: FileChangeType,
    pub additions: u32,
    pub deletions: u32,
}

/// Type of file change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Other,
}

// ── Actions ───────────────────────────────────────────────────────────────────

/// Summary of a workflow run for list views.
#[derive(Debug, Clone)]
pub struct WorkflowRunSummary {
    pub id: u64,
    pub workflow_name: String,
    pub run_number: u64,
    pub event: String,
    pub actor: String,
    pub head_branch: String,
    pub head_commit_message: String,
    pub status: RunStatus,
    pub conclusion: Option<RunConclusion>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Owner/repo string, populated in cross-repo views.
    pub repo: Option<String>,
}

/// Status of a run, job, or step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    InProgress,
    Completed,
    Waiting,
    Unknown,
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunStatus::Queued => write!(f, "queued"),
            RunStatus::InProgress => write!(f, "in_progress"),
            RunStatus::Completed => write!(f, "completed"),
            RunStatus::Waiting => write!(f, "waiting"),
            RunStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Conclusion of a completed run, job, step, or check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunConclusion {
    Success,
    Failure,
    Cancelled,
    Skipped,
    TimedOut,
    ActionRequired,
    Neutral,
    Other(String),
}

impl std::fmt::Display for RunConclusion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunConclusion::Success => write!(f, "success"),
            RunConclusion::Failure => write!(f, "failure"),
            RunConclusion::Cancelled => write!(f, "cancelled"),
            RunConclusion::Skipped => write!(f, "skipped"),
            RunConclusion::TimedOut => write!(f, "timed_out"),
            RunConclusion::ActionRequired => write!(f, "action_required"),
            RunConclusion::Neutral => write!(f, "neutral"),
            RunConclusion::Other(s) => write!(f, "{s}"),
        }
    }
}

/// A job within a workflow run.
#[derive(Debug, Clone)]
pub struct JobSummary {
    pub id: u64,
    pub name: String,
    pub runner_type: Option<String>,
    pub status: RunStatus,
    pub conclusion: Option<RunConclusion>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub steps: Vec<StepSummary>,
}

/// A step within a job.
#[derive(Debug, Clone)]
pub struct StepSummary {
    pub name: String,
    pub number: u64,
    pub status: RunStatus,
    pub conclusion: Option<RunConclusion>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

// ── Rate Limiting ─────────────────────────────────────────────────────────────

/// Current GitHub API rate limit state.
#[derive(Debug, Clone, Default)]
pub struct RateLimitState {
    /// Remaining requests in the current window.
    pub remaining: Option<u32>,
    /// Total limit for the window.
    pub limit: Option<u32>,
    /// When the window resets (Unix timestamp).
    pub reset_at: Option<Instant>,
    /// Whether we are currently blocked due to rate limiting.
    pub is_blocked: bool,
}
