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
| **Wave 1** | `naratama-sim` — MatchEngine, Formation, Lineup, TacticalDossier | `done` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` lolos (deterministik, Bug #7/#16/#24 & ADR-004 extra_time/neutral_venue) |
| **Wave 1** | `naratama-sim` — FinanceCalc, PrizeEngine, PlayerValuationService, Elo | `done` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` lolos (Bug #13 Confederation, broadcast sum exact, Hermite 19-pt, FIFA SUM Elo + KO protection) |
| **Wave 2** | `naratama-sim` + `naratama-engine` — CompetitionConfig, Scheduler (Circle + SA + Hierholzer + KO), SeasonGenerator, FixtureCalendar, FixtureValidator | `done` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace` lolos (20/18/12/10/8/ganjil liga, UCL/UECL 36-tim, karir Inggris + Piala + Blackout FIFA = 0 error, Bug #9/#17) |
| **Wave 3** | `naratama-db` + `naratama-engine` — MatchdayService, Housekeeping 18-step, FinanceService Ledger, Broadcast, Sponsors, Transfers/Loans/Windows, Scouting, Facilities/Staff/Academy, Suspensions, Offseason 7-step, Awards Gala, Custom Club | `done` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace` lolos (simulasi headless 5 musim penuh, `budget == SUM(ledger)` untuk 2.014 klub, 0 duplikat fixture, 12 skenario suspensi, Bug #1–#5/#8/#10–#12/#18/#19 fixed) |
| **Wave 4** | `naratama-engine` — InternationalCalendar, NationalTeamService, Selection, IntlSimulation, IntlTournaments (4-year cycle) | `done` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace` lolos (254 FIFA rankings seed, 23-pemain kuota, cap-tie permanen & eligibilitas ganda, siklus 4-tahunan WC 48-tim/EURO/Asian Cup/Nations League/FIFA ASEAN Cup/Friendlies + podium, Elo SUM + proteksi KO) |
| **Wave 5** | `naratama-engine` (NewsEngine, ClubLifeNews, ManagerOffers, Inbox, DesktopFacade) + `desktop/` (Tauri v2 Multi-Pane UI + i18n `id`/`en`) | `done` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace` (20/20 test lolos), `desktop/` `tsc --noEmit && vite build` lolos; native Tauri binary siap dijalankan setelah `libwebkit2gtk-4.1-dev` terpasang di Linux |

