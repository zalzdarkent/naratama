# 04 — Konfigurasi Kompetisi, Generator Musim, Algoritma Penjadwalan & Kalender Global

> **Sumber terverifikasi:** `lib/game/competition_config.dart` (824 baris), `lib/game/season_generator.dart` (1.702 baris), `lib/game/fixture_scheduler.dart` (724 baris), `lib/game/fixture_calendar.dart` (1.305 baris), `lib/game/fixture_validator.dart` (810 baris), `lib/data/repositories/season_repository.dart` (1.424 baris).

---

## 1. Sistem Konfigurasi Kompetisi Terpusat (`CompetitionConfig`)

Semua aturan kompetisi dipusatkan pada `CompetitionConfig` (dipersistensikan ke tabel `competition_configs` per `(tournament_id, season_start_year)`).

### Enum & Urutan Resolusi
- **`CompetitionType`:** `league`, `domesticCup`, `continental`, `superCup`, `international`, `friendly`.
- **`CompetitionFormat`:** `doubleRoundRobin`, `singleRoundRobin`, `leaguePhase`, `groupStage`, `knockout`, `singleMatch`, `multiStage`.
- **Urutan Resolusi:** Tabel `competition_configs` (prioritas tahun spesifik, lalu `season_start_year = 0`) → override in-memory → 24 profil bawaan (`_knownProfiles`) → `_fallback(tournament)`:
  - `orgLeague (1)` → `doubleRoundRobin`, interval 7 hari, min rest 2 hari, `allowedWeekdays` dari `play_days`.
  - `orgNationalCup (3)` → `knockout`, `knockoutLegs = (rounds == 2 ? 2 : 1)`, final 1 leg netral, `trimToPowerOfTwo = true`.
  - `orgInternational (2)` → `slots <= 2`: `superCup/singleMatch`; `3..8`: `international/knockout`; `>8`: `continental/groupStage` (grup 4, DRR, KO 2 leg, final 1 leg netral).
  - `orgFriendly (4)` → `friendly/singleMatch` (tanpa ET/penalti).

### Profil 24 Kompetisi Bawaan (`_knownProfiles`)
*(Default umum: `matchdayIntervalDays=7`, `minRestDays=2`, `extraTime=true`, `penalties=true`, **`awayGoals=false`**.)*

| ID | Kompetisi | Type | Format | Peserta | Detail Fase | KO Legs / Final |
|---:|---|---|---|---:|---|---|
| **49** | UEFA Champions League | `continental` | `leaguePhase` | 36 | 8 lawan (4H/4A), top 16 lolos | 2 leg / Final 1 leg netral |
| **51** | UEFA Europa League | `continental` | `leaguePhase` | 36 | 8 lawan (4H/4A), top 16 lolos | 2 leg / Final 1 leg netral |
| **135** | UEFA Conference League | `continental` | `leaguePhase` | 36 | 6 lawan (3H/3A), top 16 lolos | 2 leg / Final 1 leg netral |
| **99 / 164 / 1650** | ACL Elite / ACL Two / AFC Challenge | `continental` | `groupStage` | 32 | Grup 4 tim, DRR, top 2 lolos | 2 leg / Final 1 leg netral |
| **50 / 52 / 53 / 100** | CONCACAF / Libertadores / Sudamericana / CAF CL | `continental` | `groupStage` | 32 | Grup 4 tim, DRR, top 2 lolos | 2 leg / Final 1 leg netral |
| **163** | FIFA Club World Cup | `international` | `groupStage` | 16 | Grup 4 tim, SRR, top 2 lolos | 1 leg / Final 1 leg netral |
| **54 / 1651** | UEFA Super Cup / AFC Super Cup | `superCup` | `singleMatch` | 2 | 1 laga langsung | 1 leg netral |
| **56** | FIFA Intercontinental Cup | `international` | `knockout` | 8 | Bracket gugur (`useByes=true`) | 1 leg / Final 1 leg netral |
| **57** | FIFA World Cup | `international` | `groupStage` | 48 (≥2026) | 12 grup × 4, SRR | 1 leg / Final 1 leg netral |
| **95 / 98** | UEFA EURO / AFC Asian Cup | `international` | `groupStage` | 24 | 6 grup × 4, SRR | 1 leg / Final 1 leg netral |
| **94 / 96 / 97** | Copa America / Gold Cup / AFCON | `international` | `groupStage` | 16 / 16 / 24 | Grup 4 tim, SRR | 1 leg / Final 1 leg netral |
| **901** | UEFA Nations League | `international` | `groupStage` | 16 | 4 grup × 4, DRR (4 promo/rel) | 1 leg / Final 1 leg netral |
| **902** | ASEAN Championship | `international` | `groupStage` | 10 | 2 grup × 5, SRR | **2 leg (termasuk Final)** |
| **903** | FIFA ASEAN Cup | `international` | `groupStage` | 16 | 4 grup × 4, SRR | 1 leg / Final 1 leg netral |
| **900** | International Friendly | `friendly` | `singleMatch` | — | Tanpa ET/penalti | 1 leg |

---

## 2. Generator Musim & Kualifikasi (`SeasonGenerator`)

- **Kickoff Default:** `DateTime(startYear, 8, 9)` (Sabtu awal Agustus).
- **Urutan `generateCareer`:**
  1. Insert `seasons` (`"Season YYYY/YY"`).
  2. Buat 3 jendela FIFA & jadwalkan siklus 4-tahunan timnas (lihat Dokumen 06).
  3. Hubungkan semua liga domestik, piala nasional, dan piala internasional ke `season_tournaments` + simpan `competition_configs`.
  4. Pilih peserta:
     - **Liga Domestik:** Semua klub di `(countryId, division)`, dibatasi `lg.slots` (fallback 24) berdasar `power DESC`.
     - **Piala Domestik:** Semua klub di `countryId` lintas divisi (jika ≥ 4 klub).
     - **Piala Kontinental (berurutan `tournament_id ASC` agar kasta atas mengambil slot duluan!):** Ambil kuota dari `international_slots`. Urutan kualifikasi per negara (`_continentalQualifiers`): klasemen akhir Divisi 1 musim sebelumnya jika ada, atau fallback `power DESC`. Klub yang sudah masuk kompetisi kasta lebih tinggi dieksklusi dari kasta bawah (`49 → 51 → 135`, `99 → 164 → 1650`).
     - **Super Cup & Global Cups (`54, 1651, 56, 163`):** Tidak mengecualikan peserta kontinental. Diambil dari juara/runner-up turnamen *feeder* musim sebelumnya (`_lastFinal`):
       - `54` UEFA Super Cup ← `[49, 51]` (2 tim)
       - `1651` AFC Super Cup ← `[99, 164]` (2 tim)
       - `163` Club World Cup ← `[49, 52, 99, 50, 100, 51, 53, 164, 135, 1650]` (16 tim)
       - `56` Intercontinental Cup ← `[49, 52, 99, 100, 50]` (juara tiap konfederasi; OFC `null` dilewati).
       - **Virtual Feeder Fallback:** Karena satu save hanya mensimulasikan piala kontinental negara user, konfederasi lain belum punya hasil nyata di DB. `SeasonGenerator` menjalankan **`_simulateVirtualFeederFinal`** (knockout ringan dalam memori menggunakan probabilitas $P(\text{home}) = P_H / (P_H + P_A)$) tanpa menulis laga ke DB.
  5. Batch insert `season_teams`, jadwalkan Liga Domestik (Circle Method + Optimizer), jadwalkan Babak 1 Piala Domestik (hari Rabu: `startDate + (7 * (2 + (phase-1)*max(2, rounds/8)) - 3)`), jadwalkan pembukaan Super Cup (`startDate - 4d`) & Kontinental (Selasa/Kamis, hindari jendela Oktober 5–15).
  6. Jalankan `resolveFixtureClashes(seasonId)` dan `repairSelfMatches(seasonId)`.

---

## 3. Algoritma Penjadwalan Matematis (`FixtureScheduler`)

### 3.1 Circle Method Round-Robin + Home-Away Balancing Optimizer
1. **Circle Method:** Jika jumlah tim `N` ganjil, tambah sentinel BYE (`-1`). Indeks `0` tetap, indeks `1..n-1` berotasi searah jarum jam selama `n - 1` ronde. Pasangan `(ring[i], ring[n - 1 - i])`.
2. **Double Round-Robin Mirror:** Paruh kedua (`leg = 2`, ronde `halfRounds+1..2*halfRounds`) **selalu membalik** tuan rumah/tamu paruh pertama secara persis sehingga setiap klub genap bermain tepat `N - 1` kandang dan `N - 1` tandang.
3. **Simulated Annealing Home-Away Optimizer (`_optimiseOrientations`):**
   - Hanya membalik orientasi H/A di paruh pertama (`firstHome[r][i]`), tidak pernah mengubah pasangan lawan.
   - **Fungsi Penalti `_teamScore` (target = 0):**
     - **Hard — Streak H/A > 2:** untuk setiap deret identik dengan panjang `run > 2`: `+(run - 2)^2 * 100`.
     - **Soft — Jendela 5 Laga:** untuk setiap jendela 5 laga berurutan dengan jumlah $w_k \in \{-5,-3,-1,1,3,5\}$, jika $|w_k| > 1$: `+(|w_k| - 1)^2 * 4` (4-1 = `+16`, 5-0 = `+64`).
     - **Hard — Keseimbangan Single RR:** bila `!doubleRound` dan `|totalH - totalA| > 1`: `+(imbalance - 1)^2 * 50`.
   - **Seed Deterministik:** hash dari `teamIds` (`seed = 0x5EED; seed = (seed * 31 + id) & 0x7fffffff`).
   - **Parameter Annealing:** `iterations = clamp(14000 + matchCount * 180, 14000, 42000)`, hingga 3 restart (`seed + restart * 7919`), `T_init = 3.0`, pendinginan `T = max(0.02, T * 0.99992)`, terima jika `delta <= 0` atau `rand < exp(-delta / max(0.05, T))`. Berhenti segera bila `score == 0`.

### 3.2 UEFA League Phase Scheduler (`LeaguePhaseScheduler`)
- Input: 36 tim, `opponents = 8` (UCL/UEL) atau `6` (UECL), `homeMatches = 4` atau `3`, map `associationOf`.
- Ambil seluruh ronde Circle Method, lalu uji semua offset siklik sepanjang `playableOpponents` ronde berturut-turut untuk meminimalkan jumlah bentrok sesama asosiasi/negara (`_associationConflicts`).
- **Orientasi H/A Eksak (Algoritma Eulerian Hierholzer):** Bangun graf tak-berarah dari semua pasangan terpilih, jalankan sirkuit Eulerian pada tiap komponen sehingga setiap simpul berderajat genap mendapat `in-degree == out-degree == targetHome` secara eksak (`4H/4A` atau `3H/3A`)! Dilengkapi hill-climb fallback 20.000 iterasi bila ada derajat ganjil.

### 3.3 Group Stage (Snake) & Knockout Bracket
- **Distribusi Grup Snake:** Urutkan peserta `power DESC, teamId ASC`. Baris genap isi grup `0..G-1`, baris ganjil isi `G-1..0`. Tiap grup dijadwalkan dengan `RoundRobinScheduler`.
- **Bracket Gugur (`KnockoutScheduler`):**
  - Mode `trimToPowerOfTwo` (Piala Domestik): pangkas peserta ke pangkat dua terbesar (`<= N`), lipat seeding `(0 vs N-1, 1 vs N-2, ...)`.
  - Mode `useByes` (Kontinental/Global): `bracket = nextPowerOfTwo(N)`, `byeCount = bracket - N` unggulan teratas mendapat *bye* (disimpan di `knockout_byes`), sisanya bermain di babak pembuka.
- **Kemajuan Dinamis Babak Berikutnya (`maybeScheduleNextCupRound`):**
  - *Group → KO:* Bila 2 lolos/grup dan jumlah grup genap, paruh atas bracket `(1A vs 2B, 1C vs 2D, ...)`, paruh bawah `(1B vs 2A, 1D vs 2C, ...)` sehingga klub segrup tidak bertemu sebelum Final.
  - *League Phase → KO:* Ambil top `16` klasemen langsung ke Babak 16 Besar (tanpa babak play-off 9–24).
  - *KO Phase p → p+1:* Gabungkan `byes(phase p)` + pemenang `leg = MAX(leg)`. Bila tanpa bye, pasangkan berurutan `(winner[0] vs winner[1], ...)` mempertahankan pohon bracket.

---

## 4. Kalender Global Terpadu & Resolusi Konflik (`FixtureCalendar`)

### Batasan Keras Kalender
1. **Jendela Blackout FIFA (3 jendela per musim):**
   - Oktober: blokir klub **`Oct 5 – Oct 15`**
   - Maret: blokir klub **`Mar 22 – Apr 1`**
   - Turnamen Musim Panas: blokir klub **`Jun 1 – Jul 20`**
2. **Batas Akhir Musim Klub:** `seasonStart = startYear-07-01`, `seasonEnd = endYear-06-30` (secara efektif laga klub berakhir **31 Mei** karena blokir 1 Juni, sedangkan timnas berjalan 11 Juni–19 Juli).
3. **Jeda Istirahat:** Preferensi **`preferredRestDays = 3`** (72 jam), minimum keras **`minRestDays = 2`** (48 jam).
4. **Bentrok Stadion:** Tidak boleh ada 2 laga pada `stadium_id > 0` yang sama di hari yang sama.
5. **Monotonisitas Ronde:** Dalam setiap scope `(tournament_id, phase, group_id, leg)`, berlaku $\min(\text{date}(R_{r})) \ge \max(\text{date}(R_{r-1}))$.

### Algoritma `resolveSeasonConflicts` (Maks 8 Outer Pass)
Bergantian menjalankan:
- **Pass A (`_resolveSeasonConflictsPass`):** Laga berstatus `finished (2)` dikunci sebagai *fixed points*. Laga terjadwal diurutkan deterministik `(date, tournament_id, phase, round, match_id)`. Jika melanggar blackout / bentrok klub / rest / bentrok stadion / urutan ronde → cari tanggal pengganti ke depan (`0..180` hari) dengan 3 tingkat relaksasi:
  - Pass 0: `rest >= 3` + patuhi `allowedWeekdays`
  - Pass 1: `rest >= minRestDays (2)` + patuhi `allowedWeekdays`
  - Pass 2: `rest >= minRestDays (2)` + bebas hari
- **Pass B (`_enforceRoundOrderPass`):** Jika `min_date(Round_r) < max_date(Round_{r-1})`, coba tarik mundur laga `Round_{r-1}` yang terlalu telat; jika masih terbalik, dorong maju laga `Round_r`.
- **Penundaan Manual (`postponeMatch`):** Mempertahankan `match_id`, mencatat `original_date` pertama dan log audit di `fixture_reschedules` tanpa men-generate ulang kompetisi.

---

## 5. Validator Integritas Jadwal (`FixtureValidator`) & Klasemen (`SeasonRepository`)

### Kode Validasi (`validateSeason`)
Menghasilkan `FixtureValidationReport` dengan `severity: error | warning`:
- **League / Group:** `*_fixture_count`, `league_self_fixture`, `league_invalid_club`, `league_duplicate_fixture`, `*_pair_coverage`, `*_missing_pair`, `*_home_away_reversal`, `*_appearance_count`, `*_home_away_totals`, `*_home_away_balance`, `*_round_count`, `*_double_booked`.
- **League Phase:** `league_phase_fixture_count`, `*_invalid_club`, `*_self_fixture`, `*_repeated_opponent`, `*_opponent_count`, `*_home_away`, `*_double_booked`.
- **Knockout:** `knockout_duplicate_club`, `knockout_leg_count`, `knockout_no_reversal`, `knockout_missing_winner`, `knockout_invalid_winner`.
- **Global Calendar:** `calendar_duplicate_id`, `calendar_invalid_club`, `calendar_duplicate_fixture`, `calendar_invalid_date`, `calendar_overlap`, `calendar_rest` *(warning)*, `calendar_stadium_overlap`, `calendar_round_order`, `calendar_knockout_leg_order`.

### Aturan Klasemen, Pergerakan Posisi & Leaderboard
- **Poin:** Menang `3`, Seri `1`, Kalah `0`.
- **Urutan Tiebreaker (`_sortStandingRows`):** `points DESC` → `goalDifference DESC` → `goalsFor DESC` → `name.toLowerCase() ASC` (tidak ada H2H).
- **Indikator Naik/Turun (`positionChange`):** Menghitung ulang tabel menggunakan laga `round < maxRound` untuk memperoleh `previousPosition`, lalu `previousPosition - currentPosition`.
- **Leaderboard Rating (`topRated`):** Syarat minimal `minMatches = ceil(matchCount * 0.35)` dan `minMinutes = minMatches * 45` (fallback `minutes >= 90` jika kosong).
- **Perhatian Bug Saat Porting ke Rust:** `SeasonRepository.standings` saat ini tidak memfilter `phase = 0` saat `groupId == null`, sehingga jika kompetisi `leaguePhase` sudah masuk babak gugur, laga gugur ikut terhitung di klasemen fase liga — **wajib tambahkan filter `AND phase = 0` di Rust!**

