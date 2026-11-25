# Naratama Gelanggang

**Naratama Gelanggang** is a 100% offline desktop football management simulation built with **Rust**, **SQLite**, **Tauri v2**, and **React + TypeScript**.

Manage any of **2,014 clubs** across **254 nations** and **55,600+ players**—or found your own custom club—with a deterministic match engine, multi-tier domestic & continental competitions, a full 4-year international football cycle, club economy ledger, scouting & youth academy, and bilingual (`ID` / `EN`) multi-pane desktop UI.

---

## Features

- **Deterministic Match Engine & Live Match Control**: 90-minute simulation, extra time, penalty shootouts, VAR checks, injuries, competition-scoped suspensions, live tactical mentality shifts, substitutions, match replay, and detailed post-match statistics.
- **Tactics & Squad Management**: Interactive 2D tactical pitch (`4-4-2`, `4-3-3`, `3-5-2`, `4-2-3-1`, `5-3-2`, `4-1-4-1`), formation-aware Auto XI, click-to-swap starters/bench, opponent tactical dossiers, focused training, and contract renewals.
- **Club Competitions**: Domestic multi-division leagues (promotion & relegation), national cups, and continental club competitions filtered per country (`international_slots`) with group tables and horizontal knockout brackets.
- **International Football & FIFA Hub**: 254-nation FIFA World Rankings (SUM Elo) with flags & rank trends, FIFA international windows, 4-year tournament cycle (World Cup 48-team, UEFA EURO, AFC Asian Cup, UEFA Nations League, FIFA ASEAN Cup, Friendlies), dual-nationality eligibility & cap-tie rules, and player release requests.
- **Transfers, Scouting & Youth Academy**: Transfer windows, permanent & loan offers, incoming bids, scheduled transfers, regional scout network, and youth academy promotion.
- **Club Economy & Offseason**: Immutable double-entry finance ledger (`teams.budget == SUM(finance_ledger.amount)`), broadcast pools, sponsorships, stadium expansion & ticket pricing, staff hiring, facilities (Lv 1–5), FFP modes, and a 7-step offseason transition wizard.
- **5 Save Slots + Export/Import**: Atomic SQLite `.session` isolation with checkpoint saves, `.zip` archive export/import, and a Custom Club Creator with procedural SVG crests.

---

## Architecture

```text
naratama/
├── assets/
│   └── data/               # Bundled SQLite seed DB & default club/league logos
├── crates/
│   ├── naratama-model/     # Pure domain types, strongly-typed IDs, enums, errors
│   ├── naratama-sim/       # Deterministic match engine, scheduler, valuation, Elo, finance calc
│   ├── naratama-db/        # SQLite repositories, migrations, 5-slot SessionManager
│   └── naratama-engine/    # Career orchestration, matchday, transfers, intl, news, read facade
├── desktop/
│   ├── src/                # React + Vite + TypeScript multi-pane desktop UI (ID/EN)
│   └── src-tauri/          # Tauri v2 native shell & IPC command layer
└── DECISIONS.md            # Architecture Decision Records (ADR-001..005)
```

---

## Prerequisites

- **Rust** (stable toolchain, edition 2021) + `cargo`
- **Node.js** 20+ & `npm`
- **Tauri CLI**: `cargo install tauri-cli --version "^2"`
- **Linux system libraries** (Ubuntu/Debian):
  ```bash
  sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev pkg-config
  ```

---

## Quick Start

### Run the Native Desktop App (Development)
```bash
cd desktop
npm install
cargo tauri dev
```

### Build a Release Desktop Bundle
```bash
cd desktop
npm install
cargo tauri build
```

### Run Headless Rust Tests & Quality Gates
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo clippy --manifest-path desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
```

### Build Frontend Only
```bash
cd desktop
npm run build
```

---

## Keyboard Shortcuts (In-Career)

| Key | Action |
|---|---|
| `Space` | Simulate next matchday (disabled during an active Live Match) |
| `1` – `9`, `0` | Switch between the 10 main workspace views |
| `S` | Save career to active slot |
| `L` | Toggle language (`ID` / `EN`) |
| `Esc` | Close open detail/result modals |

---

## Documentation

- [DECISIONS.md](DECISIONS.md) — Architecture Decision Records (ADRs)
- [CONTRIBUTING.md](CONTRIBUTING.md) — Development setup, coding standards, and PR guidelines
- [CHANGELOG.md](CHANGELOG.md) — Release history
- [SECURITY.md](SECURITY.md) — Vulnerability reporting policy
- [LICENSE](LICENSE) — MIT License
