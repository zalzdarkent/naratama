# 08 — Arsitektur Rust Desktop, Registri Bug Flutter Asli & Roadmap Rewrite

> Dokumen ini menjembatani seluruh spesifikasi domain (Dokumen 01–07) menjadi cetak biru teknis siap-eksekusi untuk membangun ulang **Naratama Gelanggang** di **Rust Desktop**.

---

## 1. Arsitektur Cargo Workspace yang Direkomendasikan

Memecah God Objects di Flutter (`CareerProvider` 3.258 baris & `CareerService` 4.437 baris) menjadi **5 crate terisolasi dengan batas tanggung jawab tegas**:

```text
naratama/
├── Cargo.toml                  # Workspace root
├── assets/
│   ├── data/
│   │   ├── superkickoff_2025.db          # Reuse langsung dataset 55.6K pemain!
│   │   ├── default_club_logos.json       # Atau konversi ke SQLite/pack biner .pak
│   │   └── default_league_logos.json
│   ├── flags/                            # Bundel lokal SVG/PNG 254 negara (+ gb-eng/sct/wls/nir)
│   └── i18n/                             # id.json, en.json
└── crates/
    ├── naratama-model/         # Pure structs, newtype IDs, enums, error types, serde (ZERO DB/IO)
    ├── naratama-sim/           # Pure deterministic simulation & math (ZERO DB/IO, blazing fast!)
    ├── naratama-db/            # rusqlite + WAL + migrasi + 5-slot session manager + Repositories
    ├── naratama-engine/        # 9 CQRS Services + State Dirty Events (pengganti CareerService/Provider)
    └── naratama-desktop/       # Desktop UI (Tauri v2 / Iced / egui / Slint)
```

### Tanggung Jawab Tiap Crate
1. **`naratama-model`:**
   - Newtype IDs (`TeamId`, `PlayerId`, `CountryId`, `TournamentId`, `SeasonId`, `MatchId`, `Money(i64)`), `chrono::NaiveDate`.
   - Seluruh domain model (`Player`, `Team`, `Stadium`, `FixtureMatch`, `KnockoutTie`, `CompetitionConfig`, `LedgerEntry`, `TransferOffer`, `InternationalWindow`, `CountryMatch`, `CustomClubSpec`, `SeasonGalaSummary`, dll.).
2. **`naratama-sim` (100% Pure Rust, Deterministik dengan `&mut impl Rng`):**
   - `match_engine`: simulasi menit 1–90, VAR, penalti, rating, MVP, possession, fast-sim Poisson.
   - `formation` & `lineup`: 6 formasi, `Lineup::best`, AI best formation, `tactical_dossier`.
   - `scheduler`: Circle Method, Simulated Annealing Home-Away Optimizer, League Phase (Hierholzer Eulerian), Group Snake, Knockout Bracket, `FixtureValidator`.
   - `finance_calc`: `EconomyScale`, elastisitas tiket, net matchday, broadcast 70/30, amortisasi, book value, sell-on, agent fee, `market_wage`, SCR/PSR.
   - `prize_engine`: 5 jenis kompetisi, 20 bobot posisi, bonus babak gugur.
   - `valuation`: Hermite cubic spline 19-titik OVR, 6 pengali, clean rounding, smooth `±15%`, asking/negotiated/loan fee.
   - `elo`: model FIFA SUM Elo + proteksi babak gugur.
   - *Keuntungan:* Ribuan property/unit test berjalan < 0.2 detik; simulasi 5 musim headless berjalan dalam hitungan detik!
3. **`naratama-db` (`rusqlite` dengan fitur `bundled`):**
   - `SessionManager`: 5 save slot (`slot_1..5.db`), atomic copy-on-start snapshot (`.session`) untuk Save / Don't Save, pragma `WAL + synchronous=NORMAL + temp_store=MEMORY`.
   - **Unified Transactional Write Gate:** `FinanceRepo::record_tx(tx: &Transaction, ...)` dipanggil di dalam transaksi repo lain (`TransferRepo`, `PrizeRepo`, `SponsorshipRepo`, `FacilitiesRepo`, `ScoutingRepo`, `SquadRepo`) sehingga **tidak ada satupun** `UPDATE teams SET budget` liar di luar ledger!
   - Migrasi idempoten eksplisit (pengganti `ensureLocalTables`), sekaligus menonaktifkan trigger implisit berbahaya (`t_clear_season*`, `t_delete_old_messages`) dan menggantinya dengan metode Rust eksplisit.
4. **`naratama-engine` (9 Layanan CQRS + Event Bus):**
   - `OnboardingService` (Wizard & Custom Club + Pre-Season Orientation)
   - `MatchdayService` (Live session, AI matchday, weekly housekeeping 18 langkah, injury & competition-scoped suspension orchestration)
   - `CalendarService` (`SeasonGenerator`, `FixtureCalendar` 8-pass conflict resolver, postponement, cup round advancement)
   - `SquadTacticsService` (Roster, lineup, training, facilities, staff, youth academy, contracts)
   - `TransferMarketService` (Transfer windows, offers, AI bidding wars, loans, scouting, scheduled transfers)
   - `ClubFinanceService` (Ledger, P&L, broadcast, sponsorships, prizes, board injections, FFP telemetry/optional toggle)
   - `CompetitionService` (Standings, brackets, leaderboards, season awards gala, Roll of Honour, promo/rel, offseason 7-tahap)
   - `InternationalService` (FIFA windows, eligibility/cap-tie, 23-man selection, friendly approval workflow, 4-year tournaments, Elo)
   - `NarrativeService` (`NewsEngine` 23 detektor, `ClubLifeNews`, `ManagerOfferService`, Inbox)
   - Memancarkan `DirtyFlags` (`CALENDAR | SQUAD | FINANCE | STANDINGS | MARKET | INTL | INBOX | OFFSEASON`) agar UI hanya me-reload panel yang berubah.

---

## 2. Evaluasi Pilihan Framework UI Desktop Rust

Game manajer sepak bola sangat didominasi oleh **tabel data padat (sortable/filterable), tab bersarang, kartu finansial, papan taktik 2D drag-and-drop, dan bagan gugur horizontal**.

| Opsi UI | Kelebihan untuk Naratama | Kekurangan | Rekomendasi |
|---|---|---|---|
| **Tauri v2 (Rust backend + React/Svelte/Solid + Tailwind)** | Ekosistem tabel (`TanStack Table`), grafik finansial, drag-and-drop taktik, animasi Gala/Confetti, SVG/Canvas, dan CSS layout desktop **paling matang & tercepat dibangun**. Semua engine & SQLite 100% native Rust (IPC super cepat). | Memakai system WebView (~15–30 MB binary, RAM sedikit lebih tinggi dari native murni). | **🥇 Sangat Direkomendasikan** untuk kecepatan produksi & kualitas UI setara Football Manager modern |
| **Iced (Pure Rust)** | 100% Rust, Elm architecture cocok dgn CQRS, GPU-accelerated, ringan. | Widget `DataTable` padat, drag-and-drop kompleks, dan teks multi-gaya butuh banyak kode custom. | 🥈 Pilihan terbaik jika wajib 100% pure Rust |
| **egui (`eframe`, Pure Rust)** | Sangat cepat untuk prototipe dashboard padat, tabel, plot finansial, & tool-style FM klasik. | Estetika modern (gradient, animasi halus, tipografi elegan) lebih sulit dipoles dibanding web/Slint. | 🥉 Cocok untuk MVP / prototipe cepat pure Rust |
| **Slint (Rust + `.slint` DSL)** | Native ringan, desain deklaratif rapi, performa tinggi. | Ekosistem tabel data raksasa & chart masih terbatas; lisensi komersial perlu diperhatikan untuk distro tertutup. | Alternatif menarik untuk UI native bersih |

> **Kunci Arsitektur:** Karena `naratama-model`, `naratama-sim`, `naratama-db`, dan `naratama-engine` 100% independen dari UI, kamu bisa membangun & memverifikasi **seluruh otak game + CLI headless 5-musim terlebih dahulu**, lalu memasang UI Desktop (Tauri atau Iced/egui) di atasnya tanpa mengubah satu baris pun logika game!

---

## 3. Registri 26 Bug & Quirk di Kode Flutter Asli (Wajib Diketahui Sebelum Rewrite!)

Hasil audit baris-per-baris terhadap seluruh 85 file Dart menemukan **26 bug nyata & quirk desain**. Jangan sampai tersalin tanpa sadar ke Rust!

### 3.1 Bug Kritis — WAJIB Diperbaiki di Rust
| # | Lokasi Flutter Asli | Bug / Masalah | Perbaikan di Rust |
|---:|---|---|---|
| 1 | `career_provider.dart:2462` (`makeTransferOffer`) | Saat user **membeli** pemain, mengecek `squadCount(userTeamId) <= 19` alih-alih mengecek `sellerTeamId`! | Cek `squad_count(seller_id) > MIN_SQUAD_SIZE` saat menjual/membeli (pakai newtype `SellerTeamId` vs `BuyerTeamId`). |
| 2 | `finance_repository.dart` (`upsertClubState`) | Memakai `INSERT OR REPLACE` dgn kolom parsial sehingga memanggil update `distress` mereset `consecutive_breaches`, `embargo_until_season`, `last_injection_season` ke 0/NULL! | Gunakan `INSERT ... ON CONFLICT(team_id) DO UPDATE SET ...` yang mempertahankan kolom tak disebut. |
| 3 | `scouting_repository.dart:183-188` | `countryPool` South America (`[24,8,41,46]`), Africa (`[4,6,42,60]`), dan Asia (`[97,102,103,105]`) memakai **ID negara salah** (di DB asli: Brasil=30, Argentina=11, Indonesia=94, Jepang=124, Korsel=49, Australia=14)! | Query negara secara dinamis berdasarkan `countries.confederation` atau verifikasi ID terhadap DB. |
| 4 | `international_simulation_service.dart:260` | Tanggal sembuh cedera timnas dihitung dari `DateTime.now()` (jam OS) alih-alih `currentMatch.date`! | **Selalu** terima `game_date: NaiveDate` di semua service; larang pemanggilan `Utc::now()` untuk tanggal game. |
| 5 | `sponsorship_repository.dart`, `scouting_repository.dart`, `transfer_repository.dart`, `career_service.dart:advanceTransferWindow` | Sama: menulis tanggal transaksi ledger/kontrak/AI market menggunakan jam dinding OS (`_todayIso()`) alih-alih tanggal simulasi in-game. | Teruskan `current_date: NaiveDate` sebagai parameter wajib. |
| 6 | `international_simulation_service.dart` + `national_team_service.dart` + trigger `t_clear_season_countries` | **Double-counting statistik timnas & salah skala rating:** Menulis ke `match_players_countries` DAN `player_season_stats_countries` (rata-rata), lalu query menjumlah keduanya dan membagi `rating / matches_played` lagi! Trigger arsip juga memakai `p.country_id` bukan `mpc.country_id`. | Tulis laga aktif hanya ke `match_players_countries`; agregasikan sekali ke `player_season_stats_countries` (simpan `rating_sum` konsisten, `GROUP BY mpc.country_id`) saat arsip musim. |
| 7 | `match_engine.dart` (final rating) | `home.substitutes` dimutasi saat substitusi, sehingga pemain pengganti tim kandang gagal cek `isHomePlayer` dan mendapat `awayOutcome` (menang/kalah terbalik) untuk rating & poin! | Cek `row.team_id == input.home_team_id` langsung. |
| 8 | `season_repository.dart` (`seasonGalaAwards`) | Manager of the Year memakai `COALESCE((SELECT name FROM manager_profile), c.name)` sehingga nama user selalu tampil walau klub AI yang menang! | Gunakan `manager_profile.name` **hanya** jika `pa.team_id == user_team_id`. |
| 9 | `season_repository.dart` (`standings`) | Tanpa `groupId`, mengagregasi semua fase (`phase >= 0`). Pada kompetisi `leaguePhase`, begitu babak gugur berjalan, laga gugur ikut mengotori klasemen fase liga! | Filter `AND phase = 0` untuk klasemen `leaguePhase` / `groupStage`. |
| 10 | `scouting_repository.dart` (`hireScout`, `signWonderkid`) & `squad_repository.dart` (`buyMarketPlayer`) | Menulis beberapa baris `finance_ledger` dengan `season_id = NULL` (terlewat dari laporan P&L musim & PSR), dan `signWonderkid` tidak mengecek `maxSquadSize = 30` serta tidak membuat `player_registrations`. | Isi `season_id` wajib di semua entri ledger operasional, cek `maxSquadSize`, buat `player_registrations`. |
| 11 | `transfer_repository.dart` (`completeLoan`) | Tidak memvalidasi `borrowerBudget >= loanFee` maupun `borrowerSquadSize < 30` di dalam transaksi. | Tambahkan validasi saldo & kuota skuad di `complete_loan`. |
| 12 | `career_service.dart` (`_applyCountryPromotionRelegation`) | Cabang fallback promosi bila divisi bawah tidak disimulasikan menjadi *dead code* karena `min(..., lowerTable?.len ?? 0)` selalu `0`. | Hitung `swap_count` terhadap `table.len()` ketika divisi bawah tidak punya tabel. |
| 13 | `prize_engine.dart` (`EconomyScale.confederation`) vs `countries` DB | Kode konfederasi `2` (CONMEBOL) dan `4` (CAF) tertukar komentarnya di `EconomyScale`, `PlayerValuationService`, dan `NewsEngine`. | Satu enum `Confederation` tunggal di `naratama-model` yang cocok 100% dengan tabel `countries`. |
| 14 | `international_tournament_service.dart` (`scheduleGroupStage`) | Modulo `(dateIdx + p) % dates.len()` pada grup 4 tim over 3 hari dapat menempatkan 1 tim bermain 2× di tanggal yang sama. | Pakai Circle Method 3-ronde standar untuk setiap grup 4 tim. |
| 15 | `national_team_selection_service.dart` | Tidak memfilter pemain yang sudah `is_locked = 1` ke negara lain jika `players.country_id` masih menunjuk negara ini; `replaceCalledUpPlayer` lupa hapus dari `country_callups`; negara `< 23` pemain men-trigger seleksi ulang & inbox duplikat tiap refresh. | Filter `NOT EXISTS (... is_locked = 1 AND country_id != ?)`; hapus dari `country_callups` saat withdraw; cek `existing_duties.is_empty()` bukan `< squad_size`. |
| 16 | `match_engine.dart` (`penaltyShootout`) | Rumus `kicksTaken = isHome ? round : round - 1` terbalik pada tendangan tim tamu dalam cek terminasi dini best-of-5. | Hitung `home_rem = 5 - home_taken` dan `away_rem = 5 - away_taken` terpisah secara eksplisit. |
| 17 | `season_generator.dart` (Pre-season friendlies vs Super Cup) | `schedulePreSeasonFriendlies` dijalankan setelah `resolveFixtureClashes`, sehingga bisa bentrok rest-period dengan UEFA/AFC Super Cup (`H-4` vs `H-3`). | Jalankan `resolve_season_conflicts` setelah semua friendly masuk. |
| 18 | `career_service.dart` (`_lastHousekeepingDate`) | Disimpan di memori saja (hilang saat reload), menyebabkan notifikasi pembukaan/penutupan jendela transfer terlewat pada matchday pertama setelah load save. | Simpan `last_housekeeping_date` di tabel `career_state`. |
| 19 | `app_database.dart` (`t_delete_old_messages` & `t_clear_season`) | Trigger 20 pesan dapat menghapus surat tawaran transfer/manajer yang belum dibaca; trigger `t_clear_season` terpicu N kali per musim. | Pindahkan arsip musim ke 1 transaksi Rust per musim; simpan 100+ pesan atau lindungi pesan `unread/actionable`. |
| 20 | `news_engine.dart` (`drawfest`) | Cabang `hg == ag && hg >= 3` tidak pernah terjangkau karena sudah tertangkap cabang `thriller` (`hg + ag >= 6`). | Cek `drawfest` sebelum `thriller` atau gabungkan dengan template khusus. |
| 21 | `season_repository.dart` (`clubTrophies`) | Tidak memfilter `leg = MAX(leg)` pada final 2-leg, berisiko mengembalikan pemenang Leg 1 alih-alih juara agregat Leg 2. | Filter `ORDER BY phase DESC, leg DESC`. |

### 3.2 Quirk & Keputusan Desain untuk Dikonfigurasi di Rust
| # | Kondisi di Flutter Asli | Rekomendasi untuk Rust Desktop |
|---:|---|---|
| 22 | **FFP Sanctions dinonaktifkan** (`evaluateSeason` & `canRegister` return `null`), namun rumus SCR/PSR lengkap. | Jadikan opsi saat buat karier (`FFP: Off [default] / Relaxed / Standard / Strict`). |
| 23 | **Tidak ada Extra Time 30 menit** di knockout (langsung penalti) & **tidak ada Neutral Venue** di `MatchEngine` (`+2.0` home selalu aktif). | Tambahkan dukungan `neutral_venue: bool` (menghilangkan `+2.0/-1.2` di Final & Piala Dunia) dan simulasi Extra Time menit `91..=120` jika `config.extra_time == true`. |
| 24 | **`position_id == 8` (WM)** adalah `MID` di DB & AI academy, tapi `ATT` di `Lineup.groupOf` & `TrainingFocus`. | Standarkan definisi `PositionGroup` di satu fungsi tunggal pada `naratama-model`. |
| 25 | **Intercontinental Cup (`56`)** punya 5 juara konfederasi lalu dipangkas `seedByIdes` jadi 4 (membuang juara ke-5), dan **League Phase** memakai irisan Circle Method (bukan 4-pot Swiss). | Biarkan `KnockoutScheduler(use_byes: true)` menangani 5/6 juara konfederasi (dengan babak awal) dan pertimbangkan opsi Swiss 4-pot sejati untuk UCL/UEL/UECL. |
| 26 | **Tidak ada `morale`, tidak ada `firing`, tidak ada live sub di `MatchScreen`, dan `SponsorshipRepository` tidak memakai `sponsorRevenueScale = 1.50` / `FinanceSummary` tidak menampilkan `board_funding`/`loan_fee_received`/`sell_on_paid`.** | Pertahankan paritas MVP di Fase 1–4, lalu buka sebagai fitur ekspansi Desktop v2 (Live Tactical Subs & Mentality, Board Confidence, Copa America/AFCON/Gold Cup otomatis di siklus 4-tahunan). |

---

## 4. Roadmap Implementasi Rewrite ke Rust (Fase 0 – Fase 6)

| Fase | Target Crate | Lingkup Implementasi | Gerbang Verifikasi (Acceptance Tests) |
|---|---|---|---|
| **Fase 0 — Fondasi & DB** | `naratama-model`, `naratama-db` | Workspace Cargo, newtype IDs, loader `superkickoff_2025.db` via `rusqlite` WAL, 5 Save Slots + `.session` snapshot, migrasi & logo/flag loader lokal. | Buka DB bawaan → validasi 254 negara, 2.014 klub, 55.624 pemain, 228 turnamen, round-trip save/rollback 5 slot. |
| **Fase 1 — Pure Sim Core** | `naratama-sim` | `MatchEngine` (full + quick Poisson + penalti + VAR), `Formation` & `Lineup.best`, `FinanceCalc`, `PrizeEngine`, `PlayerValuationService` (Hermite 19-titik), FIFA SUM Elo. | Property & unit tests deterministik (seeded RNG): rata-rata gol realistis, tidak ada MV > $250M, broadcast sum == pool eksak, Elo knockout protection. |
| **Fase 2 — Kompetisi & Kalender** | `naratama-sim`, `naratama-engine` | `CompetitionConfig` (24 profil), Circle Method + Simulated Annealing Optimizer, League Phase (Hierholzer), Knockout Bracket, `SeasonGenerator`, `FixtureCalendar` 8-pass, `FixtureValidator`. | Uji liga 20/18/12/10/8 & ganjil (`validateSeason` = 0 error), UCL 36-tim 4H/4A, 0 bentrok klub dengan 3 jendela FIFA. |
| **Fase 3 — Career Loop, Ekonomi & Transfer** | `naratama-engine`, `naratama-db` | `MatchdayService`, 18-step Weekly Housekeeping, `FinanceService` (transactional ledger), Broadcast, Sponsor AI, Transfer Windows, Offers/Loans/Scheduled, Scouting, Facilities/Staff, Training, Academy, Competition-Scoped Suspensions, Offseason 7-tahap, Awards Gala, Custom Club. | **Headless 5-Season Simulation Test:** jalankan 5 musim penuh otomatis; cek `budget == SUM(ledger)` untuk semua 2.014 klub, 0 duplikasi laga, promosi/degradasi akurat, seluruh 12 skenario skorsing kartu merah lulus! |
| **Fase 4 — Sepak Bola Internasional** | `naratama-engine` | Kalender 3 jendela, Eligibilitas & Cap-tie, Seleksi 23 pemain + approval uji coba, Simulasi Timnas + Elo, Siklus 4-tahunan (WC 48, EURO, Asian Cup, Nations League, FIFA ASEAN Cup) + 3rd Place & Podium. | **5-Season International Cycle Test:** semua turnamen mencapai final & podium 3 negara unik, 0 bentrok klub, 0 double-counting statistik timnas. |
| **Fase 5 — Narasi & Desktop UI MVP** | `naratama-desktop` | `NewsEngine` (23 detektor), `ClubLifeNews`, `ManagerOfferService`, Inbox, serta antarmuka Desktop Multi-Pane (Sidebar, Top Bar, Dashboard, Two-Pane Tactics, Master-Detail Squad/Market/Standings, Split Broadcast Match View, Club Hub, International Hub, Offseason & Gala). | Uji end-to-end seluruh alur user dari Onboarding / Custom Club → 1 Musim Penuh → Offseason → Musim 2 pada resolusi 1280×720 hingga 2560×1440. |
| **Fase 6 — Polish & Desktop Enhancements** | `naratama-desktop`, `naratama-engine` | i18n bersih (`id`/`en`), keyboard shortcuts, ekspor/impor save `.zip`, optimasi build binary rilis Linux/Windows/macOS, serta fitur opsional v2 (interactive live match subs/mentality, Copa/AFCON/Gold Cup otomatis, FFP strict toggle). | Profiling performa (< 200ms per matchday penuh, < 2s generate musim baru), zero memory leak pada LRU logo cache. |

