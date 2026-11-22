# Port Map — Naratama Gelanggang (Flutter/Dart → Rust Desktop)

> Dokumen pelacak status migrasi antar sesi. Referensi kode asli: `/home/alif/projects/Flutter/superkickoff`. Spesifikasi teknis: `README.md` & `01..08_*.md`.

## Gate Wajib Per Unit / Wave
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. Tanpa `unimplemented!()`, tanpa `unwrap()`/`expect()` di jalur error (gunakan `Result<_, NaratamaError>`).

## Status Wave & Crate

| Wave | Crate / Unit | Status | Catatan & Hasil Gate |
|---|---|---|---|
| **Wave 0** | `naratama-model` (IDs, Enums, Domain structs, Errors) | `done` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` lolos |
| **Wave 0** | `naratama-db` (SessionManager 5 slot, Migrations, ReferenceRepo, LogoService) | `done` | `wave0_session_and_reference_queries_work` lolos (254 negara, 2.014 klub, 55.624 pemain, rollback/commit `.session`, logo backfill, refresh power) |
| **Wave 1** | `naratama-sim` — MatchEngine, Formation, Lineup, TacticalDossier | `open` | Menunggu Wave 0 |
| **Wave 1** | `naratama-sim` — FinanceCalc, PrizeEngine, PlayerValuationService, Elo | `open` | Menunggu Wave 0 |
| **Wave 2** | `naratama-sim` + `naratama-engine` — CompetitionConfig, Scheduler (Circle + SA + Hierholzer + KO), SeasonGenerator, FixtureCalendar, FixtureValidator | `open` | Menunggu Wave 1 |
| **Wave 3** | `naratama-db` + `naratama-engine` — MatchdayService, Housekeeping 18-step, FinanceService Ledger, Broadcast, Sponsors, Transfers/Loans/Windows, Scouting, Facilities/Staff/Academy, Suspensions, Offseason 7-step, Awards Gala, Custom Club | `open` | Menunggu Wave 2 |
| **Wave 4** | `naratama-engine` — InternationalCalendar, NationalTeamService, Selection, IntlSimulation, IntlTournaments (4-year cycle) | `open` | Menunggu Wave 3 |
| **Wave 5** | `naratama-engine` (NewsEngine, ClubLifeNews, ManagerOffers, Inbox) + `naratama-desktop` (Tauri v2 Multi-Pane UI + i18n) | `open` | Menunggu Wave 4 (butuh `libwebkit2gtk-4.1-dev` di Linux) |

