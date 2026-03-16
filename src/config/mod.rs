//! Configuration loading, token resolution, and validation.
#![allow(dead_code)]

use std::collections::HashMap;
use std::process::Command;

use figment::providers::{Format, Toml};
use figment::Figment;
use serde::Deserialize;
use thiserror::Error;

/// Errors that can occur during configuration loading or validation.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config file parse error in {path}: {message}")]
    ParseError { path: String, message: String },

    #[error("Invalid repo format '{entry}': expected 'owner/repo'")]
    InvalidRepoFormat { entry: String },

    #[error("No repositories configured. Add entries to the 'repos' key in your config file.")]
    NoRepos,

    #[error("Invalid refresh interval for '{key}': must be a positive integer, got {value}")]
    InvalidRefreshInterval { key: String, value: i64 },

    #[error("No GitHub token found. Set GITHUB_TOKEN, add 'token' to config, or set 'token_cmd'.")]
    NoToken,

    #[error("token_cmd '{cmd}' failed: {detail}")]
    TokenCmdFailed { cmd: String, detail: String },

    #[error(transparent)]
    Figment(#[from] figment::Error),
}

/// Refresh interval settings (in seconds).
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshConfig {
    /// How often to refresh dashboard data (default: 60s).
    #[serde(default = "default_dashboard_interval")]
    pub dashboard: u64,

    /// How often to refresh Actions run data (default: 30s).
    #[serde(default = "default_actions_interval")]
    pub actions: u64,

    /// How often to poll job logs when in-progress (default: 2s).
    #[serde(default = "default_logs_interval")]
    pub logs: u64,
}

fn default_dashboard_interval() -> u64 {
    60
}
fn default_actions_interval() -> u64 {
    30
}
fn default_logs_interval() -> u64 {
    2
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            dashboard: default_dashboard_interval(),
            actions: default_actions_interval(),
            logs: default_logs_interval(),
        }
    }
}

/// Top-level application configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// List of repositories to monitor, in `owner/repo` format.
    #[serde(default)]
    pub repos: Vec<String>,

    /// GitHub Personal Access Token (alternative to env var or token_cmd).
    #[serde(default)]
    pub token: Option<String>,

    /// Shell command whose stdout is used as the GitHub token.
    #[serde(default)]
    pub token_cmd: Option<String>,

    /// Refresh intervals.
    #[serde(default)]
    pub refresh: RefreshConfig,

    /// Named filter presets: map of preset name → list of repo strings.
    #[serde(default)]
    pub filters: HashMap<String, Vec<String>>,
}

/// Resolve the config file path using XDG base dirs, with a fallback.
fn config_path() -> Option<std::path::PathBuf> {
    // Primary: $XDG_CONFIG_HOME/xrepotui/config.toml (or ~/.config/xrepotui/config.toml)
    if let Some(config_dir) = dirs::config_dir() {
        let xdg_path = config_dir.join("xrepotui").join("config.toml");
        if xdg_path.exists() {
            return Some(xdg_path);
        }
    }

    // Fallback: ~/.xrepotui.toml
    if let Some(home_dir) = dirs::home_dir() {
        let fallback = home_dir.join(".xrepotui.toml");
        if fallback.exists() {
            return Some(fallback);
        }
    }

    None
}

/// Load and validate the application configuration.
#[allow(clippy::result_large_err)]
pub fn load() -> Result<Config, ConfigError> {
    let mut figment = Figment::new();

    if let Some(path) = config_path() {
        figment = figment.merge(Toml::file(&path));
    }
    // Allow XREPOTUI_ prefixed env vars to override config values.
    // (We don't use figment's Env provider for GITHUB_TOKEN — that's handled in token resolution.)

    let config: Config = figment.extract().map_err(|e| {
        let path = config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        ConfigError::ParseError {
            path,
            message: e.to_string(),
        }
    })?;

    validate(&config)?;
    Ok(config)
}

/// Validate the loaded configuration.
#[allow(clippy::result_large_err)]
fn validate(config: &Config) -> Result<(), ConfigError> {
    // Validate repo format.
    for entry in &config.repos {
        let parts: Vec<&str> = entry.splitn(2, '/').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(ConfigError::InvalidRepoFormat {
                entry: entry.clone(),
            });
        }
    }

    // Validate refresh intervals.
    // (Deserialized as u64 so they're already non-negative; just check for 0.)
    if config.refresh.dashboard == 0 {
        return Err(ConfigError::InvalidRefreshInterval {
            key: "dashboard".to_string(),
            value: 0,
        });
    }
    if config.refresh.actions == 0 {
        return Err(ConfigError::InvalidRefreshInterval {
            key: "actions".to_string(),
            value: 0,
        });
    }
    if config.refresh.logs == 0 {
        return Err(ConfigError::InvalidRefreshInterval {
            key: "logs".to_string(),
            value: 0,
        });
    }

    Ok(())
}

/// Resolve the GitHub Personal Access Token using the priority order:
/// 1. `GITHUB_TOKEN` environment variable
/// 2. `token` field in config file
/// 3. Output of `token_cmd` shell command
#[allow(clippy::result_large_err)]
pub fn resolve_token(config: &Config) -> Result<String, ConfigError> {
    // 1. Environment variable.
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            return Ok(token.trim().to_string());
        }
    }

    // 2. Config file token field.
    if let Some(ref token) = config.token {
        if !token.trim().is_empty() {
            return Ok(token.trim().to_string());
        }
    }

    // 3. token_cmd.
    if let Some(ref cmd) = config.token_cmd {
        let output = Command::new("sh").args(["-c", cmd]).output().map_err(|e| {
            ConfigError::TokenCmdFailed {
                cmd: cmd.clone(),
                detail: e.to_string(),
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(ConfigError::TokenCmdFailed {
                cmd: cmd.clone(),
                detail: format!("exited with {}: {}", output.status, stderr),
            });
        }

        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    Err(ConfigError::NoToken)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_valid_repos() {
        let config = Config {
            repos: vec!["owner/repo".to_string(), "org/project".to_string()],
            token: None,
            token_cmd: None,
            refresh: RefreshConfig::default(),
            filters: HashMap::new(),
        };
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn validate_invalid_repo_format() {
        let config = Config {
            repos: vec!["notavalidrepo".to_string()],
            token: None,
            token_cmd: None,
            refresh: RefreshConfig::default(),
            filters: HashMap::new(),
        };
        let err = validate(&config).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidRepoFormat { .. }));
    }

    #[test]
    fn validate_empty_owner() {
        let config = Config {
            repos: vec!["/repo".to_string()],
            token: None,
            token_cmd: None,
            refresh: RefreshConfig::default(),
            filters: HashMap::new(),
        };
        let err = validate(&config).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidRepoFormat { .. }));
    }

    #[test]
    fn token_from_env_takes_priority() {
        std::env::set_var("GITHUB_TOKEN", "env-token");
        let config = Config {
            repos: vec![],
            token: Some("config-token".to_string()),
            token_cmd: None,
            refresh: RefreshConfig::default(),
            filters: HashMap::new(),
        };
        let result = resolve_token(&config).unwrap();
        assert_eq!(result, "env-token");
        std::env::remove_var("GITHUB_TOKEN");
    }

    #[test]
    fn token_from_config_when_no_env() {
        std::env::remove_var("GITHUB_TOKEN");
        let config = Config {
            repos: vec![],
            token: Some("config-token".to_string()),
            token_cmd: None,
            refresh: RefreshConfig::default(),
            filters: HashMap::new(),
        };
        let result = resolve_token(&config).unwrap();
        assert_eq!(result, "config-token");
    }

    #[test]
    fn no_token_returns_error() {
        std::env::remove_var("GITHUB_TOKEN");
        let config = Config {
            repos: vec![],
            token: None,
            token_cmd: None,
            refresh: RefreshConfig::default(),
            filters: HashMap::new(),
        };
        let err = resolve_token(&config).unwrap_err();
        assert!(matches!(err, ConfigError::NoToken));
    }

    #[test]
    fn validate_zero_dashboard_interval() {
        let config = Config {
            repos: vec![],
            token: None,
            token_cmd: None,
            refresh: RefreshConfig {
                dashboard: 0,
                actions: 30,
                logs: 2,
            },
            filters: HashMap::new(),
        };
        let err = validate(&config).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidRefreshInterval { .. }));
    }
}
