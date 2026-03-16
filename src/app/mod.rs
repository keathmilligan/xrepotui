//! Core application state, event types, event loop, and poll scheduler.
#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{Event, EventStream, KeyCode, KeyModifiers};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::config::Config;
use crate::github::models::*;
use crate::github::GitHubClient;
use crate::navigation::{LogViewerState, PrViewState, View, ViewStack};

// ── Per-repo data ─────────────────────────────────────────────────────────────

/// All fetched data for a single repository.
#[derive(Debug, Clone, Default)]
pub struct RepoData {
    pub key: String, // "owner/repo"
    pub summary: Option<RepoSummary>,
    pub commits: Vec<CommitSummary>,
    pub open_prs: Vec<PrSummary>,
    pub workflow_runs: Vec<WorkflowRunSummary>,
}

/// Per-repo fetch state.
#[derive(Debug, Clone, Default)]
pub struct RepoFetchState {
    pub loading: bool,
    pub error: Option<String>,
    pub last_refreshed: Option<Instant>,
}

// ── AppEvent ──────────────────────────────────────────────────────────────────

/// Events processed by the main event loop.
pub enum AppEvent {
    /// Fetched data for a repository.
    DataFetched(Box<RepoData>),
    /// Fetch error for a repository.
    FetchError { repo: String, message: String },
    /// Rate limit hit — blocked until reset.
    RateLimited { reset_at: Instant },
    /// A terminal (keyboard/resize) event.
    TerminalEvent(Event),
    /// Periodic tick for time-based UI updates (elapsed counters, etc.).
    Tick,
    /// Job log fetched.
    LogFetched {
        job_id: u64,
        content: String,
        is_complete: bool,
    },
    /// Fetching log for a job started (for loading indicator).
    LogLoading { job_id: u64 },
}

// ── AppState ──────────────────────────────────────────────────────────────────

/// Central application state.
pub struct AppState {
    /// Navigation view stack.
    pub views: ViewStack,
    /// Fetched data keyed by "owner/repo".
    pub repo_data: HashMap<String, RepoData>,
    /// Fetch state keyed by "owner/repo".
    pub fetch_state: HashMap<String, RepoFetchState>,
    /// Current GitHub API rate limit state.
    pub rate_limit: RateLimitState,
    /// Whether the app is running.
    pub running: bool,
    /// Whether a quit confirmation is pending.
    pub pending_quit: bool,
    /// Last time all repos were refreshed.
    pub last_refreshed: Option<Instant>,
    /// Current PR detail, if the PR view is open.
    pub pr_detail: Option<PrDetail>,
    /// Log content keyed by job_id.
    pub log_content: HashMap<u64, Vec<String>>,
    /// Whether a log is currently loading.
    pub log_loading: bool,
    /// Whether the job for the currently open log is complete.
    pub log_job_complete: bool,
}

impl AppState {
    /// Create initial state for all configured repos.
    pub fn new(repos: &[String]) -> Self {
        let mut repo_data = HashMap::new();
        let mut fetch_state = HashMap::new();
        for r in repos {
            repo_data.insert(
                r.clone(),
                RepoData {
                    key: r.clone(),
                    ..Default::default()
                },
            );
            fetch_state.insert(
                r.clone(),
                RepoFetchState {
                    loading: true,
                    ..Default::default()
                },
            );
        }
        Self {
            views: ViewStack::new(),
            repo_data,
            fetch_state,
            rate_limit: RateLimitState::default(),
            running: true,
            pending_quit: false,
            last_refreshed: None,
            pr_detail: None,
            log_content: HashMap::new(),
            log_loading: false,
            log_job_complete: false,
        }
    }

    /// Handle an AppEvent and return the mutated state.
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::DataFetched(data) => {
                let data = *data;
                let key = data.key.clone();
                self.repo_data.insert(key.clone(), data);
                if let Some(fs) = self.fetch_state.get_mut(&key) {
                    fs.loading = false;
                    fs.error = None;
                    fs.last_refreshed = Some(Instant::now());
                }
                self.last_refreshed = Some(Instant::now());
            }

            AppEvent::FetchError { repo, message } => {
                if let Some(fs) = self.fetch_state.get_mut(&repo) {
                    fs.loading = false;
                    fs.error = Some(message);
                }
            }

            AppEvent::RateLimited { reset_at } => {
                self.rate_limit.is_blocked = true;
                self.rate_limit.reset_at = Some(reset_at);
            }

            AppEvent::TerminalEvent(event) => {
                self.handle_terminal_event(event);
            }

            AppEvent::Tick => {
                // Unblock rate limit if reset time has passed.
                if let Some(reset_at) = self.rate_limit.reset_at {
                    if Instant::now() >= reset_at {
                        self.rate_limit.is_blocked = false;
                    }
                }
            }

            AppEvent::LogFetched {
                job_id,
                content,
                is_complete,
            } => {
                self.log_loading = false;
                self.log_job_complete = is_complete;

                let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

                // If the log viewer is open and was at the bottom, keep it there.
                let was_at_bottom = if let View::LogViewer(ref s) = *self.views.current() {
                    s.at_bottom
                } else {
                    false
                };

                let total = lines.len();
                self.log_content.insert(job_id, lines);

                // Update step starts and total_lines in the log viewer state.
                if let View::LogViewer(ref mut s) = *self.views.current_mut() {
                    s.total_lines = total;
                    s.step_starts = parse_step_boundaries(
                        self.log_content
                            .get(&job_id)
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]),
                    );
                    if was_at_bottom {
                        s.scroll = total.saturating_sub(1);
                        s.at_bottom = true;
                    }
                }
            }

            AppEvent::LogLoading { .. } => {
                self.log_loading = true;
            }
        }
    }

    /// Handle a terminal (keyboard/resize) event.
    fn handle_terminal_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => {
                // Quit confirmation mode
                if self.pending_quit {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            self.running = false;
                        }
                        _ => {
                            self.pending_quit = false;
                        }
                    }
                    return;
                }

                // Global keys
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.running = false;
                        return;
                    }
                    _ => {}
                }

                // Dispatch to current view
                let is_root = self.views.is_root();
                match self.views.current_mut() {
                    View::Dashboard(ref mut state) => {
                        handle_dashboard_key(
                            state,
                            key,
                            is_root,
                            &mut self.running,
                            &mut self.pending_quit,
                        );
                    }
                    View::Repo(_) => {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            self.views.pop();
                        }
                    }
                    View::Pr(ref mut state) => {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            self.views.pop();
                        } else {
                            handle_pr_key(state, key);
                        }
                    }
                    View::ActionsRun(_) => {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            self.views.pop();
                        }
                    }
                    View::LogViewer(ref mut state) => {
                        if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                            self.views.pop();
                        } else {
                            handle_log_viewer_key(state, key);
                        }
                    }
                }
            }

            Event::Resize(_, _) => {
                // Terminal will re-render on next loop iteration.
            }

            _ => {}
        }
    }

    /// Invalidate all cached data (used on manual refresh `r`).
    pub fn invalidate_cache(&mut self) {
        for fs in self.fetch_state.values_mut() {
            fs.loading = true;
            fs.last_refreshed = None;
        }
        self.last_refreshed = None;
    }
}

// ── Key handlers for individual views ────────────────────────────────────────

use crate::navigation::DashboardState;
use crossterm::event::KeyEvent;

fn handle_dashboard_key(
    state: &mut DashboardState,
    key: KeyEvent,
    is_root: bool,
    _running: &mut bool,
    pending_quit: &mut bool,
) {
    if state.filter.active {
        match key.code {
            KeyCode::Esc => {
                state.filter.active = false;
                state.filter.text.clear();
            }
            KeyCode::Backspace => {
                state.filter.text.pop();
            }
            KeyCode::Char(c) => {
                state.filter.text.push(c);
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            state.selected = state.selected.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Char('G') => {
            // Jump to last — clamping happens in the renderer.
            state.selected = usize::MAX;
        }
        KeyCode::Char('/') => {
            state.filter.active = true;
        }
        KeyCode::Char('q') | KeyCode::Esc if is_root => {
            *pending_quit = true;
        }
        _ => {}
    }
}

fn handle_pr_key(state: &mut PrViewState, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            state.body_scroll += 1;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.body_scroll = state.body_scroll.saturating_sub(1);
        }
        KeyCode::Tab => {
            state.focused_panel = (state.focused_panel + 1) % 3;
        }
        _ => {}
    }
}

fn handle_log_viewer_key(state: &mut LogViewerState, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            state.scroll = state.scroll.saturating_add(1);
            state.at_bottom = false;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.scroll = state.scroll.saturating_sub(1);
            state.at_bottom = false;
        }
        KeyCode::Char('G') => {
            state.scroll = state.total_lines.saturating_sub(1);
            state.at_bottom = true;
        }
        KeyCode::Char(']') | KeyCode::Char('n') => {
            // Jump to next step.
            let next = state
                .step_starts
                .iter()
                .find(|&&s| s > state.scroll)
                .copied();
            if let Some(s) = next {
                state.scroll = s;
                // Update current step index.
                state.current_step = state
                    .step_starts
                    .iter()
                    .position(|&x| x == s)
                    .unwrap_or(state.current_step);
            }
        }
        KeyCode::Char('[') | KeyCode::Char('p') => {
            // Jump to previous step.
            let prev = state
                .step_starts
                .iter()
                .rev()
                .find(|&&s| s < state.scroll)
                .copied();
            if let Some(s) = prev {
                state.scroll = s;
                state.current_step = state
                    .step_starts
                    .iter()
                    .position(|&x| x == s)
                    .unwrap_or(state.current_step);
            }
        }
        _ => {}
    }
}

// ── Step boundary parsing ─────────────────────────────────────────────────────

/// Parse step boundary line indices from GitHub Actions log lines.
/// GitHub uses `##[group]` prefixes or timestamp + step markers.
pub fn parse_step_boundaries(lines: &[String]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            if line.contains("##[group]") || line.contains("##[endgroup]") {
                None // only group starts count
            } else if line.contains("##[group]") {
                Some(i)
            } else {
                // Also match GitHub's timestamp-prefixed step headers like:
                // "2024-01-01T00:00:00.0000000Z ##[group]Run actions/checkout@v4"
                if line.contains("##[group]") {
                    Some(i)
                } else {
                    None
                }
            }
        })
        .collect()
}

/// Parse step boundaries more carefully — match lines containing `##[group]`.
pub fn parse_step_boundaries_v2(lines: &[String]) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| {
            if line.contains("##[group]") {
                Some(i)
            } else {
                None
            }
        })
        .collect()
}

// ── Main event loop ───────────────────────────────────────────────────────────

/// Run the application until the user quits.
pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    config: Config,
    token: String,
) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    let mut state = AppState::new(&config.repos);

    // Spawn poll tasks for each repo.
    for repo_key in &config.repos {
        let parts: Vec<&str> = repo_key.splitn(2, '/').collect();
        if parts.len() != 2 {
            continue;
        }
        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        let tx = tx.clone();
        let token = token.clone();
        let dashboard_interval = config.refresh.dashboard;

        tokio::spawn(async move {
            let client = match GitHubClient::new(token) {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(AppEvent::FetchError {
                        repo: format!("{owner}/{repo}"),
                        message: e.to_string(),
                    });
                    return;
                }
            };
            // Use a Mutex so the client can be shared across poll iterations.
            let client = std::sync::Arc::new(tokio::sync::Mutex::new(client));
            let mut ticker = interval(Duration::from_secs(dashboard_interval));
            loop {
                ticker.tick().await;
                let mut c = client.lock().await;
                if c.is_rate_limited() {
                    continue;
                }
                let key = format!("{owner}/{repo}");
                let summary = c.fetch_repo_summary(&owner, &repo).await;
                let open_prs = c.fetch_open_prs(&owner, &repo, 20).await;
                let workflow_runs = c.fetch_workflow_runs(&owner, &repo, 10).await;

                let commits = if let Ok(ref s) = summary {
                    c.fetch_recent_commits(&owner, &repo, &s.default_branch, 10)
                        .await
                        .unwrap_or_default()
                } else {
                    vec![]
                };

                match summary {
                    Ok(s) => {
                        let data = RepoData {
                            key: key.clone(),
                            summary: Some(s),
                            commits,
                            open_prs: open_prs.unwrap_or_default(),
                            workflow_runs: workflow_runs.unwrap_or_default(),
                        };
                        let _ = tx.send(AppEvent::DataFetched(Box::new(data)));
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::FetchError {
                            repo: key,
                            message: e.to_string(),
                        });
                    }
                }
            }
        });
    }

    // Tick task (for time-based UI updates).
    let tick_tx = tx.clone();
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            if tick_tx.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });

    // Terminal event task.
    let term_tx = tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        while let Some(Ok(event)) = reader.next().await {
            if term_tx.send(AppEvent::TerminalEvent(event)).is_err() {
                break;
            }
        }
    });

    // Main loop.
    while state.running {
        // Render.
        terminal.draw(|frame| {
            crate::ui::draw(frame, &state);
        })?;

        // Process next event.
        if let Some(event) = rx.recv().await {
            state.handle_event(event);
        }
    }

    Ok(())
}
