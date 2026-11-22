# 03 — Mesin Simulasi Pertandingan, Formasi, Latihan, Fasilitas, Staf, Cedera & Skorsing

> **Sumber terverifikasi:** `lib/game/match_engine.dart` (1.376 baris), `lib/game/formation.dart`, `lib/game/training.dart`, `lib/game/squad_rules.dart`, `lib/data/repositories/{match_repository,facilities_repository,squad_repository}.dart`, `lib/game/career_service.dart`, `test/{match_engine_test,player_injury_test,red_card_suspension_test}.dart`.

---

## 1. Mesin Simulasi Pertandingan (`MatchEngine`)

### 1.1 Struktur Data Utama
- **`MatchInput`**: `home_squad`, `away_squad`, `home_team_id`, `away_team_id`, `home_power`, `away_power`, `home_formation`, `away_formation`, `home_lineup_ids`, `away_lineup_ids`, `capacity_{general,stand,terrace,boxes}`, `ticket_price_*`, `reference_price_*`, `home_form` (default `0.5`), `competition_prestige` (default `0.5`), `referee_temper` (`1..=5`, default `2`), `{home,away}_injury_reduction` (`0.0..=0.45`), `{home,away}_tactical_bonus` (`0.0..=0.075`).
- **`MatchEvent`** (14 tipe string): `"goal"`, `"assist"`, `"save"`, `"yellow_card"`, `"red_card"`, `"penalty_scored"`, `"penalty_missed"`, `"own_goal"`, `"injury"`, `"substitution_in"`, `"substitution_out"`, `"var_check"`, `"var_goal_disallowed"`, `"shootout_kick"` (menit `120`, detail `"scored <r>"` / `"missed <r>"`).
- **`MatchPlayerRow`**: `player_id`, `player_name`, `team_id`, `starter` (0/1), `minutes`, `goals`, `assists`, `yellow_cards`, `red_cards`, `mvp` (0/1), `rating` (integer `20..=100` = `2.0..10.0`), `points`. Hanya baris `minutes > 0` yang dikembalikan.
- **`MatchResult`**: skor akhir, `events` (kosong jika `recordEvents == false`), `player_rows`, kehadiran 4 tier, `revenue` (**gross tiket**), `shots`, `shots_on_target`, `saves`, `home_possession`.

### 1.2 Perhitungan Kekuatan Lini & Keunggulan Pertandingan
Menggunakan **`p.power` mentah** dari 11 starter (bukan nilai efektif kebugaran):
1. `_lineMean(starters, group)`: rata-rata `power` pemain starter yang memiliki `Lineup.groupOf(p) == group`. Jika sebuah lini **kosong (0 pemain natural)**, terkena penalti **-7%**: `_overallMean(starters) * 0.93` (atau `50.0` jika starter kosong).
2. Rating 3 lini mentah:
   $$\text{rawAtt} = \frac{6.0 \cdot \bar{P}_{\text{ATT}} + 2.4 \cdot \bar{P}_{\text{MID}} + 1.0 \cdot \bar{P}_{\text{DEF}}}{9.4}$$
   $$\text{rawMid} = \frac{6.0 \cdot \bar{P}_{\text{MID}} + 1.0 \cdot \bar{P}_{\text{ATT}} + 1.0 \cdot \bar{P}_{\text{DEF}}}{8.0}$$
   $$\text{rawDef} = \frac{6.0 \cdot \bar{P}_{\text{DEF}} + 2.4 \cdot \bar{P}_{\text{MID}} + 2.0 \cdot \bar{P}_{\text{GK}}}{10.4}$$
3. Blending dengan `teamPower` (jika `> 0`):
   - $\text{squadOvr} = (\text{rawAtt} + \text{rawMid} + \text{rawDef}) / 3.0$
   - $\text{ovr} = 0.65 \cdot \text{squadOvr} + 0.35 \cdot \text{teamPower}$
   - $\text{att/mid/def} = 0.75 \cdot \text{raw} + 0.25 \cdot \text{teamPower}$
4. **Matchup Edge & Home Advantage:**
   $$\text{ovrGap} = \text{homeOvr} - \text{awayOvr}$$
   $$\text{homeEdge} = 0.45(\text{homeAtt} - \text{awayDef}) + 0.25(\text{homeMid} - \text{awayMid}) + 0.40 \cdot \text{ovrGap} + 2.0$$
   $$\text{awayEdge} = 0.45(\text{awayAtt} - \text{homeDef}) + 0.25(\text{awayMid} - \text{homeMid}) - 0.40 \cdot \text{ovrGap} - 1.2$$
   > *Catatan:* Tidak ada *mentality slider* maupun *rock-paper-scissors* antar formasi di `MatchEngine`. Satu-satunya modifier taktis adalah `tacticalBonus` dari staf Tactical Analyst (`3★: +4.5%`, `4★: +6.0%`, `5★: +7.5%`). Juga belum ada flag `neutralVenue` di `MatchEngine` (bisa ditambahkan di Rust agar final/Piala Dunia benar-benar netral!).

### 1.3 Konstanta & Loop Simulasi Menit 1–90
- **Konstanta inti:** `_baseChancePerMinute = 0.112`, `_conversionBase = 0.132`, `_onTargetShare = 0.35`, `_penaltyPool = 0.060`, `_penaltyConvert = 0.76`, `_ownGoalPool = 0.020`, `_yellowPool = 3.6`, `_redPool = 0.12`, `_injuryPool = 0.40`.
- **Faktor Wasit:** `tempsFactor(temper) = 0.6 + 0.2 * temper` (`1=0.8`, `2=1.0`, `3=1.2`, `4=1.4`, `5=1.6`) — hanya menskala kartu kuning & merah.
- **Laju Peluang & Konversi Dasar:**
  - `shotRate(edge)`: jika `edge >= 0` → `0.112 * clamp(exp(edge * 0.026), 1.0, 2.35)`; jika `edge < 0` → `0.112 * clamp(exp(edge * 0.052), 0.16, 1.0)`.
  - `conversion(edge, tactBonus)`: jika `edge >= 0` → `mult = clamp(exp(edge * 0.015), 1.0, 1.62)`; jika `< 0` → `mult = clamp(exp(edge * 0.035), 0.32, 1.0)`; hasil `clamp(0.132 * mult * (1 + tactBonus), 0.035, 0.34)`.
  - `homeAttackShare = clamp(baseHomeRate / max(0.001, baseHomeRate + baseAwayRate), 0.12, 0.88)`.

**Urutan eksekusi setiap `minute` (`1..=90`):**
1. **Penyesuaian Kartu Merah:** `missing = max(0, 11 - onPitch.len())`. Tim yang kekurangan pemain: `rate *= 0.82^missing`; lawan mendapat `rate *= 1.16^oppMissing` dan `conversion *= 1.08^oppMissing`.
2. **Dominance Urgency Surge:** Jika `|ovrGap| >= 10.0 && minute >= 35` dan tim favorit hanya unggul `<= 1` gol → `surge = clamp(1.0 + (|ovrGap| - 8.0) * 0.015, 1.08, 1.38)`, `rate *= surge`, `conversion = clamp(conversion * 1.12, 0.04, 0.36)`.
3. **Serangan Home & Away:** Roll `< rate` → tambah `shots`, roll `r`:
   - `r < conversion` → Gol (+ `shotsOnTarget`). Jika open play & `recordEvents == true`: peluang **10% `var_check`**, dan di dalamnya **20% `var_goal_disallowed`** (net 2% gol dibatalkan; gol & assist di-rollback).
   - `r < conversion + 0.35` → `shotsOnTarget += 1`, `oppSaves += 1`, event `save` untuk kiper lawan.
4. **Penalti:** Home roll `< (0.060/90) * homeAttackShare`; Away `< (0.060/90) * (1 - homeAttackShare)`. Konversi **76%**. Tidak ada VAR untuk penalti.
5. **Gol Bunuh Diri:** Roll `< 0.020/90`, diuntungkan ke Home berdasar `homeAttackShare`, pelaku dipilih dari bek (bobot `2.0`) vs lainnya (`1.0`).
6. **Kartu Kuning & Merah:** `homeFoulShare = clamp(1.0 - homeAttackShare*0.4 + 0.2, 0.30, 0.70)`.
   - Kuning: roll `< (3.6/90) * tempsFactor`. Jika `yellowCards` mencapai 2 → kartu merah (`redCards = 1`, `yellowCards` direset ke `1`, detail `"second yellow"`, keluar lapangan, menit dikunci).
   - Merah langsung: roll `< (0.12/90) * tempsFactor` (detail `"straight red"`).
7. **Cedera:** Coin-flip sisi (`home`/`away`), roll `< (0.40/180) * clamp(1 - injuryReduction, 0.1, 1.0)`. Penyerang 30% lebih rentan (`1.30` vs `1.0`). Memicu substitusi otomatis.
8. **Substitusi Rutin:** Di menit **`62`, `72`, `84`**, tiap tim secara independen memiliki **85% peluang** mengganti pemain non-GK terlemah di grup posisi yang sama dengan pemain cadangan non-GK pertama di bangku.

**Bobot Pemilihan Pemain:**
- **Pencetak gol:** `ATT=3.2`, `MID=1.4`, `DEF=0.50`, `GK=0.03` × `clamp(power,35,99)/70.0` (+ `power/250.0` jika set-piece).
- **Pemberi assist (bukan pencetak & bukan GK):** `MID=1.6`, `ATT=1.2`, `DEF=0.8` × `clamp(power,35,99)/70.0`.
- **Pelanggar (kartu):** `DEF=1.6`, `MID=1.3`, lainnya `0.9`.

### 1.4 Rating, MVP, Penguasaan Bola, Adu Penalti & Simulasi Cepat
- **Rating Pemain (`20..=100`):**
  $$\text{rating} = \text{clamp}\Big(\text{round}\big(44 + 6 \cdot \text{outcome} + 14 \cdot G + 8 \cdot A - 16 \cdot RC - 3 \cdot YC + \mathcal{U}(-4, 4)\big),\; 20,\; 100\Big)$$
  (`outcome`: Menang=3, Seri=1, Kalah=0; `points = outcome + 2*G`).
- **Bonus Kiper Starter:** `rating += saves*4 + (conceded==0 ? 12 : 0) - (conceded>1 ? (conceded-1)*2 : 0)`; `points += (saves / 3) + (conceded==0 ? 3 : 0)`.
- **MVP:** Pemain dengan `minutes >= 30` dan `rating` tertinggi.
- **Penguasaan Bola:** `homePoss = clamp(round(50 + ((homeMid - awayMid)*0.65 + ovrGap*0.35 + 1.5)*0.85), 25, 75)`.
- **Knockout & Adu Penalti:** Tidak ada perpanjangan waktu 30 menit (langsung adu penalti). Pada 2-leg: Leg 1 seri dibiarkan; Leg 2 hitung agregat (`leg2Home + leg1Away` vs `leg2Away + leg1Home`), cek `awayGoalsRule` hanya jika aktif, lalu penalti. Setiap penendang (outfield berdasar `power DESC` lalu GK) punya peluang gol flat **76%**, best-of-5 lalu sudden death.
- **Simulasi Cepat (`quickSimulate` Poisson):** `diff = (homePower + 1.8) - awayPower` (catatan: `simulateMatchFast` memanggil dengan `homePower + 3`, sehingga keunggulan kandang efektif `+4.8`). Ekspektasi gol Home `1.35 * exp(...)`, Away `1.18 * exp(...)`, sampling Knuth Poisson.

---

## 2. Formasi & Seleksi Starting XI (`Formation`, `Lineup`)

### 2.1 Daftar 6 Formasi
`4-4-2` (default: 4D-4M-2F), `4-3-3` (4-3-3), `4-2-3-1` (4-5-1), `3-5-2` (3-5-2), `5-3-2` (5-3-2), `3-4-3` (3-4-3).  
Slot `team_lineups` berindeks `0..=10`: `0`=GK, `1..D`=DEF, `D+1..D+M`=MID, sisanya ATT.

### 2.2 Algoritma `Lineup.best`
1. **Skor Efektif Seleksi:** `effective(p) = p.power * (0.70 + 0.30 * clamp(p.fitness, 0, 100) / 100.0)`.
2. **Filter Kelayakan:** Hanya `status_id == 1`, bukan `bannedIds`, dan untuk non-GK wajib `fitness >= 35` (**GK bebas dari batas stamina 35**).
3. **Pengisian 3-Pass:**
   - Pass 1: Isi kuota tiap grup (1 GK, D DEF, M MID, F ATT) dari pemain natural terbaik.
   - Pass 2: Cross-fill lini kosong dari lini bertetangga (`DEF ↔ MID`, `ATT ↔ MID`, GK tidak pernah cross-fill).
   - Pass 3: Backfill darurat pemain outfield mana saja hingga 11.
   - Cadangan: maks **7 pemain** (outfield diprioritaskan di depan kiper cadangan, urut `effective DESC`).
4. **Formasi Terbaik AI (`refreshAiFormations`):** Coba semua 6 formasi, maksimalkan `attackOn + midfieldOn + defenseOn + ((teamId.hash ^ form.hash) % 100) * 0.05`.

---

## 3. Latihan & Perkembangan Pemain (`Training`)

- **5 Fokus (`TrainingFocus`):** `attack` (`pos 8,9,10,11`), `midfield` (`5,6,7`), `defence` (`2,3,4`), `goalkeeper` (`1`), `fitness` (semua aktif `fitness < 100`, tanpa batas jumlah pemain).
- **Biaya:** **`$75.000`** per sesi (`facility_upkeep`).
- **Gerbang Matchday:** Setiap fokus bisa dijalankan **1× per laga yang telah dimainkan** (`matchesPlayed > team_training.last_match_count` mereset `done_focuses`).
- **Skala Training Ground (Level 1–5):**

| Level | Nama Fasilitas | Biaya Upgrade | Maks Pemain/Sesi | Tambah Fitness | Tambah OVR (`power`) | Bonus Wonderkid Offseason (`age <= 21`) |
|---:|---|---:|---:|---:|---:|---:|
| 1 | Standard Pitches | — | 3 | +4 | +1 | +0 |
| 2 | Modern Training Complex | $1.2M | 4 | +5 | +1 | +1 |
| 3 | High-Performance Centre | $3.0M | 5 | +6 | **+2** | +1 |
| 4 | State-of-the-Art Training Ground | $6.5M | 6 | +7 | +2 | **+2** |
| 5 | World-Class Football Campus | $12.0M | 7 | +8 | +2 | +2 |

---

## 4. Fasilitas Klub, Staf Kepelatihan & Dossier Taktis

> **Catatan Upkeep:** Tidak ada biaya pemeliharaan mingguan otomatis untuk fasilitas/akademi — hanya biaya upgrade satu kali (CapEx).

### 4.1 Medical Centre (Level 1–5), Akademi (Level 1–10) & Stadion
- **Medical Centre:** Upgrade `1→2: $1.0M`, `2→3: $2.5M`, `3→4: $5.5M`, `4→5: $10.0M`.
  - Reduksi risiko cedera: `(medicalLevel - 1) * 0.07` (L1 `0%` s/d L5 `28%`).
  - Bonus pemulihan fitness mingguan (sehat): `medicalLevel - 1` (`0..+4`).
  - Bonus pemain `fitness <= 40`: `+2` jika `medicalLevel >= 3`.
- **Youth Academy (Level 1–10):** Biaya upgrade `level * $1.000.000` (`$1M` s/d `$9M`).
- **Ekspansi Stadion:** Setiap klik menambah `max(100, current / 10)` kursi pada masing-masing 4 tier (hingga batas `max_*`), biaya **`newSeats × $1.200`**.

### 4.2 Staf Kepelatihan (3 Peran Unik, 12 Kandidat Tetap 2★–5★)
Biaya rekrut satu kali: `1★ $100K`, `2★ $250K`, `3★ $650K`, `4★ $1.6M`, `5★ $3.2M` (pecat gratis).

| Peran (`team_staff.role`) | Gaji Mingguan (2★–5★) | Efek Gameplay Eksak |
|---|---|---|
| **`head_physio`** | $7K / $14K / $28K / $55K | Reduksi cedera `+stars * 0.04` (gabungan dgn Medical maks **`45%`**); pengurangan stamina pasca-laga turun dari `-8` → **`-6`** (3–4★) atau **`-5`** (5★); pemulihan mingguan sehat `+round(stars/2)`, cedera `+stars`. |
| **`tactical_analyst`** | $8K / $15K / $32K / $60K | Bonus konversi gol di `MatchEngine`: `stars >= 3 ? stars * 0.015 : 0` (`+4.5% / +6.0% / +7.5%`). Keyakinan Dossier Taktis `min(98, 45 + stars*11)` (`0★ = 35%`), membuka saran kontra-taktik tambahan di ≥3★ & ≥4★. |
| **`head_scout`** | $8K / $16K / $35K / $65K | Saat regional scout dikirim: peluang `60%` mendapat +1 wonderkid ekstra (≥4★) dan bonus potensial `+(stars - 2) * 2` (`+2 / +4 / +6` untuk 3★/4★/5★). |

### 4.3 Dossier Analisis Taktis Pra-Pertandingan (`generateTacticalReport`)
Mengambil formasi lawan (`team_tactics`), 2 pemain kunci (`power DESC`), gaya main & mentalitas prediksi berdasar switch formasi (`4-3-3`, `3-5-2`, `4-2-3-1`, `5-3-2`/`5-4-1`, default), kelemahan (GK `< oppPower - 3`, bek terlemah jika bek ≥ 4, kelelahan jika ≥ 3 pemain `fitness < 80`), rekomendasi kontra-taktik (bertingkat sesuai bintang analis), dan prediksi laga (`oppPower > 82`, `>= 70`, `< 70`).

---

## 5. Skuad, Akademi Muda, Cedera & Skorsing Berbasis Kompetisi

### 5.1 Aturan Kuota Skuad & Akademi Muda
- **`SquadRules`:** `maxSquadSize = 30`, `minSquadSize = 19`.
- **Intake Akademi User (`graduateYouthPlayersIfDue`):** Berjalan tiap **90 hari in-game**. Menghasilkan **5–9 prospek** usia `17`, `power = 42 + level*2 + 0..3`.
  - Peluang *Generational Wonderkid*: Lv10 `8%` (potensial 95–98), Lv8–9 `4%` (93–96), Lv6–7 `2%` (93–96), Lv1–5 `0%`. Standar: `clamp(power + 12 + level + 0..8, power, 94)`.
  - Promosi (`signProspect`): cek `< 30` pemain, gaji `$1.200/mgg`, kontrak 3 tahun, status reserve.
- **Regenerasi Akademi AI (`replenishAIAcademies`, offseason):** Menjaga tiap klub AI punya minimal `>= 2 GK`, `>= 5 DEF`, `>= 5 MID`, `>= 3 ATT`, dan total `>= 20` (maks 8 regen/musim, cap 30). OVR `40–48`, 12% wonderkid (potensial 85–90), 88% normal (65–84).

### 5.2 Cedera (`players.injury_return_date` & `fitness = 40`)
- Tidak ada nama jenis cedera (hanya durasi hari & fitness turun ke 40).
- Laga klub: **`1–14 hari`** sejak `match.date`; laga timnas: **`7–27 hari`** *(bug di Dart memakai `DateTime.now()`, WAJIB pakai `match.date` di Rust!)*.
- Pemain dianggap cedera pada tanggal `ref` bila `(injury_return_date - ref).inDays > 0`. Tidak pernah memperpendek tanggal cedera yang sudah lebih lama.

### 5.3 Skorsing Kartu Merah Berbasis Kompetisi (*Competition-Scoped Suspensions*)
Ini adalah subsistem yang telah diaudit ketat di `red_card_suspension_test.dart`:
1. **Tidak ada akumulasi kartu kuning lintas laga.** Hanya kartu merah (langsung atau 2 kuning) yang memberi larangan **1 pertandingan** (`type = 2`, `matches_remaining = 1`).
2. **Isolasi Kompetisi (`suspensions.competition_type`):** Setiap turnamen dipetakan ke enum `CompetitionType`: `'league'`, `'domesticCup'`, `'continental'`, `'superCup'`, `'international'`, atau `'friendly'` (`9999` & `900`).
   - Kartu merah di **liga** hanya melarang bermain di liga berikutnya — pemain boleh tampil di piala/friendly, dan laga piala/friendly **tidak mengurangi** sisa skorsing liga.
   - Kartu merah di **friendly** hanya berlaku untuk friendly berikutnya, **tidak pernah** mengganggu laga liga!
   - Baris legacy (`competition_type IS NULL`) diperlakukan sebagai `'league'`.
3. **Urutan Wajib dalam Transaksi `saveResult`:**
   - **Langkah 1 (Tick dulu):** `tickSuspensionsForMatch(home, away, compType)` mengurangi `matches_remaining -= 1` hanya untuk klub yang bermain dan hanya untuk `compType` laga ini (sekaligus menggabungkan baris duplikat; hapus jika mencapai 0).
   - **Langkah 2 (Catat ban baru):** Deduplikasi `suspendedPlayerIds` (1 pemain maks +1 ban per laga); jika sudah punya ban aktif di `compType` yang sama, jumlahkan ke satu baris; jika belum, insert baru.
4. **Isolasi Weekly Housekeeping:** `tickSuspensions()` di minggu jeda hanya menyentuh `competition_type IS NULL`, sehingga jeda FIFA / minggu kosong tidak pernah menghapus skorsing resmi.
5. **Refresh Kekuatan Klub (`refreshTeamPower`):** `teams.power = round(AVG(top 11 active players.power))`, `teams.value = SUM(all active players.value)`.

