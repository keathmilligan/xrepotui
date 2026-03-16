//! xrepotui: Multi-repository TUI dashboard and interface for Git and GitHub.

mod app;
mod config;
mod github;
mod navigation;
mod ui;

use std::io;
use std::panic;

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// Restore the terminal to a usable state. Called on both clean exit and panic.
fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Install a panic hook that restores the terminal before printing the panic message,
    // so the user sees a readable error rather than a garbled screen.
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));

    // Load configuration — exit with a clear error if invalid.
    let config = config::load().map_err(|e| {
        anyhow::anyhow!("Failed to load configuration: {}", e)
    })?;

    // Resolve GitHub token.
    let token = config::resolve_token(&config).map_err(|e| {
        anyhow::anyhow!("Failed to resolve GitHub token: {}", e)
    })?;

    // Set up terminal.
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Run the application.
    let result = app::run(&mut terminal, config, token).await;

    // Always restore terminal, even on error.
    restore_terminal();
    terminal.show_cursor()?;

    result
}
