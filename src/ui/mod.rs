//! UI rendering: all views, layout, status bar, and help overlay.

use chrono::{DateTime, Utc};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Row, Table, Wrap};
use ratatui::Frame;

use crate::app::AppState;
use crate::github::models::*;
use crate::navigation::View;

// ── Colour palette ────────────────────────────────────────────────────────────

const COLOR_ACCENT: Color = Color::Cyan;
const COLOR_SUCCESS: Color = Color::Green;
const COLOR_FAILURE: Color = Color::Red;
const COLOR_WARNING: Color = Color::Yellow;
const COLOR_MUTED: Color = Color::DarkGray;

// ── Entry point ───────────────────────────────────────────────────────────────

/// Draw the full TUI frame.
pub fn draw(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    // Minimum dimensions check.
    if area.width < 100 || area.height < 30 {
        let msg = Paragraph::new(format!(
            "Terminal too small ({} x {}). Minimum: 100 x 30.",
            area.width, area.height
        ))
        .style(Style::default().fg(COLOR_WARNING));
        frame.render_widget(msg, area);
        return;
    }

    // Root layout: breadcrumb (1) | content (fill) | status bar (1).
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    draw_breadcrumb(frame, state, chunks[0]);
    draw_content(frame, state, chunks[1]);
    draw_status_bar(frame, state, chunks[2]);

    // Overlays (drawn on top).
    if state.pending_quit {
        draw_quit_confirm(frame, area);
    }
}

// ── Breadcrumb ────────────────────────────────────────────────────────────────

fn draw_breadcrumb(frame: &mut Frame, state: &AppState, area: Rect) {
    let crumb = state.views.breadcrumb();
    let line = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(
            crumb,
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

// ── Content dispatcher ────────────────────────────────────────────────────────

fn draw_content(frame: &mut Frame, state: &AppState, area: Rect) {
    match state.views.current() {
        View::Dashboard(_) => draw_dashboard(frame, state, area),
        View::Repo(s) => draw_repo_view(frame, state, s, area),
        View::Pr(s) => draw_pr_view(frame, state, s, area),
        View::ActionsRun(s) => draw_actions_view(frame, state, s, area),
        View::LogViewer(s) => draw_log_viewer(frame, state, s, area),
    }
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn draw_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let mut spans = vec![];

    // Rate limit info.
    if state.rate_limit.is_blocked {
        spans.push(Span::styled(
            " [RATE LIMITED] ",
            Style::default()
                .fg(COLOR_FAILURE)
                .add_modifier(Modifier::BOLD),
        ));
    } else if let (Some(remaining), Some(limit)) =
        (state.rate_limit.remaining, state.rate_limit.limit)
    {
        spans.push(Span::styled(
            format!(" API: {remaining}/{limit} "),
            Style::default().fg(COLOR_MUTED),
        ));
    }

    // Last refreshed.
    if let Some(when) = state.last_refreshed {
        let secs = when.elapsed().as_secs();
        spans.push(Span::styled(
            format!(" refreshed {secs}s ago "),
            Style::default().fg(COLOR_MUTED),
        ));
    }

    // Key hint.
    spans.push(Span::styled(
        " r:refresh  ?:help  q:back ",
        Style::default().fg(COLOR_MUTED),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ── Dashboard view ────────────────────────────────────────────────────────────

fn draw_dashboard(frame: &mut Frame, state: &AppState, area: Rect) {
    // Split: repo table (top 60%) + active runs (bottom 40%).
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    draw_dashboard_table(frame, state, chunks[0]);
    draw_active_runs(frame, state, chunks[1]);
}

fn draw_dashboard_table(frame: &mut Frame, state: &AppState, area: Rect) {
    let selected = if let View::Dashboard(ref s) = *state.views.current() {
        s.selected
    } else {
        0
    };

    let header = Row::new(vec!["Repository", "CI", "PRs", "Issues", "Last Commit"]).style(
        Style::default()
            .fg(COLOR_ACCENT)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = state
        .repo_data
        .values()
        .enumerate()
        .map(|(i, data)| {
            let fetch = state.fetch_state.get(&data.key);
            let is_loading = fetch.map(|f| f.loading).unwrap_or(true);
            let error = fetch.and_then(|f| f.error.as_deref());

            let style = if i == selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            if let Some(err_msg) = error {
                Row::new(vec![
                    data.key.clone(),
                    "ERR".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    err_msg.chars().take(30).collect::<String>(),
                ])
                .style(style.fg(COLOR_FAILURE))
            } else if is_loading {
                Row::new(vec![
                    data.key.clone(),
                    "...".to_string(),
                    "...".to_string(),
                    "...".to_string(),
                    "Loading...".to_string(),
                ])
                .style(style.fg(COLOR_MUTED))
            } else if let Some(ref summary) = data.summary {
                let ci_status = ci_status_indicator(&data.workflow_runs);
                let ci_style = ci_conclusion_color(&data.workflow_runs);
                let last_commit = data
                    .commits
                    .first()
                    .map(|c| format_relative_time(c.timestamp))
                    .unwrap_or_else(|| "-".to_string());

                Row::new(vec![
                    Span::raw(summary.name.clone()),
                    Span::styled(ci_status, ci_style),
                    Span::raw(summary.open_pr_count.to_string()),
                    Span::raw(summary.open_issue_count.to_string()),
                    Span::raw(last_commit),
                ])
                .style(style)
            } else {
                Row::new(vec![
                    data.key.clone(),
                    "-".into(),
                    "-".into(),
                    "-".into(),
                    "-".into(),
                ])
                .style(style)
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Percentage(30),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Repositories "),
    );

    frame.render_widget(table, area);
}

fn draw_active_runs(frame: &mut Frame, state: &AppState, area: Rect) {
    let active: Vec<&WorkflowRunSummary> = state
        .repo_data
        .values()
        .flat_map(|d| &d.workflow_runs)
        .filter(|r| r.status == RunStatus::InProgress || r.status == RunStatus::Queued)
        .collect();

    let items: Vec<ListItem> = if active.is_empty() {
        vec![ListItem::new("  No active runs")]
    } else {
        active
            .iter()
            .map(|r| {
                let repo = r.repo.as_deref().unwrap_or("-");
                let elapsed = format_relative_time(r.created_at);
                let text = format!("  {repo}  {}  {}  ({})", r.workflow_name, r.event, elapsed);
                ListItem::new(text).style(Style::default().fg(COLOR_WARNING))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Active Runs "),
    );
    frame.render_widget(list, area);
}

// ── Repo view ─────────────────────────────────────────────────────────────────

fn draw_repo_view(
    frame: &mut Frame,
    state: &AppState,
    view: &crate::navigation::RepoViewState,
    area: Rect,
) {
    let key = format!("{}/{}", view.owner, view.repo);
    let data = state.repo_data.get(&key);

    // Two-panel: left (commits + PRs) | right (Actions runs).
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left panel: splits vertically into commits (top) and PRs (bottom).
    let left_panels = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(panels[0]);

    // Commits.
    let commit_items: Vec<ListItem> = data
        .map(|d| {
            d.commits
                .iter()
                .map(|c| {
                    ListItem::new(format!(
                        "{} {} {}",
                        c.short_sha,
                        c.author,
                        c.message.chars().take(40).collect::<String>()
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    let commit_border = if view.focused_panel == 0 {
        COLOR_ACCENT
    } else {
        Color::White
    };
    let commits_list = List::new(commit_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Commits ")
            .border_style(Style::default().fg(commit_border)),
    );
    frame.render_widget(commits_list, left_panels[0]);

    // PRs.
    let pr_items: Vec<ListItem> = data
        .map(|d| {
            d.open_prs
                .iter()
                .map(|pr| {
                    ListItem::new(format!(
                        "#{} {} ({})",
                        pr.number,
                        pr.title.chars().take(40).collect::<String>(),
                        pr.author
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    let pr_list = List::new(pr_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Open PRs ")
            .border_style(Style::default().fg(commit_border)),
    );
    frame.render_widget(pr_list, left_panels[1]);

    // Right panel: Actions runs.
    let run_items: Vec<ListItem> = data
        .map(|d| {
            d.workflow_runs
                .iter()
                .map(|r| {
                    let status = format_run_status(&r.status, &r.conclusion);
                    ListItem::new(format!("{} {} {}", r.workflow_name, r.event, status))
                })
                .collect()
        })
        .unwrap_or_default();

    let run_border = if view.focused_panel == 1 {
        COLOR_ACCENT
    } else {
        Color::White
    };
    let runs_list = List::new(run_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Actions Runs ")
            .border_style(Style::default().fg(run_border)),
    );
    frame.render_widget(runs_list, panels[1]);
}

// ── PR view ───────────────────────────────────────────────────────────────────

fn draw_pr_view(
    frame: &mut Frame,
    state: &AppState,
    view: &crate::navigation::PrViewState,
    area: Rect,
) {
    if let Some(ref detail) = state.pr_detail {
        // Header block.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(0)])
            .split(area);

        let mergeable_indicator = match detail.mergeable_state {
            MergeableState::Mergeable => {
                Span::styled("✓ Mergeable", Style::default().fg(COLOR_SUCCESS))
            }
            MergeableState::Conflicting => {
                Span::styled("✗ Conflicts", Style::default().fg(COLOR_FAILURE))
            }
            MergeableState::Unknown => Span::styled("? Unknown", Style::default().fg(COLOR_MUTED)),
        };

        let header_text = vec![
            Line::from(format!("PR #{}: {}", detail.number, detail.title)),
            Line::from(format!(
                "Author: {}  {}→{}",
                detail.author, detail.head_branch, detail.base_branch
            )),
            Line::from(vec![mergeable_indicator]),
        ];

        let header = Paragraph::new(header_text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Pull Request "),
        );
        frame.render_widget(header, chunks[0]);

        // Body / files.
        let body_text = detail.body.as_deref().unwrap_or("(no description)");
        let body = Paragraph::new(body_text)
            .wrap(Wrap { trim: false })
            .scroll((view.body_scroll as u16, 0))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Description "),
            );
        frame.render_widget(body, chunks[1]);
    } else {
        let loading = Paragraph::new("Loading PR details...")
            .style(Style::default().fg(COLOR_MUTED))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(loading, area);
    }
}

// ── Actions view ──────────────────────────────────────────────────────────────

fn draw_actions_view(
    frame: &mut Frame,
    _state: &AppState,
    view: &crate::navigation::ActionsViewState,
    area: Rect,
) {
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left: jobs list placeholder (full jobs data lives in RepoData's workflow_runs/jobs).
    let jobs_block = Block::default()
        .borders(Borders::ALL)
        .title(" Jobs ")
        .border_style(Style::default().fg(if view.focused_panel == 0 {
            COLOR_ACCENT
        } else {
            Color::White
        }));
    frame.render_widget(jobs_block, panels[0]);

    // Right: steps list placeholder.
    let steps_block = Block::default()
        .borders(Borders::ALL)
        .title(" Steps ")
        .border_style(Style::default().fg(if view.focused_panel == 1 {
            COLOR_ACCENT
        } else {
            Color::White
        }));
    frame.render_widget(steps_block, panels[1]);
}

// ── Log viewer ────────────────────────────────────────────────────────────────

fn draw_log_viewer(
    frame: &mut Frame,
    state: &AppState,
    view: &crate::navigation::LogViewerState,
    area: Rect,
) {
    if state.log_loading {
        let loading = Paragraph::new("Fetching log...")
            .style(Style::default().fg(COLOR_MUTED))
            .block(Block::default().borders(Borders::ALL).title(" Log "));
        frame.render_widget(loading, area);
        return;
    }

    let lines = state.log_content.get(&view.job_id);
    let content = if let Some(lines) = lines {
        let start = view.scroll;
        let height = area.height as usize;
        let visible: Vec<Line> = lines
            .iter()
            .skip(start)
            .take(height)
            .map(|l| {
                // Simple plain render — ansi-to-tui conversion would go here.
                Line::from(Span::raw(l.clone()))
            })
            .collect();
        visible
    } else {
        vec![Line::from("(no log content)")]
    };

    let step_info = if !view.step_starts.is_empty() {
        format!(
            " Step {}/{} ",
            view.current_step + 1,
            view.step_starts.len()
        )
    } else {
        String::new()
    };

    let log_widget = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Log: {}{step_info}", view.job_name)),
    );
    frame.render_widget(log_widget, area);
}

// ── Quit confirm overlay ──────────────────────────────────────────────────────

fn draw_quit_confirm(frame: &mut Frame, area: Rect) {
    let width = 36u16;
    let height = 3u16;
    let x = area.width.saturating_sub(width) / 2;
    let y = area.height.saturating_sub(height) / 2;
    let popup = Rect {
        x,
        y,
        width,
        height,
    };

    frame.render_widget(Clear, popup);
    let confirm = Paragraph::new(" Quit xrepotui? (y/n) ")
        .block(Block::default().borders(Borders::ALL).title(" Quit "))
        .style(Style::default().fg(COLOR_WARNING));
    frame.render_widget(confirm, popup);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ci_status_indicator(runs: &[WorkflowRunSummary]) -> String {
    match runs.first() {
        None => "  -  ".to_string(),
        Some(r) => match r.status {
            RunStatus::InProgress | RunStatus::Queued => " ... ".to_string(),
            RunStatus::Completed => match &r.conclusion {
                Some(RunConclusion::Success) => "  ✓  ".to_string(),
                Some(RunConclusion::Failure) => "  ✗  ".to_string(),
                Some(RunConclusion::Cancelled) => "  ∅  ".to_string(),
                _ => "  -  ".to_string(),
            },
            _ => "  -  ".to_string(),
        },
    }
}

fn ci_conclusion_color(runs: &[WorkflowRunSummary]) -> Style {
    match runs.first() {
        None => Style::default().fg(COLOR_MUTED),
        Some(r) => match r.status {
            RunStatus::InProgress | RunStatus::Queued => Style::default().fg(COLOR_WARNING),
            RunStatus::Completed => match &r.conclusion {
                Some(RunConclusion::Success) => Style::default().fg(COLOR_SUCCESS),
                Some(RunConclusion::Failure) => Style::default().fg(COLOR_FAILURE),
                _ => Style::default().fg(COLOR_MUTED),
            },
            _ => Style::default().fg(COLOR_MUTED),
        },
    }
}

fn format_run_status(status: &RunStatus, conclusion: &Option<RunConclusion>) -> String {
    match status {
        RunStatus::InProgress => "in_progress".to_string(),
        RunStatus::Queued => "queued".to_string(),
        RunStatus::Completed => conclusion
            .as_ref()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "completed".to_string()),
        _ => status.to_string(),
    }
}

fn format_relative_time(dt: DateTime<Utc>) -> String {
    let secs = Utc::now().signed_duration_since(dt).num_seconds();
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}
