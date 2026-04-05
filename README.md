# Stassh

<p align="center">
  <img src="/logo.svg" alt="Stassh logo" width="160" />
</p>

Stassh is a local-first SSH command deck for your terminal.
Manage hosts, check reachability, verify host keys, and jump into live SSH sessions from one TUI.

![Stassh overview](/combined.png)

## Features

### Keyboard-First Control

Manage your SSH workflow from a fast terminal UI built for quick navigation, host selection, and session launching without leaving the keyboard.

### All Hosts In One Place

Keep your infrastructure organized in a single command deck where you can browse, select, and connect to hosts from one interface.

### Instant Reachability Checks

See whether a host is `unknown`, `reachable`, or `unreachable` before connecting so you can act faster and avoid wasted SSH attempts.

### Built-In Live SSH Sessions

Launch interactive SSH sessions directly inside the app, reducing context switching and keeping your workflow focused.

### Safer Host Verification

Review host key fingerprints and trust prompts before connecting so you can verify identity instead of blindly accepting new hosts.

### Local-First Workflow

Run everything locally with a responsive TUI experience that keeps host management close to your terminal and under your control.

## Security

### Encrypted Local Data

Protect stored data with optional local database encryption using Argon2 and AES-256-GCM for strong at-rest security.

### Host Key Fingerprint Checks

Verify server identities through fingerprint prompts to help prevent connecting to the wrong machine or accepting spoofed hosts.

### User-Controlled Trust

Decide when to trust a host and when to reject it, giving you explicit control over connection safety instead of hidden automation.

## Stack

- Rust workspace (`crates/backend`, `crates/tui`)
- `tokio`, `vt100`, `russh`, `ratatui`, `crossterm`

## Install

### GitHub Releases

Download the archive for your platform from [GitHub Releases](https://github.com/getstassh/stassh/releases), extract it, and place `stassh` somewhere on your `PATH`.

### Install Script

```bash
curl -fsSL https://raw.githubusercontent.com/getstassh/stassh/main/install.sh | sh
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/getstassh/stassh/main/install.sh | sh -s -- --version v0.1.0
```

### Cargo

```bash
cargo install --git https://github.com/getstassh/stassh stassh --locked
```

## Run From Source

```bash
cargo run -p stassh
```

On first launch, Stassh walks you through encryption and telemetry preferences.

## Contributing

Please open an issue before contributing so we can discuss scope and approach first.
See `CONTRIBUTING.md` for contribution terms.

## License

Licensed under PolyForm Noncommercial 1.0.0. See `LICENSE.md`.

---

Created by Lazar - [bylazar.com](https://bylazar.com)
