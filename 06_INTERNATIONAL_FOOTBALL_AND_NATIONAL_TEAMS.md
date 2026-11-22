# 06 — Sepak Bola Internasional, Kalender FIFA, Timnas, Peringkat Elo & Turnamen Negara

> **Sumber terverifikasi:** `lib/game/{international_calendar,national_team_service,national_team_selection_service,international_simulation_service,international_tournament_service}.dart`, `lib/data/models/international_models.dart`, `lib/data/initial_fifa_rankings.dart`, `lib/game/season_generator.dart`, `test/{sim_5_seasons_intl_test,major_tournaments_cycle_test,fifa_rankings_and_duty_test}.dart`.

---

## 1. Kalender FIFA 3 Jendela & Siklus Turnamen 4 Tahunan

### 1.1 Tiga Jendela Internasional Kanonik (`InternationalCalendarService`)
Setiap musim `(seasonId, startYear)` menghasilkan tepat **3 jendela** di `international_windows` (semua `is_mandatory = 1`, `club_fixtures_allowed = 0`):

| `window_id` | Tipe | Tanggal Laga | Blokir Klub & Pelepasan (`release` – `return`) | Maks Laga |
|---|---|---|---|---:|
| `win_{sid}_oct` | `fifaWindow` | `{startYear}-10-06` s/d `10-14` | **`10-05` – `10-15`** (11 hari) | 2 |
| `win_{sid}_mar` | `fifaWindow` | `{startYear+1}-03-23` s/d `03-31` | **`03-22` – `04-01`** (11 hari) | 2 |
| `win_{sid}_summer` | `finalTournament` | `{startYear+1}-06-11` s/d `07-19` | **`06-01` – `07-20`** (50 hari) | 7 |

- **Invarian Blackout Klub:** Tidak boleh ada 1 pun laga klub non-cancelled (selain friendly pra-musim `9999`) yang jatuh di rentang `[release_start_date, return_date]`. Jika masih ada jadwal klub bentrok saat pemain dipanggil, `rescheduleClashingDutyFixtures` otomatis menunda laga klub via `postponeMatch`.

### 1.2 Siklus 4-Tahunan Turnamen Negara (`SeasonGenerator`)
Ditentukan dari **`cycleYear = (startYear + 1) % 4`** (= tahun kalender turnamen musim panas):

| `cycleYear` | Contoh Musim | Jendela Oktober | Jendela Maret | Jendela Musim Panas |
|---:|---|---|---|---|
| **`1`** | 2024/25, 2028/29 | UEFA Nations League (`901`) MD 1–2 + Friendlies (`900`) | UEFA Nations League (`901`) MD 3–4 + Friendlies (`900`) | UEFA Nations League Knockouts + Friendlies (`900`) |
| **`2`** | 2025/26, 2029/30 | Friendlies (`900`) | Friendlies (`900`) | **FIFA World Cup (`57`)** (48 tim) |
| **`3`** | 2026/27, 2030/31 | UEFA Nations League (`901`) MD 1–2 + Friendlies (`900`) | UEFA Nations League (`901`) MD 3–4 + Friendlies (`900`) | UEFA Nations League Knockouts + **FIFA ASEAN Cup (`903`)** (16 tim) |
| **`0`** | 2027/28, 2031/32 | Friendlies (`900`) | Friendlies (`900`) | **AFC Asian Cup (`98`)** (24 AFC) + **UEFA EURO (`95`)** (24 UEFA) |

> *Catatan Penting:* Kualifikasi (`904`, `905`, `906`) sengaja **tidak dijadwalkan** otomatis dalam siklus saat ini — peserta turnamen utama langsung diambil dari peringkat `power DESC` (dengan negara user selalu diikutsertakan). Copa America (`94`), Gold Cup (`96`), AFCON (`97`), dan ASEAN Championship (`902`) sudah didukung di mesin knockout/podium, namun belum dipanggil di `season_generator.dart` (bisa diaktifkan dengan mudah di Rust!).

---

## 2. Eligibilitas Pemain, Cap-Tie & Algoritma Seleksi Skuad 23 Pemain

### 2.1 Aturan Eligibilitas & Cap-Tie (`NationalTeamService`)
- Negara tempat klub pemain bernaung **tidak pernah** memberi eligibilitas timnas.
- Resolusi `eligibleCountryIdsForPlayer(playerId)`:
  1. Jika ada baris `player_national_eligibility` dengan `is_locked = 1` → hanya negara tersebut (sudah *cap-tied*).
  2. Lazy cap-tie: jika pernah tampil `minutes > 0` di laga kompetitif (`country_matches.tournament_id != 900`), kunci permanen (`lockAllegiance`: set `is_locked = 1` & hapus eligibilitas negara lain).
  3. Selain itu: gabungkan `players.country_id` (primer) + semua baris sekunder di `player_national_eligibility`.
- `refreshAllCountryPowers`: `countries.power = ROUND(AVG(top 11 active primary players.power))`.

### 2.2 Seleksi Skuad & Pelepasan Wajib vs Opsional (`NationalTeamSelectionService`)
Dipicu otomatis 7 hari sebelum `release_start_date` (`ensureInternationalCallups`):
- **Target Default:** **`squadSize = 23`** dengan kuota minimum: **3 GK**, **7 DEF**, **6 MID**, **4 ATT** (total 20) + 3 slot fleksibel terbaik.
- **Filter Ketersediaan:** Tolak pemain yang `fitness < 40`, atau cedera (`injury_return_date > window.startDate`), atau sudah aktif di `international_duty` yang tanggalnya tumpang tindih. Tolak nama duplikat.
- **Rumus Skor Seleksi:**
  $$\text{score}(p) = p.\text{power} + \frac{p.\text{fitness}}{100.0} \times 3.0$$
- **Kapten & Nomor Punggung:** Kapten = pemain outfield dengan `power` tertinggi; kiper pertama mendapat nomor `#1`, sisanya `2..N`.
- **Pelepasan Wajib vs Persetujuan Manajer (Khusus Friendly):**
  - Jika jendela bertipe `friendly`, atau `!isMandatoryRelease`, **atau** semua laga negara tersebut di jendela itu adalah `tournament_id == 900` (uji coba): pemain dari klub user berstatus **`pending_release`** (tidak langsung masuk `country_callups`), memicu banner + dialog persetujuan di UI (`showFriendlyCallupConfirmationDialog`).
  - Jika user **menyetujui** → status jadi `'active'` & masuk `country_callups`.
  - Jika user **menolak** → status `'rejected'`, sistem otomatis memanggil pemain pengganti terbaik dari klub lain di posisi yang sama (`_replaceRejectedPlayer`), lalu menunda laga klub user yg bentrok untuk pemain yang disetujui.
  - Untuk turnamen resmi/jendela wajib: langsung `'active'`, pemain diblokir dari starting XI klub (`_dutyPlayerIds` & `_suspendedPlayerIds`), dan otomatis kembali ke klub (`status = 'returned'`) pada housekeeping setelah `return_date < currentDate`.

---

## 3. Simulasi Laga Internasional & Model FIFA Elo (SUM)

### 3.1 Pipeline Simulasi (`InternationalSimulationService`)
1. **Idempoten:** Abaikan jika `country_matches.status == 2`.
2. **Skuad:** Ambil dari `country_callups` (`>=11`), jika kurang jalankan `selectSquadForCountry`, fallback top-23 berdasarkan `players.country_id`.
3. **Reuse `MatchEngine`:** Formasi `4-4-2`, stadion netral sintetis 65.000 kursi, wasit temper `2..3`.
4. **Knockout Tie-Break:** Langsung adu penalti (`penaltyShootout`, 76% per tendangan) bila seri pada `phase > 0 && leg == 1 && tournamentId != 900` (tanpa perpanjangan waktu).
5. **Transaksi Atomik Pasca-Laga:**
   - Update `country_matches` (`status = 2`, skor, penalti, `winner_id`).
   - Simpan `match_players_countries` & update `player_season_stats_countries`.
   - **Kelelahan (`players.fitness`):** Starter turun **`8..12`**, pemain pengganti **`4..6`**, lantai **`30`** (diterapkan sekali; tidak pernah tumpang tindih dengan laga klub).
   - **Cap-tie otomatis:** Bila `tournamentId != 900 && minutes > 0`.
   - **Cedera:** Untuk setiap event `injury`, set `fitness = 40` dan `injury_return_date` `+7..27 hari` *(di Rust wajib dihitung dari `currentMatch.date`, memperbaiki bug `DateTime.now()` di Dart!)*.

### 3.2 Rumus Peringkat FIFA Elo (SUM Model)
- **Bobot Kepentingan Laga ($I$):**
  | Turnamen | ID | Kondisi Fase | Bobot $I$ |
  |---|---:|---|---:|
  | International Friendly | `900` | Semua | `10.0` |
  | UEFA Nations League | `901` | Semua | `15.0` |
  | FIFA ASEAN Cup | `903` | Grup & QF (`phase < 2`) / SF–Final (`>= 2`) | `20.0` / `25.0` |
  | Lainnya (Kualifikasi, Copa, AFCON, Gold, ASEAN Champ) | lainnya | Semua | `25.0` |
  | AFC Asian Cup & UEFA EURO | `98`, `95` | Grup & R16 (`< 2`) / QF–Final (`>= 2`) | `35.0` / `40.0` |
  | FIFA World Cup | `57` | Grup, R32, R16 (`< 3`) / QF–Final (`>= 3`) | `50.0` / `60.0` |
- **Ekspektasi Hasil ($W_e$):**
  $$dr = P_{\text{home}} - P_{\text{away}}, \qquad W_{e,\text{home}} = \frac{1}{10^{-dr / 600} + 1}, \qquad W_{e,\text{away}} = 1 - W_{e,\text{home}}$$
- **Hasil Aktual ($W$):**
  - Normal: Menang `1.0`, Seri `0.5`, Kalah `0.0`.
  - Adu Penalti: Pemenang **`0.75`**, Kalah **`0.50`**.
- **Perubahan Poin & Proteksi Babak Gugur:**
  $$\Delta = I \cdot (W - W_e)$$
  - **Proteksi Knockout FIFA:** Jika `phase > 0` dan `tournament_id ∈ {57, 98, 95, 903}`, maka **`if Δ < 0 → Δ = 0.0`** (tim tidak kehilangan poin Elo jika kalah di babak gugur turnamen utama!).
  - $P_{\text{new}} = \max(0.0, \text{round\_2dp}(P_{\text{old}} + \Delta))$.
- **Seed Awal (`kInitialFifaPoints`):** 254 negara (216 berperingkat FIFA 2024: Argentina #1 `1889.02`, Prancis `1851.92`, Spanyol `1836.42`, Indonesia `1124.17`, Thailand `1223.06`, Vietnam `1161.38`; 38 wilayah non-FIFA `0.0`).

---

## 4. Mesin Turnamen Internasional (`InternationalTournamentService`)

### 4.1 Katalog Format & Kemajuan Babak
| Turnamen | ID | Peserta | Grup | Best-3rd | Fase Gugur (`phase`) | Final Phase |
|---|---:|---:|---|---|---|---:|
| **FIFA World Cup** | `57` | 48 | 12 × 4 (SRR) | **8 dari 12** | R32(1)→R16(2)→QF(3)→SF(4)→Final+3rd(5) | `5` |
| **AFC Asian Cup** | `98` | 24 (AFC) | 6 × 4 (SRR) | **4 dari 6** | R16(1)→QF(2)→SF(3)→Final+3rd(4) | `4` |
| **UEFA EURO** | `95` | 24 (UEFA) | 6 × 4 (SRR) | **4 dari 6** | R16(1)→QF(2)→SF(3)→Final+3rd(4) | `4` |
| **UEFA Nations League** | `901` | 16 (UEFA) | 4 × 4 (SRR, Oct+Mar) | — | QF(1)→SF(2)→Final+3rd(3) | `3` |
| **FIFA ASEAN Cup** | `903` | 16 (11 ASEAN + 5 tamu: AUS, IND, CHN, HKG, JOR) | 4 × 4 (SRR) | — | QF(1)→SF(2)→Final+3rd(3) | `3` |
| **ASEAN Championship** | `902` | 10 ASEAN (tanpa Timor-Leste) | 2 × 5 (SRR) | — | SF(1)→Final+3rd(2) | `2` |

### 4.2 Undian Pot, Klasemen, Pairing Gugur & Podium
- **Undian Grup Berbasis Pot:** Peserta sudah diurutkan `power DESC`; Pot 1 = `0..G-1`, Pot 2 = `G..2G-1`, dst., tiap pot diacak ke `G` grup. *(Catatan Rust: ganti penjadwalan grup `(dateIdx+p)%dates.len()` menjadi Circle Method 3-ronde agar tidak ada negara main 2× di tanggal sama!)*
- **Klasemen & Best-3rd Tiebreaker:** `points DESC` → `GD DESC` → `GF DESC` → `countryName ASC`.
- **Cross-Group Knockout Pairing:**
  - *Tanpa Best-3rd (genap):* Paruh atas `(1A vs 2B, 1C vs 2D)`, paruh bawah `(1B vs 2A, 1D vs 2C)`.
  - *Dengan Best-3rd (WC 32 tim / EURO & Asian 16 tim):* Juara grup awal bertemu tim peringkat-3 terbaik, sisa juara grup bertemu runner-up dari ujung berlawanan, sisa runner-up saling bertemu.
- **Perebutan Tempat Ketiga & Podium:** Saat semifinal selesai (2 pemenang + 2 pecundang), jadwalkan **Final (`round = 1`, `maxPrevDate + 4d`)** dan **3rd-Place Playoff (`round = 2`, `H-1` sebelum Final)**. `getTournamentPodium` mengembalikan Juara 1, Juara 2, dan Juara 3 beserta skor dan adu penalti.
- **Matchmaking Uji Coba (`scheduleFriendliesForWindow`):** Ambil Top 32 (paksa sertakan negara user), urutkan `power DESC`. Laga 1 (`start + 2d`): pasangan berdekatan `(0 vs 1, 2 vs 3, ...)` agar imbang. Laga 2 (`end - 1d`): acak ulang untuk variasi lawan.

