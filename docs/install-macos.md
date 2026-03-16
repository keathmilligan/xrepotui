# `xrepotui` macOS Installation Guide

## Homebrew

```bash
brew tap keathmilligan/tap
brew install keathmilligan/tap/xrepotui
```

Stay up-to-date with `brew upgrade xrepotui`.

## Shell Installer

```bash
curl -fsSL https://packages.keathmilligan.net/xrepotui/install.sh | sh
```

This will install `xrepotui` into `~/.local/bin`.

## cargo

If you have Rust development tools installed:

```bash
cargo install xrepotui
```

## dmg Installer

Download the signed `.dmg` installer for your platform architecture (Apple Silicon `aarch64` or Intel `x86_64`) directly from the [GitHub Releases](https://github.com/keathmilligan/xrepotui/releases) page.

## Binary

Download the macOS binary archive for your architecture (Apple Silicon `aarch64` or Intel `x86_64`) from the [GitHub Releases](https://github.com/keathmilligan/xrepotui/releases) page.

Extract the `xrepotui` binary into a directory in your `PATH`.
