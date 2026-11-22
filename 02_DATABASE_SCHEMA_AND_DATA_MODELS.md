# 02 — Skema Database SQLite, Data Referensi, Trigger & Model Domain Rust

> **Sumber terverifikasi:** `assets/data/superkickoff_2025.db` (DDL lengkap 77 tabel), `lib/data/db/schema.dart`, `lib/data/db/app_database.dart` (1.482 baris), `lib/data/models/{models,international_models,roll_of_honour_models,save_slot_info}.dart`, `lib/data/repositories/{reference_repository,club_logo_service,league_logo_service}.dart`, `lib/core/country_names.dart`.

---

## 1. Aset Data & Arsitektur Penyimpanan

Game memakai **satu file SQLite per Save Slot** yang menyalin database referensi bawaan (`assets/data/superkickoff_2025.db`, ~54 MB) saat karier baru dibuat. File yang sama bertindak sebagai data referensi sekaligus penyimpanan progres karier.

| Aset | Ukuran | Isi |
|---|---:|---|
| `assets/data/superkickoff_2025.db` | 54 MB | 77 tabel (data referensi penuh + tabel karier kosong) |
| `assets/data/default_club_logos.json` | 43 MB | 2.014 pasangan `team_id -> logo_base64` (PNG/WebP) |
| `assets/data/default_league_logos.json` | 301 KB | 18 pasangan `tournament_id -> logo_base64` |
| `assets/img/logo.png` | 2.9 MB | Logo resmi Naratama Gelanggang |

### Konfigurasi Multi-Save Slot & SQLite Pragmas (`AppDatabase`)
- **5 Save Slots:** Nama file `superkickoff_2025_v3_slot_{1..5}.db`.
- **Snapshot Sesi (Save / Don't Save):** `beginSession()` menyalin file DB slot ke `<slot_path>.session`. Jika user keluar dan memilih *Don't Save*, file `.session` disalin balik menimpa DB utama.
- **SQLite Pragmas Desktop (WAJIB dipertahankan di Rust `rusqlite`):**
  ```sql
  PRAGMA journal_mode = WAL;
  PRAGMA synchronous = NORMAL;
  PRAGMA temp_store = MEMORY;
  ```
- **Inisialisasi/Migrasi Otomatis Saat Buka DB (`ensureLocalTables`):**
  1. Pastikan seluruh tabel lokal & kolom tambahan (`players.injury_return_date`, `suspensions.competition_type`, `stadiums.ticket_price_*`, `matches.{kickoff_time,original_date,reschedule_reason,neutral_venue,extra_time,away_goals_rule}`, `countries.previous_rank`, `teams.logo_base64`, `tournaments.logo_base64`, dll.).
  2. **Backfill budget klub nol:**
     ```sql
     UPDATE teams SET budget = CASE
       WHEN power <= 40 THEN 500000
       ELSE MAX(500000, (power - 40) * (power - 40) * 40000)
     END WHERE COALESCE(budget, 0) <= 0;
     ```
  3. Seed turnamen tambahan: `9999` (*Pre-Season Friendly*), `1650` (*AFC Challenge League*), `1651` (*AFC Super Cup*), `900`–`904` (turnamen timnas), serta rename Liga Indonesia (`85` BRI Super League, `86` Pegadaian Championship, `189` Liga Nusantara).
  4. Seed kuota `international_slots` AFC (`99`, `164`, `1650`) dan hapus super/global cup (`54, 56, 163, 1651`) dari `international_slots`.
  5. Seed 254 poin FIFA awal (`kInitialFifaPoints`) jika `MAX(countries.points) <= 0`.
  6. Seed logo klub & liga dari JSON bundel, normalisasi singkatan klub Indonesia (`Per` → `PSB`, `PSJ`, dll.), normalisasi nama negara Spanyol → Indonesia, dan seed 53 merek sponsor global (`Fly Emirates`, `Spotify`, `Nike`, `Adidas`, dll.).
  7. Hitung ulang semua `teams.power` & `countries.power`.

---

## 2. Data Master Referensi & Konstanta Enum

### 2.1 Divisi (`divisions`, 5 baris)
`1`: First Division | `2`: Second Division | `3`: Third Division | `4`: Fourth Division | `0`: No Division (klub terdegradasi keluar piramida/digantikan Custom Club).

### 2.2 Posisi (`positions`, 11 baris)
| `position_id` | `abbreviation` | Nama | DB `type` | `PositionBadge` (Kanan `'D'` / Kiri `'I'`) | Grup Match Engine (`Lineup.groupOf`) |
|---:|---|---|---:|---|---:|
| 1 | `GK` | Goalkeeper | 1 | `GK` | 1 (GK) |
| 2 | `CB` | Centre Back | 2 | `CB` | 2 (DEF) |
| 3 | `CB` | Sweeper Centre Back | 2 | `CB` | 2 (DEF) |
| 4 | `FB` | Full Back | 2 | `RB` / `LB` | 2 (DEF) |
| 5 | `DM` | Defensive Midfielder | 3 | `CDM` | 3 (MID) |
| 6 | `AM` | Attacking Midfielder | 3 | `CAM` | 3 (MID) |
| 7 | `CM` | Central Midfielder | 3 | `CM` | 3 (MID) |
| **8** | **`WM`** | **Wide Midfielder** | **3 (MID)** | `RW` / `LW` | **4 (ATT)** *(perbedaan!)* |
| 9 | `W` | Winger Forward | 4 | `RW` / `LW` | 4 (ATT) |
| 10 | `ST` | Striker | 4 | `ST` | 4 (ATT) |
| 11 | `SS` | Second Striker | 4 | `CF` | 4 (ATT) |

### 2.3 Konfederasi (`countries.confederation`)
| Kode | Konfederasi | Jumlah Negara di DB | Rentang Power |
|---:|---|---:|---|
| `0` | Unaffiliated / Teritori | 40 | 0 |
| `1` | **CONCACAF** | 38 | 40–83 |
| `2` | **CONMEBOL** | 10 | 73–91 |
| `3` | **UEFA** | 56 | 40–90 |
| `4` | **CAF** | 53 | 50–82 |
| `5` | **AFC** | 45 | 50–83 |
| `6` | **OFC** | 12 | 35–67 |

> **Peringatan Inkonsistensi Kode Konfederasi di Kode Flutter Asli:**  
> Di database SQLite dan `country_names.dart`: **`2 = CONMEBOL`, `4 = CAF`**.  
> Tetapi di `EconomyScale.confederation` (`prize_engine.dart`) dan `NewsEngine._confederations`, komentarnya menganggap `2 = Africa/CAF` dan `4 = South America/CONMEBOL`! Di Rust, **jadikan satu enum `Confederation` tunggal** yang langsung sesuai dengan data asli DB (`1=Concacaf, 2=Conmebol, 3=Uefa, 4=Caf, 5=Afc, 6=Ofc`).

### 2.4 Master Skills (25), Tipe Wasit (5), Status & Enum (`schema.dart`)
- **25 Skills (`skills`):** 1 Diving, 2 Reflexes, 3 Anticipation, 4 Aerial Play, 5 Positioning, 6 Tackling, 7 Marking, 8 Ball Recovery, 9 Finishing, 10 Heading, 11 Strength, 12 Physique, 13 Speed, 14 Agility, 15 Creativity, 16 Passing, 17 Stamina, 18 Free Kicks, 19 Dribbling, 20 Technique, 21 Leadership, 22 Aggression, 23 Intelligence, 24 Ball Control, 25 Movement. *(Catatan: tabel `player_skills` di DB awal memiliki 0 baris; simulasi saat ini mengandalkan `players.power`, `potential`, `fitness`, dan `position_id`.)*
- **5 Tipe Wasit (`referee_types`):** `1` Lenient (0.8× kartu), `2` Fair (1.0×), `3` Unsure (1.2×), `4` Bipolar (1.4×), `5` Grumpy (1.6×).
- **Status Pemain (`players.status_id`):** `1` Active, `2` Retired, `3` Dead.
- **Status Skuad (`players.squad_status`):** `0` Reserve, `1` First Team, `2` Loan Listed.
- **Status Bursa (`players.market_status`):** `1` Active (Normal), `2` For Sale, `3` On Loan.
- **Organisasi Turnamen (`tournaments.organization`):** `1` Domestic League, `2` Continental/International/Super Cup, `3` National Cup, `4` Friendly.
- **Status Laga (`matches.status`):** `1` Scheduled, `2` Finished, `3` Cancelled.
- **Status Penawaran Transfer (`transfer_offers.status`):** `1` Pending, `2` Accepted, `3` Rejected, `4` Countered, `5` ContractNegotiation, `6` PlayerAccepted, `7` PlayerRejected, `8` Completed, `9` AwaitingWindow (Scheduled), `10` Cancelled, `11` Failed.
- **Tipe Penghargaan (`player_awards.type`):** `1` Golden Boot, `2` MVP, `3` Best Young Player, `4` Ballon d'Or, `5` Golden Glove, `6` Team of the Year, `7` Manager of the Year.

---

## 3. Katalog Lengkap 77+ Tabel SQLite

### 3.1 Referensi Dunia & Entitas Utama
| Tabel | Primary Key | Kolom Kunci & Catatan |
|---|---|---|
| `countries` | `country_id` | `name`, `iso2`, `power`, `points` (Elo FIFA), `confederation`, `previous_rank` |
| `divisions` | `division_id` | `name` (5 baris) |
| `teams` | `team_id` | `abbreviation`, `name`, `founded_year`, `value`, `power`, `budget` (cached cash), `points`, `stadium_id`, `country_id`, `coach_id`, `division`, `logo_base64` |
| `coaches` | `coach_id` | `name`, `age`, `country_id`, `points` |
| `stadiums` | `stadium_id` | `name`, `city`, `country_id`, `capacity_{general,stand,terrace,boxes}`, `max_{general,stand,terrace,boxes}`, `ticket_price_{general,stand,terrace,boxes}` |
| `players` | `player_id` | `name`, `age`, `height`, `stars`, `value` (MV), `salary` (weekly USD), `profile` (`'D'`/`'I'`), `experience`, `fitness` (0–100), `status_id`, `power`, `potential`, `market_status`, `country_id`, `position_id`, `squad_status`, `is_captain`, `shirt_number`, `championships`, `points`, `skill_a`, `skill_b`, `injury_return_date` |
| `team_players` | *(no PK, indexed)* | `team_id`, `player_id` (relasi kepemilikan/pinjaman aktif saat ini) |
| `positions`, `skills`, `position_skills`, `player_skills`, `player_statuses` | respective IDs | Master atribut & posisi |
| `referees`, `referee_types` | `referee_id` / `type_id` | Master wasit (`referees` kosong di base DB → fallback `Fair`) |
| `tournaments` | `tournament_id` | `name`, `division_id`, `status`, `organization`, `rounds`, `reversed`, `date_1/2`, `label_1/2`, `play_days`, `slots`, `points`, `promotion_slots`, `relegation_spots`, `prize_{teams,top_scorer,mvp}`, `logo_base64` |
| `tournament_countries` | *(indexed)* | `country_id`, `tournament_id`, `slots` |
| `international_slots` | `UNIQUE(tournament_id, country_id)` | `tournament_id`, `country_id`, `slots` (kuota kontinental per negara) |
| `groups` | `group_id` | `name` (35 label grup A..Z+) |

### 3.2 Karier, Sesi, Taktik, Latihan, Fasilitas & Akademi
| Tabel | Primary Key | Kolom Kunci & Catatan |
|---|---|---|
| `manager_profile` | `id CHECK(id=1)` | `name`, `country_id` |
| `career_state` | `season_id` | `orientation_completed` (0/1), `pre_season_date` |
| `offseason_state` | `id CHECK(id=1)` | `season_id`, `step` (0..6), `report`, `window_weeks`, `camp`, `ambition` |
| `board_objectives` | `(start_year, team_id)` | `target_rank`, `bonus`, `status` (`'pending'|'met'|'failed'`) |
| `career_history` | *(no PK)* | `season_id`, `coach_id`, `team_id`, `country_id`, `matches_played`, `wins`, `draws`, `losses`, `goals_for`, `goals_against`, `start_year`, `end_year`, `championships` |
| `team_tactics` | `team_id` | `formation` (default `'4-4-2'`) |
| `team_lineups` | `(team_id, slot)` | `player_id` (`slot` 0..10) |
| `team_training` | `team_id` | `season_id`, `last_match_count`, `sessions`, `done_focuses` (CSV) |
| `team_facilities` | `team_id` | `training_level` (1..5), `medical_level` (1..5) |
| `team_staff` | `staff_id`, `UNIQUE(team_id, role)` | `role` (`head_physio|tactical_analyst|head_scout`), `name`, `stars` (1..5), `salary`, `specialty`, `hired_date` |
| `team_youth_academy` | `team_id` | `level` (1..10), `last_graduated_season`, `last_graduated_date` |
| `academy_prospects` | `prospect_id` | `team_id`, `name`, `age`, `position_id`, `power`, `potential`, `stars`, `profile`, `height`, `country_id`, `created_date`, `signed` |
| `manager_offers` | `offer_id` | `team_id`, `season_id`, `date`, `status` (0..4), `club_name`, `league_name`, `target`, `contract_years`, `budget`, `power`, `from_power` |

### 3.3 Kompetisi, Jadwal, Pertandingan & Statistik
| Tabel | Primary Key / Unique | Kolom Kunci & Catatan |
|---|---|---|
| `seasons` | `season_id` | `name`, `start_year`, `end_year`, `is_new_format` |
| `season_tournaments` | *(indexed)* | `season_id`, `tournament_id`, `status` (1 aktif / 0 arsip — **memicu trigger arsip!**) |
| `season_teams` | *(indexed)* | `team_id`, `season_id`, `coach_id`, `position` (**memicu trigger jual/pinjam AI!**) |
| `competition_configs` | `(tournament_id, season_start_year)` | 30 kolom aturan format kompetisi (lihat Dokumen 04) |
| `matches` | `match_id`, `UNIQUE ux_matches_fixture` | `tournament_id`, `season_id`, `referee_id`, `stadium_id`, `home_team_id`, `away_team_id`, `home_goals`, `away_goals`, `winner_id`, `attendance_*`, `revenue`, `phase`, `group_id`, `round`, `status`, `leg`, `date`, `penalty_{home,away}_goals`, `kickoff_time`, `original_date`, `reschedule_reason`, `neutral_venue`, `extra_time`, `away_goals_rule` |
| `match_incidents` | `incident_id` | `match_id`, `data` (JSON array of `MatchEvent`) |
| `match_players` | *(indexed)* | `player_id`, `match_id`, `team_id`, `starter`, `minutes`, `points`, `rating` (×10), `mvp`, `goals`, `yellow_cards`, `red_cards`, `assists` |
| `player_season_stats` | *(indexed)* | Arsip agregasi musim per `(player_id, team_id, season_id)` (`rating` = SUM) |
| `suspensions` | `suspension_id` | `player_id`, `type` (2=red), `matches_remaining`, `season_id`, `competition_type` |
| `knockout_byes` | `(season_id, tournament_id, phase, team_id)` | Klub penerima bye di babak gugur |
| `fixture_reschedules` | `id` | `match_id`, `old_date`, `new_date`, `reason`, `created_at` |
| `player_awards` | `award_id` | `player_id`, `team_id`, `type` (1..7), `year`, `season_id`, `goals`, `rating`, `stat_subtitle`, `position_id` |
| `world_ranking_history` | *(no PK)* | `team_id`, `year`, `position` |

### 3.4 Keuangan, Sponsor, Hadiah, Transfer & Scouting
| Tabel | Primary Key / Unique | Kolom Kunci & Catatan |
|---|---|---|
| `finance_ledger` | `ledger_id` | `team_id`, `season_id`, `entry_date`, `period`, `category`, `subcategory`, `amount` (signed), `ref_type`, `ref_id`, `description`, `created_at` |
| `finance_settings` | `id CHECK(id=1)` | `mode`, `completed_seasons`, `initialized`, `opening_balance_date`, `mode_locked`, `last_completed_season`, `updated_at` |
| `finance_club_state` | `team_id` | `consecutive_breaches`, `distress`, `embargo_until_season`, `last_injection_season`, `last_ffp_level`, `updated_at` |
| `player_registrations` | `registration_id` | `player_id`, `team_id`, `from_team_id`, `transfer_fee`, `agent_fee`, `sign_on_bonus`, `start_date`, `end_date`, `annual_amortization`, `release_clause`, `status` |
| `transfer_clauses` | `clause_id` | `player_id`, `beneficiary_team_id`, `type` (`sell_on|release_clause`), `pct`, `amount`, `active`, `created_date`, `settled_date` |
| `contracts` | `contract_id` | `player_id`, `team_id`, `start_date`, `end_date`, `status`, `salary`, `{goal,assist,mvp,title,appearance}_bonus` |
| `sponsors` & `sponsorships` | `sponsor_id` / `sponsorship_id` | Master merek + kontrak 3 slot (`base_reward`=upfront, `progress`=weekly, `is_unique_prize`=targetBonus) |
| `competition_prize_rules` & `competition_prize_awards` | `tournament_id` / `UNIQUE(season,tour,team)` | Override hadiah + catatan payout idempoten |
| `transfer_windows` | `UNIQUE(season,country,tour,window_type)` | `start_date`, `end_date`, `allow_free_agents_outside` |
| `transfer_offers` | `offer_id` | `player_id`, `from_team_id`, `to_team_id`, `offer_amount`, `player_value`, `status` (1..11), `counter_amount`, `proposed_salary`, `proposed_duration_years`, `is_loan`, `loan_seasons`, `agreement/effective/eligible_date`, `transfer_type`, `loan_months`, `wage_split_ratio`, `loan_start/end_date`, `cancellation_reason` |
| `player_transfers` & `coach_transfers` | `transfer_id` | Riwayat transfer permanen (`type=1`) |
| `team_loans` | `loan_id` | `player_id`, `from_team_id`, `to_team_id`, `date_from`, `date_to`, `months`, `notified` |
| `scouts` & `scout_reports` | `scout_id` / `report_id` | Maks 4 scout aktif + laporan wonderkid belum direkrut (`signed=0`) |

### 3.5 Sepak Bola Internasional & Narasi
| Tabel | Primary Key / Unique | Kolom Kunci & Catatan |
|---|---|---|
| `international_windows` | `window_id` | `season_id`, `name`, `window_type`, `start_date`, `end_date`, `release_start_date`, `return_date`, `is_mandatory`, `max_matches`, `club_fixtures_allowed` |
| `international_duty` | `UNIQUE(season_id, player_id, window_id)` | `country_id`, `release_date`, `return_date`, `squad_number`, `is_captain`, `status` (`active|pending_release|rejected|withdrawn|returned`) |
| `player_national_eligibility` | `(player_id, country_id)` | `is_primary`, `is_locked` (cap-tie), `notes` |
| `country_matches` | `match_id` | Identik `matches` tetapi pakai `home_country_id`, `away_country_id`, plus `window_id` |
| `country_callups` | `(season_id, player_id, country_id)` | Skuad panggilan timnas musim aktif |
| `match_players_countries` & `player_season_stats_countries` | `PK(player,country,season)` | Statistik pertandingan & arsip musim internasional |
| `country_coaches`, `season_countries` | composite PK | Relasi pelatih/musim timnas (saat ini belum aktif ditulis) |
| `news` | `news_id`, `UNIQUE(ref)` | `season_id`, `date`, `scope`, `category`, `headline`, `body`, `importance`, `team_id`, `player_id` |
| `messages` | `message_id` | `title`, `body`, `date`, `sender`, `is_read`, `type`, `player_id`, `team_id`, `transfer_offer_id` |

*(Tabel legacy/cadangan di base DB yang tidak ditulis kode aktif saat ini: `android_metadata`, `board_injections`, `commercial_deals`, `finance_accounts`, `finance_ffp_assessments`, `finance_staff`, `friendlies`, `game_save_metadata`, `manager_milestones`, `player_training_focus`, `scout_assignments`, `tactical_plans` — biarkan aman atau bersihkan di migrasi Rust.)*

---

## 4. Trigger & View SQLite Bawaan (Perhatian Khusus Rewrite Rust!)

Database membawa 1 View (`v_countries`) dan **5 Trigger SQLite aktif**:
1. **`t_clear_season`** (`BEFORE UPDATE OF status ON season_tournaments WHEN NEW.status = 0`): Mengagregasi `match_players` → `player_season_stats`, lalu menghapus `match_players` dan `match_incidents` untuk seluruh `NEW.season_id`.
2. **`t_clear_season_countries`** (`BEFORE UPDATE OF status ON season_tournaments WHEN NEW.status = 0`): Mengagregasi `match_players_countries` → `player_season_stats_countries`, lalu menghapus `match_players_countries`. *(Bug di Dart: `GROUP BY p.player_id, p.country_id` mengabaikan pemain naturalisasi/secondary nationality!)*
3. **`t_delete_old_messages`** (`AFTER INSERT ON messages`): Menghapus pesan lama sehingga total baris di `messages` selalu `<= 20`.
4. **`t_process_loans`** (`AFTER INSERT ON season_teams`): Otomatis menandai 0–1 pemain non-top-6 per klub menjadi `squad_status = 2` (loan listed).
5. **`t_sell_players`** (`AFTER INSERT ON season_teams`): Otomatis menandai 0–3 pemain non-top-6 per klub menjadi `market_status = 2` (for sale).

> **Rekomendasi Rust:** Di Rust, **drop trigger `t_clear_season*` dan `t_delete_old_messages`** (atau pindahkan seluruh logika arsip/listing/retensi ke fungsi Rust eksplisit di dalam transaksi) agar tidak ada *hidden side effect* saat mutasi tabel dan mudah di-unit-test secara deterministik.

---

## 5. Pemetaan Model Domain ke Rust (`naratama-model`)

Gunakan *newtype IDs* (`TeamId(i64)`, `PlayerId(i64)`, `SeasonId(i64)`, `TournamentId(i64)`, `CountryId(i64)`, `MatchId(i64)`) + `chrono::NaiveDate` (bukan `String` mentah!) agar bug salah tukar ID (seperti bug `makeTransferOffer` yang mengecek `userTeamId` alih-alih `sellerTeamId`) mustahil terjadi di waktu kompilasi:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TeamId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CountryId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TournamentId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeasonId(pub i64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MatchId(pub i64);

/// Uang kanonik: integer USD (signed i64, negatif diizinkan untuk kas merah)
pub type Money = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Confederation {
    None = 0, Concacaf = 1, Conmebol = 2, Uefa = 3, Caf = 4, Afc = 5, Ofc = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PositionGroup { Goalkeeper = 1, Defender = 2, Midfielder = 3, Attacker = 4 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PreferredFoot { Right, Left } // 'D' / 'I' di SQLite
```

