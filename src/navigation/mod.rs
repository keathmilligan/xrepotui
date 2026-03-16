//! Navigation model: view stack, breadcrumbs, panel focus, filter input, and
//! vim-style list state.
#![allow(dead_code)]

// ── View State Types ──────────────────────────────────────────────────────────

/// State for the dashboard view.
#[derive(Debug, Clone, Default)]
pub struct DashboardState {
    /// Index of the selected repository row.
    pub selected: usize,
    /// Filter state for the repo list.
    pub filter: FilterState,
}

/// State for the repository drill-down view.
#[derive(Debug, Clone)]
pub struct RepoViewState {
    pub owner: String,
    pub repo: String,
    /// Which panel currently has focus (0 = commits+PRs, 1 = Actions runs).
    pub focused_panel: usize,
    /// Selected index in the commits list.
    pub selected_commit: usize,
    /// Selected index in the PRs list.
    pub selected_pr: usize,
    /// Selected index in the Actions runs list.
    pub selected_run: usize,
}

impl RepoViewState {
    pub fn new(owner: String, repo: String) -> Self {
        Self {
            owner,
            repo,
            focused_panel: 0,
            selected_commit: 0,
            selected_pr: 0,
            selected_run: 0,
        }
    }
}

/// State for the pull request detail view.
#[derive(Debug, Clone)]
pub struct PrViewState {
    pub owner: String,
    pub repo: String,
    pub pr_number: u64,
    /// Scroll offset for the PR body.
    pub body_scroll: usize,
    /// Scroll offset for the files-changed list.
    pub files_scroll: usize,
    /// Which panel has focus.
    pub focused_panel: usize,
}

impl PrViewState {
    pub fn new(owner: String, repo: String, pr_number: u64) -> Self {
        Self {
            owner,
            repo,
            pr_number,
            body_scroll: 0,
            files_scroll: 0,
            focused_panel: 0,
        }
    }
}

/// State for the Actions run view.
#[derive(Debug, Clone)]
pub struct ActionsViewState {
    /// Some(repo) if viewing a specific repo's run, None for cross-repo list.
    pub repo_context: Option<(String, String)>,
    pub run_id: Option<u64>,
    /// Selected job index.
    pub selected_job: usize,
    /// Which panel has focus (0 = jobs, 1 = steps).
    pub focused_panel: usize,
    /// Filter for cross-repo list.
    pub filter: FilterState,
}

impl ActionsViewState {
    pub fn new_run(owner: String, repo: String, run_id: u64) -> Self {
        Self {
            repo_context: Some((owner, repo)),
            run_id: Some(run_id),
            selected_job: 0,
            focused_panel: 0,
            filter: FilterState::default(),
        }
    }

    pub fn new_cross_repo() -> Self {
        Self {
            repo_context: None,
            run_id: None,
            selected_job: 0,
            focused_panel: 0,
            filter: FilterState::default(),
        }
    }
}

/// State for the log viewer.
#[derive(Debug, Clone)]
pub struct LogViewerState {
    pub owner: String,
    pub repo: String,
    pub job_id: u64,
    pub job_name: String,
    /// Current vertical scroll offset (line index).
    pub scroll: usize,
    /// Whether the viewport was at the bottom before the last update.
    pub at_bottom: bool,
    /// Indices of step boundary lines.
    pub step_starts: Vec<usize>,
    /// Currently active step index (0-based).
    pub current_step: usize,
    /// Total number of log lines loaded.
    pub total_lines: usize,
}

impl LogViewerState {
    pub fn new(owner: String, repo: String, job_id: u64, job_name: String) -> Self {
        Self {
            owner,
            repo,
            job_id,
            job_name,
            scroll: 0,
            at_bottom: true,
            step_starts: vec![],
            current_step: 0,
            total_lines: 0,
        }
    }
}

// ── Filter State ──────────────────────────────────────────────────────────────

/// State for the inline list filter input.
#[derive(Debug, Clone, Default)]
pub struct FilterState {
    pub active: bool,
    pub text: String,
}

impl FilterState {
    /// Returns true if the given name matches the current filter (case-insensitive).
    pub fn matches(&self, name: &str) -> bool {
        if self.text.is_empty() {
            return true;
        }
        name.to_lowercase().contains(&self.text.to_lowercase())
    }
}

// ── View Enum ─────────────────────────────────────────────────────────────────

/// A single entry in the navigation stack.
#[derive(Debug, Clone)]
pub enum View {
    Dashboard(DashboardState),
    Repo(RepoViewState),
    Pr(PrViewState),
    ActionsRun(ActionsViewState),
    LogViewer(LogViewerState),
}

impl View {
    /// Human-readable label for the breadcrumb.
    pub fn breadcrumb_label(&self) -> String {
        match self {
            View::Dashboard(_) => "Dashboard".to_string(),
            View::Repo(s) => format!("{}/{}", s.owner, s.repo),
            View::Pr(s) => format!("PR #{}", s.pr_number),
            View::ActionsRun(s) => {
                if let Some(id) = s.run_id {
                    format!("Run #{id}")
                } else {
                    "Actions".to_string()
                }
            }
            View::LogViewer(s) => format!("Log: {}", s.job_name),
        }
    }
}

// ── ViewStack ─────────────────────────────────────────────────────────────────

/// The navigation stack. Always has at least one entry (the dashboard).
#[derive(Debug, Clone)]
pub struct ViewStack {
    stack: Vec<View>,
}

impl ViewStack {
    /// Create a new stack with the dashboard as the root.
    pub fn new() -> Self {
        Self {
            stack: vec![View::Dashboard(DashboardState::default())],
        }
    }

    /// Push a new view onto the stack.
    pub fn push(&mut self, view: View) {
        self.stack.push(view);
    }

    /// Pop the top view. Returns the popped view if the stack has more than one entry.
    pub fn pop(&mut self) -> Option<View> {
        if self.stack.len() > 1 {
            self.stack.pop()
        } else {
            None
        }
    }

    /// Return a reference to the current (top) view.
    pub fn current(&self) -> &View {
        self.stack.last().expect("ViewStack is never empty")
    }

    /// Return a mutable reference to the current (top) view.
    pub fn current_mut(&mut self) -> &mut View {
        self.stack.last_mut().expect("ViewStack is never empty")
    }

    /// Returns true if this is the root (dashboard) view.
    pub fn is_root(&self) -> bool {
        self.stack.len() == 1
    }

    /// Generate the breadcrumb string from the current stack.
    pub fn breadcrumb(&self) -> String {
        self.stack
            .iter()
            .map(|v| v.breadcrumb_label())
            .collect::<Vec<_>>()
            .join(" > ")
    }
}

impl Default for ViewStack {
    fn default() -> Self {
        Self::new()
    }
}

// ── Vim-style List State ──────────────────────────────────────────────────────

/// Tracks selection in a list with vim-style navigation and `g g` detection.
#[derive(Debug, Clone, Default)]
pub struct ListState {
    pub selected: usize,
    pub len: usize,
    /// Whether the last key press was `g` (for `g g` detection).
    pub last_g: bool,
}

impl ListState {
    pub fn new(len: usize) -> Self {
        Self {
            selected: 0,
            len,
            last_g: false,
        }
    }

    /// Move selection down one item.
    pub fn move_down(&mut self) {
        self.last_g = false;
        if self.len > 0 && self.selected < self.len - 1 {
            self.selected += 1;
        }
    }

    /// Move selection up one item.
    pub fn move_up(&mut self) {
        self.last_g = false;
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Jump to the last item (`G`).
    pub fn jump_to_last(&mut self) {
        self.last_g = false;
        if self.len > 0 {
            self.selected = self.len - 1;
        }
    }

    /// Handle a `g` key press. On second consecutive `g`, jump to first item.
    /// Returns true if a jump occurred.
    pub fn handle_g(&mut self) -> bool {
        if self.last_g {
            self.selected = 0;
            self.last_g = false;
            true
        } else {
            self.last_g = true;
            false
        }
    }

    /// Update the list length (e.g., after filtering).
    pub fn update_len(&mut self, new_len: usize) {
        self.len = new_len;
        if self.selected >= new_len && new_len > 0 {
            self.selected = new_len - 1;
        }
    }
}

// ── Scroll State ──────────────────────────────────────────────────────────────

/// Scroll state for vertically scrollable content.
#[derive(Debug, Clone, Default)]
pub struct ScrollState {
    pub offset: usize,
    pub total_lines: usize,
    pub viewport_height: usize,
}

impl ScrollState {
    /// Returns true if the viewport is at the last line.
    pub fn is_at_bottom(&self) -> bool {
        if self.total_lines <= self.viewport_height {
            return true;
        }
        self.offset >= self.total_lines - self.viewport_height
    }

    /// Scroll down one line.
    pub fn scroll_down(&mut self) {
        if self.total_lines > self.viewport_height {
            let max = self.total_lines - self.viewport_height;
            if self.offset < max {
                self.offset += 1;
            }
        }
    }

    /// Scroll up one line.
    pub fn scroll_up(&mut self) {
        if self.offset > 0 {
            self.offset -= 1;
        }
    }

    /// Scroll down half a page.
    pub fn page_down(&mut self) {
        let half = (self.viewport_height / 2).max(1);
        for _ in 0..half {
            self.scroll_down();
        }
    }

    /// Scroll up half a page.
    pub fn page_up(&mut self) {
        let half = (self.viewport_height / 2).max(1);
        for _ in 0..half {
            self.scroll_up();
        }
    }

    /// Jump to the last line.
    pub fn jump_to_end(&mut self) {
        if self.total_lines > self.viewport_height {
            self.offset = self.total_lines - self.viewport_height;
        }
    }

    /// Jump to the first line.
    pub fn jump_to_start(&mut self) {
        self.offset = 0;
    }

    /// Advance offset to follow new lines appended at the bottom.
    pub fn follow_to_end(&mut self) {
        self.jump_to_end();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_stack_starts_with_dashboard() {
        let stack = ViewStack::new();
        assert!(matches!(stack.current(), View::Dashboard(_)));
        assert!(stack.is_root());
    }

    #[test]
    fn view_stack_push_and_pop() {
        let mut stack = ViewStack::new();
        stack.push(View::Repo(RepoViewState::new(
            "owner".into(),
            "repo".into(),
        )));
        assert!(!stack.is_root());
        assert!(matches!(stack.current(), View::Repo(_)));

        stack.pop();
        assert!(stack.is_root());
        assert!(matches!(stack.current(), View::Dashboard(_)));
    }

    #[test]
    fn view_stack_pop_at_root_returns_none() {
        let mut stack = ViewStack::new();
        assert!(stack.pop().is_none());
        assert!(stack.is_root());
    }

    #[test]
    fn breadcrumb_reflects_stack() {
        let mut stack = ViewStack::new();
        assert_eq!(stack.breadcrumb(), "Dashboard");

        stack.push(View::Repo(RepoViewState::new(
            "owner".into(),
            "repo".into(),
        )));
        assert_eq!(stack.breadcrumb(), "Dashboard > owner/repo");

        stack.push(View::Pr(PrViewState::new(
            "owner".into(),
            "repo".into(),
            42,
        )));
        assert_eq!(stack.breadcrumb(), "Dashboard > owner/repo > PR #42");
    }

    #[test]
    fn filter_state_matches_case_insensitive() {
        let f = FilterState {
            active: true,
            text: "foo".to_string(),
        };
        assert!(f.matches("foobar"));
        assert!(f.matches("FOO"));
        assert!(!f.matches("bar"));
    }

    #[test]
    fn filter_state_empty_matches_all() {
        let f = FilterState::default();
        assert!(f.matches("anything"));
        assert!(f.matches(""));
    }

    #[test]
    fn list_state_vim_navigation() {
        let mut ls = ListState::new(5);
        ls.move_down();
        ls.move_down();
        assert_eq!(ls.selected, 2);
        ls.move_up();
        assert_eq!(ls.selected, 1);
        ls.jump_to_last();
        assert_eq!(ls.selected, 4);
    }

    #[test]
    fn list_state_g_g_jumps_to_first() {
        let mut ls = ListState::new(5);
        ls.selected = 4;
        ls.handle_g(); // first g
        let jumped = ls.handle_g(); // second g
        assert!(jumped);
        assert_eq!(ls.selected, 0);
    }
}
