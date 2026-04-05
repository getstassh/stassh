# Stassh

Stassh is a local-first SSH command deck for your terminal.
Manage hosts, check reachability, verify host keys, and jump into live SSH sessions from one TUI.

## Highlights

- Dark, keyboard-first terminal UI built with `ratatui` + `crossterm`
- Host cards with quick status checks (`unknown` / `reachable` / `unreachable`)
- Interactive SSH sessions powered by `russh`
- Host key trust prompts with fingerprint verification
- Optional local database encryption (Argon2 + AES-256-GCM)

## Stack

- Rust workspace (`crates/backend`, `crates/tui`)
- `tokio`, `vt100`, `russh`, `ratatui`, `crossterm`

## Run

```bash
cargo run -p tui
```

On first launch, Stassh walks you through encryption and telemetry preferences.

## Contributing

Please open an issue before contributing so we can discuss scope and approach first.
See `CONTRIBUTING.md` for contribution terms.

## License

Licensed under PolyForm Noncommercial 1.0.0. See `LICENSE.md`.

---

Created by Lazar - [bylazar.com](https://bylazar.com)
