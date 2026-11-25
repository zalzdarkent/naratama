# Contributing to Naratama Gelanggang

Thank you for your interest in contributing! This guide covers project conventions, code quality gates, and the pull request workflow.

---

## Development Setup

1. Install **Rust** (stable), **Node.js 20+**, and **Tauri CLI v2** (`cargo install tauri-cli --version "^2"`).
2. On Linux (Ubuntu/Debian), install WebKitGTK & GTK dependencies:
   ```bash
   sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev pkg-config
   ```
3. Install frontend dependencies:
   ```bash
   cd desktop && npm install
   ```
4. Launch the desktop app in dev mode:
   ```bash
   cd desktop && cargo tauri dev
   ```

---

## Architecture & Coding Standards

Please read [DECISIONS.md](DECISIONS.md) before making structural changes.

### Rust Workspace (`crates/*`)
- **Zero `unsafe` code** (`unsafe_code = "forbid"` at workspace level).
- **No `unwrap()` or `expect()` in library error paths** — propagate errors via `Result<T, NaratamaError>`.
- **Determinism**: Simulation logic in `naratama-sim` must accept an explicit `rng: &mut impl Rng` and never read wall-clock OS time (`Utc::now()` / `Local::now()`) for in-game dates.
- **Economy invariant**: Every club cash mutation must go through `FinanceRepo::record_tx` so `teams.budget == SUM(finance_ledger.amount)` always holds.
- **Clippy pedantic**: All workspace crates and `desktop/src-tauri` must pass Clippy with `-D warnings`.

### Desktop Frontend (`desktop/src/*`)
- Strict TypeScript (`tsc --noEmit`).
- Keep all user-facing strings localized in both `desktop/src/i18n/id.json` and `desktop/src/i18n/en.json` with identical key structures.
- Use `positionMeta(position_id)` for position badges (`GK/DEF/MID/ATT`), `formatMoney(amount)` for currency values, and `countryFlag(iso2, name)` for national team flags.
- Keep `desktop/src-tauri` excluded from the headless workspace `Cargo.toml` so CI can test simulation & DB crates without GUI libraries.

---

## Mandatory Verification Checklist

Run all of the following before opening a pull request:

```bash
# Rust formatting, lints, and full workspace tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Tauri v2 backend lints
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --all-targets -- -D warnings

# Frontend typecheck and production bundle
cd desktop && npm run build
```

---

## Pull Request Guidelines

- Keep changes focused and minimal; avoid unrelated refactors in feature/fix PRs.
- Include unit or integration tests for new engine rules or bug fixes.
- Describe *what* changed, *why*, and how it was verified.

