# Changelog

All notable changes to **Naratama Gelanggang** are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-11-25

### Added
- **Core Rust Workspace**:
  - `naratama-model`: Strongly-typed IDs, domain models, competition/economy/international enums, and error types.
  - `naratama-sim`: Deterministic match engine (90', extra time, penalties, VAR, injuries), formation & tactical dossier engine, Circle/SA/Hierholzer fixture scheduler, Hermite player valuation, prize & broadcast calculators, and FIFA SUM Elo engine.
  - `naratama-db`: SQLite schema migrations, atomic 5-slot `SessionManager` with mid-career checkpoints and `.zip` export/import, and transactional club/finance/market/international repositories.
  - `naratama-engine`: Full career orchestration (`SeasonGenerator`, `MatchdayService`, `FinanceService`, `SquadTacticsService`, `TransferMarketService`, `OffseasonService`, `CustomClubService`, `NationalTeamService`, `InternationalSimulationService`, `NewsEngine`, `InboxService`, `DesktopFacade`).
- **Native Desktop Application (Tauri v2 + React/Vite/TypeScript)**:
  - Start Screen with 5 save slots, Export/Import archives, New Career Wizard, and Custom Club Creator with live SVG crest preview.
  - Multi-pane desktop shell with bilingual `ID` / `EN` localization and keyboard shortcuts.
  - Interactive 2D Tactics Board (6 formations, formation-aware Auto XI, click-to-swap, 5 training focuses) and Opponent Tactical Dossier.
  - Live Match panel (live minute progression, substitutions, mentality controls), Match Replay, and full-time Match Statistics modal.
  - Club Competitions (domestic leagues, domestic cups, country-relevant continental club cups with group tables & horizontal knockout brackets).
  - International Hub (254-nation FIFA World Rankings with flags & rank trends, FIFA windows, national team fixtures/groups/brackets, podiums, leaders, and call-up management).
  - Transfer Market (permanent & loan offers, incoming bids, scheduled transfers), Scouting Network & Youth Academy, Club Hub (facilities, staff, stadium expansion, ticket pricing, sponsors, cash flow), History & Honours, World News, Inbox, and 7-step Offseason Wizard.

