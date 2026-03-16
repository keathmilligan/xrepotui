[![CI](https://github.com/keathmilligan/xrepotui/actions/workflows/ci.yml/badge.svg)](https://github.com/keathmilligan/xrepotui/actions/workflows/ci.yml)
[![Release](https://github.com/keathmilligan/xrepotui/actions/workflows/release.yml/badge.svg?branch=v0.1.0)](https://github.com/keathmilligan/xrepotui/actions/workflows/release.yml)
[![macOS DMG](https://packages.keathmilligan.net/xrepotui/badges/macos-dmg.svg)](https://github.com/keathmilligan/xrepotui/releases/latest)
[![Windows MSI](https://packages.keathmilligan.net/xrepotui/badges/msi.svg)](https://github.com/keathmilligan/xrepotui/releases/latest)
[![Homebrew](https://packages.keathmilligan.net/xrepotui/badges/homebrew.svg)](https://github.com/keathmilligan/xrepotui/releases/latest)
[![Scoop](https://packages.keathmilligan.net/xrepotui/badges/scoop.svg)](https://github.com/keathmilligan/xrepotui/releases/latest)
[![apt](https://packages.keathmilligan.net/xrepotui/badges/apt.svg)](https://github.com/keathmilligan/xrepotui/releases/latest)
[![rpm](https://packages.keathmilligan.net/xrepotui/badges/rpm.svg)](https://github.com/keathmilligan/xrepotui/releases/latest)
[![crates.io](https://packages.keathmilligan.net/xrepotui/badges/crates-io.svg)](https://github.com/keathmilligan/xrepotui/releases/latest)
[![Install Scripts](https://packages.keathmilligan.net/xrepotui/badges/install-scripts.svg)](https://github.com/keathmilligan/xrepotui/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

_`Cross-Repo TUI`_

A terminal UI dashboard for monitoring and managing multiple GitHub repositories at a glance.

---

## Overview

`xrepotui` provides a unified TUI for tracking activity across multiple GitHub repositories:

- **Repository summary** — stars, forks, open PRs, open issues, language, visibility
- **Recent commits** — author, message, and timestamp for each repo
- **Pull requests** — open PRs with review status, checks, and changed files
- **GitHub Actions** — live workflow run status, job and step breakdown, streamed logs
- **Cross-repo dashboard** — view all repos and active CI runs in one screen

`xrepotui` is written in Rust and runs on macOS, Linux and Windows (Intel, ARM and Apple Silicon).

## Installation and Update

There are many ways to install `xrepotui` depending on your platform.

### macOS (Homebrew)

```bash
brew tap keathmilligan/tap
brew install keathmilligan/tap/xrepotui
```

Stay up-to-date with `brew upgrade xrepotui`.

See the [macOS Install Guide](docs/install-macos.md) for other ways to install on macOS.

### Windows (PowerShell)

In an elevated powershell session, run:

```powershell
irm https://packages.keathmilligan.net/xrepotui/install.ps1 | iex
```

See the [Windows Install Guide](docs/install-windows.md) for other ways to install on Windows.

### Linux (shell installer)

```bash
curl -fsSL https://packages.keathmilligan.net/xrepotui/install.sh | sh
```

This will install `xrepotui` into `~/.local/bin`.

See the [Linux Install Guide](docs/install-linux.md) for other ways to install on Linux.

## Configuration

Create `~/.config/xrepotui/config.toml` (or `~/.xrepotui.toml` as a fallback):

```toml
# GitHub personal access token (or use GITHUB_TOKEN env var)
token = "ghp_..."

# Repositories to track
repos = [
  "owner/repo1",
  "owner/repo2",
]

# Optional: run a command to retrieve the token
# token_cmd = "gh auth token"

[refresh]
dashboard = 60   # seconds
actions = 30
logs = 2
```

A `GITHUB_TOKEN` environment variable takes precedence over the config file token.

## Key Bindings

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up |
| `gg` / `G` | Jump to top / bottom |
| `Enter` | Open selected item |
| `Esc` / `Backspace` | Go back |
| `/` | Filter |
| `r` | Refresh |
| `q` | Quit |

## License

MIT
