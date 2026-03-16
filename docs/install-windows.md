# Windows Installation Guide

## Winget

In an elevated powershell session, run:

```powershell
winget install keathmilligan.xrepotui
```

Stay up-to-date with `winget upgrade xrepotui`

## PowerShell Installer

In an elevated powershell session, run:

```powershell
irm https://packages.keathmilligan.net/xrepotui/install.ps1 | iex
```

## cargo

If you have Rust development tools installed you can install with `cargo`:

```bash
cargo install xrepotui
```

## Scoop (Windows)

```powershell
scoop bucket add keathmilligan https://github.com/keathmilligan/scoop-bucket
scoop install xrepotui
```

## Chocolatey (Windows)

```powershell
choco install xrepotui
```

## Windows MSI

Download the signed `.msi` installer directly from the [GitHub Releases](https://github.com/keathmilligan/xrepotui/releases) page.

## Binary

Download the Windows binary archive for your architecture (Intel `x86_64` or ARM `aarch64`) from the [GitHub Releases](https://github.com/keathmilligan/xrepotui/releases) page.

Extract the `xrepotui` binary into a directory in your `PATH`.
