# 05 — Ekonomi Klub, Buku Besar Keuangan, Valuasi Pasar, Transfer, Pinjaman & Scouting

> **Sumber terverifikasi:** `lib/game/{finance_rules,finance_calc,finance_service,finance_health,finance_ffp_service,finance_summary_builder,broadcast_service,prize_engine,player_valuation_service,transfer_window_manager}.dart`, `lib/data/repositories/{finance_repository,prize_repository,sponsorship_repository,transfer_repository,scouting_repository}.dart`, `docs/FINANCE_REALISM_SPEC.md`, `AUDIT_MV.md`.

---

## 1. Model Uang Kanonik & Buku Besar Immutable (`FinanceService`)

- **Mata Uang Kanonik:** Integer bertanda (`i64` di Rust), 1 unit = `$1` USD game.
- **Satuan Gaji:** `players.salary`, `contracts.salary`, `team_staff.salary`, `scouts.salary` semuanya adalah **gaji mingguan gross** (`annual = weekly * 52`).
- **Saldo Kas (`teams.budget`):** Saldo kas riil (bisa negatif; **tidak pernah di-clamp 0**).
- **Invarian Keras Sistem:**
  $$\forall \text{team\_id}:\quad \text{teams.budget} \;==\; \sum \text{finance\_ledger.amount}$$
  *(Saldo awal dicatat 1× sebagai baris `category = 'board_funding'`, `ref_type = 'opening_balance'`.)*

### 1.1 Kategori Tertutup (`LedgerCategory`, 22 total) & Periode (`LedgerPeriod`, 5 total)
- **Periode:** `weekly`, `matchday`, `transfer`, `annual`, `board`.
- **Income (11):** `matchday_tickets`, `matchday_merch`, `broadcast`, `sponsorship_upfront`, `sponsorship_weekly`, `sponsorship_bonus`, `prize_money`, `player_sales`, `sell_on_received`, `loan_fee_received`, `board_funding`.
- **Expense (11):** `player_wages`, `staff_wages`, `academy_upkeep`, `facility_upkeep`, `matchday_ops`, `transfer_fees`, `agent_fees`, `sell_on_paid`, `loan_fee_paid`, `bonus_payouts`, `ffp_levy`.
- **Football Revenue (7, penyebut SCR/PSR):** `matchday_tickets`, `matchday_merch`, `broadcast`, `sponsorship_upfront`, `sponsorship_weekly`, `sponsorship_bonus`, `prize_money` *(tidak termasuk jual pemain, pinjaman, atau injeksi direksi)*.

### 1.2 Skala Ekonomi Global (`EconomyScale`)
- `national(countryPower)`: `0.35 + clamp(countryPower, 0, 100) * 0.0175` (rentang `[0.35, 2.10]`).
- `confederation(c)`: `1 => 0.85`, `2 => 0.95`, `3 => 1.15` (UEFA), `4 => 0.90`, `5 => 0.85` (AFC), `6 => 0.80` (OFC), `_ => 1.00`.
- `tier(div)`: `1.0 / (1.0 + clamp(div - 1, 0, 5) * 0.35)` (Div 1 = `1.0`, Div 2 = `0.7407`, Div 3 = `0.5882`, Div 4 = `0.4878`).

### 1.3 Injeksi Darurat Direksi & Status FFP
- **Emergency Board Injection (`emergencyInjections`):** Maksimal **1× per klub per musim** jika `budget < 0`. Menyuntik `+(-budget)` (`board_funding`, `ref_type='emergency_injection'`), mengembalikan saldo ke `0` dan menandai `finance_club_state.distress = 1`. Jika jatuh negatif lagi di musim yang sama, dibiarkan negatif dengan status `distress = 1`.
- **Status FFP Aktual di Kode (`FinanceFfpService`):**
  - **Telemetri & Formula AKTIF (`health()`):**
    - `squadCost = annualPlayerWages + annualStaffWages + annualAmortization + agentFees`
    - `SCR = squadCost / max(footballRevenue, 1)` (Warning `0.75`/`0.70`, Breach `0.85`/`0.75`, HardBlock `1.00`/`0.90` untuk `standard`/`strict`).
    - Rolling 3-musim `PSR = ledgerNet - boardFunding + facilityCapex + transferFeesPaid - amortization - bookValueOfSales - bookValueWriteOff`, dengan `allowedLoss = max(2M, round(0.10 * psrRevenue))` (strict: `1M` / `5%`).
  - **Penegakan Sanksi SENGAJA DINONAKTIFKAN:** `evaluateSeason()` dan `canRegister()` saat ini langsung `return null` atas permintaan user sebelumnya (UI FFP juga disembunyikan dari tab Finance). Di Rust, jadikan **toggle pengaturan karier** (`ffp_enforcement: Off | Relaxed | Standard | Strict`) agar defaultnya tetap bebas blokir seperti sekarang namun bisa diaktifkan bagi pemain hardcore!

---

## 2. Mesin Pendapatan (Matchday, Hak Siar, Hadiah & Sponsor)

### 2.1 Net Matchday & Elastisitas Harga Tiket (`FinanceCalc`)
- **Harga Referensi per Divisi:** `max(1, round(basePrice * EconomyScale.tier(division)))`, dengan harga dasar: `General = $14`, `Stand = $38`, `Terrace = $21`, `Boxes = $120`. Rentang wajar harga tiket: `0.50× – 2.50×` referensi.
- **Attractiveness:** `clamp(0.30 + 0.006*homePower + 0.20*form + 0.10*prestige + 0.05*(awayPower/100), 0.20, 1.00)`.
- **Price Demand:** `clamp(1.35 - 0.35 * (price / referencePrice), 0.35, 1.15)`.
- **Kehadiran:** Tier biasa = `round(capacity * clamp(attractiveness * priceDemand, 0.25, 1.00))`; VIP Boxes (inelastis) = `round(capacity * clamp(0.55 + 0.45 * attractiveness, 0.55, 1.00))`.
- **Pencatatan Ledger Tuan Rumah (`recordMatchday`):**
  - `matchday_tickets` = `grossTickets + round(grossTickets * 0.20)` *(hospitality 20%)*
  - `matchday_merch` = `totalAttendance * 3` *($3 per penonton)*
  - `matchday_ops` = `-round(grossTickets * 0.10)` *(biaya operasional 10%)*

### 2.2 Hak Siar Domestik (`BroadcastService`, 2 Cicilan)
- **Pool per `(country_id, division)`:**
  $$\text{pool} = \max\Big(100\,000,\; \text{round}\big(150\,000\,000 \times \text{national} \times \text{conf} \times \text{tier} \times (0.75 + \tfrac{\ln(\max(E,2))}{4})\big)\Big)$$
- **Distribusi:** **70% dibagi rata** (`equalShare`), **30% berdasarkan merit** (bobot peringkat `E, E-1, ..., 1`, penyesuaian pembulatan eksak sehingga $\sum \text{shares} == \text{pool}$).
- **Pembayaran Idempoten (Semua Klub):**
  - Cicilan 1 (`broadcast_preseason`): `share / 2` pada housekeeping pertama.
  - Cicilan 2 (`broadcast_midseason`): `share - share / 2` saat ≥ 50% total laga musim selesai (`finished * 2 >= total`).

### 2.3 Mesin Hadiah Kompetisi (`PrizeEngine`, `PrizeRepository`)
- **Pool:** `scaledBase = round(rule.basePool * national * conf * tier * (0.75 + ln(max(E,2))/4))`; jika DB punya `legacyPrizeTeams > 0`, ambil `max(scaledBase, (scaledBase + legacy) / 2)`, minimum `$100.000`.
- **Profil Default 5 Jenis Kompetisi:**
  | Jenis | `basePool` | Partisipasi/Klub | Bonus Menang | Bonus Seri | Runner-Up | Juara |
  |---|---:|---:|---:|---:|---:|---:|
  | `league` | $15.0M | $0 | $0 | $0 | $0 | *(bobot 20 peringkat)* |
  | `domesticCup` | $20.0M | $150K | $0 | $0 | $1.25M | $2.5M |
  | `continental` | $55.0M | $1.0M | $625K | $225K | $5.0M | $10.0M |
  | `intercontinental` | $100.0M | $1.5M | $1.0M | $375K | $7.5M | $15.0M |
  | `superCup` | $7.5M | $625K | $0 | $0 | $500K | $3.75M |
- **Bobot Peringkat Liga (20 posisi, total 230):** `[40, 30, 24, 20, 17, 15, 13, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 2, 2, 1]`.
- **Fraksi Babak Piala (`qualificationRounds`):** Depth 1..5 = Domestic Cup `6/9/13/20/30%`, Continental `8/12/17/24/34%`, Intercontinental `10/16/22/30/40%`.
- **Override Tier Kontinental:** UEL (`51`) = tier 2, UECL (`135`) = tier 3, Sudamericana (`53`) = tier 2, ACL Two (`164`) = tier 2, AFC Challenge (`1650`) = tier 3.

### 2.4 Sponsor 3 Slot (`SponsorshipRepository`)
- **3 Slot Unik:** `1` Main Shirt (`$1.2M × scale`), `2` Stadium (`$800K × scale`), `3` Sleeve (`$500K × scale`), dengan `scale = max(1.0, (clubPower - 40) / 10.0)`.
- **Struktur Kontrak:** Upfront = `baseUpfront`, Mingguan = `round(baseUpfront / 38.0 * 0.8)`, Target Peringkat = `3` (`power>=75`) / `6` (`>=65`) / `10` (`<65`), Bonus Target Akhir Musim = `round(baseUpfront * 0.75)`.
- **Auto-Sign AI Deterministik:** `ensureSeasonDeals` mengisi semua 3 slot merek berbeda (RNG seed `teamId * 10007 + seasonId * 997`) untuk seluruh klub AI di awal musim, dan `payWeeklyAll` membayar cicilan mingguan ke semua klub.

---

## 3. Mesin Valuasi Pasar Pemain (`PlayerValuationService`)

Memisahkan secara tegas 4 konsep: **Market Value (MV)**, **Asking Price**, **Negotiated Transfer Fee**, dan **Loan Fee**.

### 3.1 Kurva Nilai Dasar (19 Titik Anchor OVR, Interpolasi Hermite Cubic Smoothstep)
Untuk OVR di-clamp `[40, 99]`, pada segmen $[x_0, x_1]$ hitung $t = (ovr - x_0)/(x_1 - x_0)$, lalu $\text{smooth}(t) = t^2(3 - 2t)$, sehingga $\text{base} = y_0 + \text{smooth}(t)(y_1 - y_0)$:

| OVR | 40 | 45 | 50 | 55 | 60 | 65 | 70 | 73 | 76 | 79 | 82 | 84 | 86 | 88 | 90 | 92 | 94 | 96 | 99 |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| **Base** | 20K | 50K | 100K | 250K | 500K | 1.2M | 3.2M | 5.5M | 10M | 18M | 30M | 42M | 60M | 85M | 115M | 145M | 180M | 210M | 240M |

### 3.2 Pengali Multi-Faktor & Konvergensi Bertahap
$$\text{TargetMV} = \text{roundClean}\Big(\text{clamp}\big(\text{round}(\text{Base} \times M_{\text{age}} \times M_{\text{pot}} \times M_{\text{pos}} \times M_{\text{rep}} \times M_{\text{perf}} \times M_{\text{ctr}}),\; 15\text{K},\; 250\text{M}\big)\Big)$$
1. **Usia ($M_{\text{age}}$):** `<=17: 0.85` | `18–20: 1.15` | `21–24: 1.25` | `25–28: 1.05` | `29–30: 0.90` | `31–32: 0.72` | `33–34: 0.50` | `35–36: 0.35` | `37+: 0.25`.
2. **Potensial ($M_{\text{pot}}$):** Jika `age >= 25 || potential <= power` → `1.0`. Lainnya: `1.0 + min(0.35, (potential - power) * 0.025 * max(0, (25 - age) / 9.0))` (maks `1.35`).
3. **Posisi ($M_{\text{pos}}$):** GK `0.88`, CB/SW `0.93`, FB `0.94`, DM `0.96`, CM/WM `1.00`, AM `1.03`, W `1.05`, SS `1.06`, ST `1.08`.
4. **Reputasi ($M_{\text{rep}}$):** `confMult` (UEFA `1.10`, CONCACAF/CONMEBOL `1.02`, lainnya `0.88`) × `teamMult` (`0.85..1.15` linear untuk teamPower `50..90`), clamp `[0.75, 1.25]`.
5. **Performa ($M_{\text{perf}}$):** Guard sampel `appearances < 5` → `1.0`. Lainnya: `1.0 + clamp(avgRating - 6.8, -1.0, 1.2) * 0.08`, ditambah bonus penyerang/gelandang serang (`pos 6..11`: `(G+A)/apps >= 0.5 → +0.04`, `>= 0.8 → +0.03`) atau kiper/bek tengah (`pos <= 3`: `CS/apps >= 0.40 → +0.04`), clamp `[0.85, 1.15]`.
6. **Kontrak ($M_{\text{ctr}}$):** `<=6m: 0.90`, `7–12m: 0.95`, `13–48m: 1.00`, `>48m: 1.03`.
7. **Pembulatan Bersih (`roundValuationCleanly`):** `>=10M` kelipatan `500K`, `>=1M` kelipatan `100K`, `>=100K` kelipatan `10K`, di bawahnya kelipatan `5K`.
8. **Pembaruan Bertahap (`smoothValueUpdate`):** Langkah perubahan dibatasi maksimal **`±15%`** dari MV lama menuju `TargetMV` agar tidak terjadi inflasi eksponensial majemuk.

### 3.3 Rumus Harga Transfer & Pinjaman
- **Asking Price:** `MV × clamp(contractMult × statusMult × clubFinMult, 0.55, 1.80)` — kontrak `<=6m: 0.65`, `7–12m: 0.85`, `>=36m: 1.20`; transfer-listed `0.85`, untouchable `1.35`; finansial `0.80..1.25`.
- **Negotiated Fee:** `clamp(initialOffer*(1-L) + askingPrice*L, 0.70*MV, 1.50*MV)`.
- **Loan Fee:** `roundClean(max(10_000, round(MV * 0.08 * loanSeasons)))` (8% MV per musim).

---

## 4. Bursa Transfer, Jendela Transfer, Ekonomi Transfer, Pinjaman & Scouting

### 4.1 Jendela Transfer per Negara (`TransferWindowManager`)
Tepat **2 jendela per musim** (`summer` & `winter`). Yang menentukan sah tidaknya pendaftaran adalah **jendela negara klub pembeli (`toTeamId`)**. Status buka/tutup & Deadline Day dihitung murni dari tanggal ISO (`start_date <= date <= end_date`).

| `country_id` | Negara | Summer Window | Winter Window | Free Agent di Luar Jendela |
|---:|---|---|---|---|
| `94` | Indonesia | Jul 01 – Aug 31 | Dec 15 – Jan 15 *(lintas tahun!)* | Ya |
| `44 / 65 / 75` | Inggris / Spanyol / Prancis | Jul 01 – Sep 01 | Jan 01 – Jan 31 | Tidak |
| `106` | Italia | Jul 01 – Sep 01 | Jan 02 – Jan 31 | Tidak |
| `80` | Jerman | Jul 01 – Sep 01 | Jan 01 – Feb 01 | Tidak |
| `10` | Arab Saudi | Jul 01 – Sep 15 | Jan 01 – Jan 30 | Ya |
| *Default* | Negara lain | Jul 01 – Aug 31 | Jan 01 – Jan 31 | Ya |

### 4.2 State Machine Penawaran & Eksekusi Atomik (`TransferRepository`)
```
Offer Submitted (1)
 ├── >= 95% MV ──▶ Club Accepted (2) ──▶ Contract Negotiation (5)
 │                                         ├── salary >= 1.1x current ──▶ Player Accepted (6)
 │                                         │     ├── Window OPEN   ──▶ completeTransfer ──▶ Completed (8)
 │                                         │     └── Window CLOSED ──▶ scheduleTransfer ──▶ AwaitingWindow (9) ──▶ Completed (8) / Cancelled (10) / Failed (11)
 │                                         └── < 1.1x ──▶ kembali Accepted (2) (minta 1.2x)
 ├── 80..94.9% ──▶ Countered (4, counter = 95% MV) ──▶ acceptCounterOffer ──▶ Accepted (2)
 └── < 80%     ──▶ Rejected (3)
```

**Langkah Atomik `completeTransfer` (dalam 1 Transaksi DB):**
1. Cek kuota skuad pembeli `< 30` (`SquadRules.maxSquadSize`, kecuali permanenisasi pemain pinjaman yg sudah berada di klub pembeli).
2. Hitung **Agent Fee pembeli:** `max($50.000, round(fee * 0.05))` (jika `fee > 0`). Verifikasi `buyerBudget >= fee + agentFee`.
3. Pindahkan kepemilikan di `team_players`, reset `market_status = 1`, `squad_status = 1`, `is_captain = 0`, hapus `team_loans` jika ada.
4. **Penyelesaian Klausul Sell-On:** Untuk setiap baris aktif `transfer_clauses(type='sell_on')` milik pemain, hitung `sellOn = round(max(0, fee - originalFee) * clamp(pct, 0.05, 0.25))`. Bayarkan dari hasil penjualan seller ke `beneficiary_team_id` (`sell_on_paid` & `sell_on_received`), tandai klausul `active = 0`.
5. **Buat Klausul Sell-On Baru:** Untuk penjualan permanen (`fromTeamId > 0, fee > 0`), otomatis simpan klausul `sell_on` **`15%`** atas profit penjualan berikutnya untuk `fromTeamId`.
6. Catat seluruh mutasi kas ke `finance_ledger` (`transfer_fees`, `agent_fees`, `player_sales`, `sell_on_*`) + update `teams.budget`.
7. Nonaktifkan `contracts` & `player_registrations` lama (`status = 0`); buat kontrak baru (`duration` default 3 tahun) dan `player_registrations` baru dengan **`annual_amortization = round(fee / durationYears)`** (book value harian = `clamp(fee - round(annualAmort * elapsedDays / 365), 0, fee)`).
8. Catat `player_transfers`, hitung ulang `teams.power` pembeli & penjual, set `offerCompleted (8)`, dan tolak semua penawaran kompetitor yang masih terbuka.

### 4.3 Pinjaman (`completeLoan`) & Auto-Return
- Durasi 1–2 musim (`loanEndYear`: berakhir `endYear-06-30` untuk pinjaman pra-musim, atau `(endYear+seasons)-06-30` untuk tengah musim).
- Pindahkan `team_players` ke peminjam (`market_status = 3`, `squad_status = 2`), catat `loan_fee_paid` & `loan_fee_received`, simpan baris `team_loans`. *(Catatan: saat ini peminjam selalu menanggung 100% gaji; `wage_split_ratio` belum diterapkan di `paySalaries`.)*
- Tidak ada fitur recall manual tengah musim; otomatis dikembalikan di Offseason Tahap 1 (`recallFinishedLoans`).

### 4.4 Departemen Scouting Wonderkid (`ScoutingRepository`)
- **Kapasitas:** Maks **4 scout** aktif (`regions`: South America, Europe, Africa, Asia). 5 kandidat tetap (2★–5★).
- **Biaya Rekrut / Gaji Mingguan:** `1★ $150K/$4K`, `2★ $400K/$10K`, `3★ $900K/$22K`, `4★ $1.8M/$45K`, `5★ $3.5M/$85K`.
- **Penugasan (`deployScout`):** Instan (tanpa tunggu minggu), menghasilkan **`2..3` wonderkid** (atau **`2..4`** jika punya Head Scout ≥4★ dgn peluang 60%), usia `16–18`:
  - `power = clamp(52 + scout.stars*4 + rng(0..4), 55, 82)`
  - `potential = clamp(clamp(72 + scout.stars*4 + headScoutBonus, 72, 95) + rng(0..5), power + 8, 96)`
  - **Diskon Perekrutan (`signWonderkid`):** Biaya rekrut = **`75%` dari MV**, gaji awal `$2.500/mgg`, kontrak 3 tahun.
  - *(Bug WAJIB diperbaiki di Rust: `countryPool` di `scouting_repository.dart` menggunakan ID negara salah `[24,8,41,46]` dan `[97,102,103,105]` alih-alih ID DB asli Brasil=30, Argentina=11, Indonesia=94, Jepang=124, Korsel=49, Australia=14! Juga `signWonderkid` lupa cek `maxSquadSize = 30` dan lupa mengisi `season_id` & `player_registrations`.)*

