# 07 — Spesifikasi UI/UX Lengkap (26 Layar), Design System & Blueprint Adaptasi Desktop

> **Sumber terverifikasi:** `lib/main.dart`, `lib/state/career_provider.dart` (3.258 baris), `lib/ui/theme.dart`, `lib/ui/widgets/{common,custom_club_preview_card,finance_widgets}.dart`, dan seluruh 26 layar di `lib/ui/screens/*.dart`.

---

## 1. Identitas Visual & Design System

> **Koreksi Penting:** Tema utama aplikasi di `lib/ui/theme.dart` adalah **Deep Midnight Navy + Championship Gold + Royal Blue** (selaras dengan logo perisai Naratama Gelanggang). Warna hijau zamrud (`#166534` / `#14532D`) hanya dipakai untuk **kanvas lapangan sepak bola 2D** di Taktik, Team Detail, dan Gala TOTY.

### 1.1 Palet Warna Kanonik
| Token | Hex | Kegunaan |
|---|---|---|
| `surface` | `#070D18` | Latar belakang utama aplikasi, AppBar, Navigasi, modal |
| `surfaceCard` | `#0F1728` | Kartu, input field, chip, header tabel |
| `surfaceCardHigh` | `#162035` | Permukaan elevasi, pill anggaran, kartu bersarang |
| `border` / `divider` | `#23304A` / `#1E293B` | Garis tepi tombol/input, pemisah baris |
| `accent` (Gold) | `#E5A93C` | Tombol aksi utama (CTA), tab aktif, highlight klub user, OVR |
| `accentLight` / `accentDim` | `#FBBF24` / `#85530E` | Aksen emas terang & highlight baris klasemen/jadwal user |
| `onPrimary` | `#1A1002` | Teks gelap di atas tombol emas |
| `galaGold` | `#FFD700` | Gala Penghargaan, Ballon d'Or, Team of the Year |
| `royalBlue` / `Light` | `#1D4ED8` / `#3B82F6` | Aksen sekunder, hak siar, badge bek, tinta tanda tangan |
| `success` / `warn` / `danger` | `#22C55E` / `#F59E0B` / `#EF4444` | Positif/Naik/Sehat · Peringatan/Kartu Kuning · Negatif/Cedera/Skorsing/Degradasi |
| `textDim` | `#94A3B8` | Label sekunder, metadata, header tabel |
| `intlBlue` | `#1E88E5` / `#64B5F6` | Badge `TIMNAS`, banner panggilan timnas |
| `pitchGreen` | `#166534` → `#14532D` | Kanvas lapangan 2D (Gala: `#0F3B20` → `#134E27`) |
| `contractPaper` | `#FDF8EC` | Kertas kontrak animasi `SigningScreen` |

### 1.2 Kode Warna Posisi, OVR, Rating & Stamina
- **Badge Posisi:** GK `#F59E0B` (Amber) · DEF (`CB/LB/RB`) `#3B82F6` (Biru) · MID (`CDM/CM/CAM`) `#22C55E` (Hijau) · ATT (`LW/RW/ST/CF`) `#EF4444` (Merah).
- **OVR Badge:** `>=85` Amber · `>=78` Gold · `>=70` Biru · `>=60` Putih 70% · `<60` Merah.
- **Rating Pertandingan:** `>=7.0` Gold · `6.0–6.9` Putih · `<6.0` Merah (laga timnas: `>=7.5` Gold).
- **Fitness Bar:** `>70` Hijau · `41–70` Oranye · `<=40` Merah.
- **Format Angka & Tanggal:** `tabularFigures` wajib pada skor, jam, anggaran, dan tabel. Mata uang `$X.XB / $X.XM / $XK / $X` *(perbaiki bug di `CustomClubScreen` yang sempat memakai simbol `€`)*.
- **Cache Aset & Bendera (Desktop Rust):**
  - Logo klub/liga base64 wajib memakai **LRU cache terikat** (mis. 450–600 entri) agar tidak mendecode ulang saat scroll.
  - Di Flutter, bendera negara mengambil PNG daring dari `flagcdn.com` dengan fallback emoji. Untuk **Rust Desktop 100% offline**, bundel aset bendera SVG/PNG lokal (termasuk 4 bendera khusus Britania: `gb-eng`, `gb-sct`, `gb-wls`, `gb-nir`).
- **Lokalisasi (i18n):** Kode Flutter asli mencampur teks Bahasa Indonesia dan Inggris pada layar yang sama. Di Rust Desktop, pisahkan seluruh string ke berkas kamus i18n bersih (`id.json` & `en.json`).

---

## 2. Peta Navigasi & Katalog Fungsional 26 Layar

### 2.1 Entry, Save Slots & Onboarding (4 Layar)
1. **`StartScreen` (`start_screen.dart`):** Logo Naratama Gelanggang, CTA *Continue Career*, *Load Game / Save Slots*, *New Career*, dan *Create Your Own Club* (emas).
2. **`SaveSlotSelectorSheet` (`save_slot_selector_sheet.dart`):** 5 slot karier (mode `load` vs `newCareer` dengan dialog konfirmasi hapus/timpa, menampilkan klub, nama manajer, musim, tanggal terakhir).
3. **`SetupScreen` (`setup_screen.dart`):** Wizard 4 langkah dengan progress bar (*Profile* → *Country* → *Division* → *Club*, termasuk shortcut ke Custom Club) + layar loading progres generasi dunia.
4. **`CustomClubScreen` + `CustomClubPreviewCard`:** Wizard 5 langkah (*Manager* → *Identity* → *Stadium* → *Kits & Crest* → *Starting Squad*) dengan live canvas preview perisai (4 bentuk, 5 simbol, 3 huruf) dan 2 jersey Home/Away (4 pola, 10 palet warna).

### 2.2 Dashboard & 4 Tab Utama (9 Layar)
5. **`HomeScreen` + `OverviewTab` (`home_screen.dart`):** Shell utama (AppBar dengan crest klub, Inbox badge, Bracket picker, Leave Career Save/Discard dialog; Busy overlay scrim; takeover penuh ke `OffseasonView` saat offseason). Overview memiliki 2 sub-tab (`Ringkasan` | `Berita`). Ringkasan menampilkan: Banner Annual Awards Gala, Banner Persetujuan Panggilan Timnas Uji Coba, Hero Card (*Pre-Season Orientation* ATAU *Next Match* dgn tombol `Play Match`, `Advance Week`/`Simulate Week`, `Analisis Taktis Lawan`), Club Summary 4-chip + tombol `Club Hub`, `MiniStandingsCard` (top 5 + posisi user), kartu Inbox terbaru, dan Berita Dunia terbaru.
6. **`NewsTab` (`news_tab.dart`):** Feed berita 3 scope (`Dunia` | `Region` | `Liga`), 15 badge kategori warna, lead story `importance >= 4/5`, klik membuka artikel penuh.
7. **`SquadTab` (`squad_tab.dart`):** 3 sub-tab:
   - *Squad:* Sortir (`Overall`, `MV`, `Age`, `Position` asc/desc), badge status (`Pemain Inti (XI)`, `SUSP N`, `CEDERA · N hari`, `TIMNAS`, jual/pinjam), fitness bar, menu aksi (toggle starter, kapten, daftar jual, daftar pinjam).
   - *Statistics:* Profil skuad, `DataTable` lengkap (`APP, MIN, G, A, MVP, YC, RC, RTG`), dan 3 papan pemimpin klub.
   - *Formation:* Dropdown 6 formasi, `Auto XI`, `Clear`, `Train`, **lapangan 2D interaktif** (drag-and-drop antar slot otomatis swap) + zona bangku cadangan (drop untuk melepas starter).
8. **`CompetitionTab` (`competition_tab.dart`):** Kontainer 3 sub-tab (`Klasemen` | `Jadwal` | `Timnas`).
9. **`StandingsTab` (`standings_tab.dart`):** Dropdown seluruh liga domestik & piala + tombol Riwayat Musim. 5 sub-tab (`Table/Bracket`, `Scorers`, `Assists`, `Clean sheets`, `Rating`). Tabel liga dilengkapi indikator zona kualifikasi per konfederasi (Eropa Top-5, Eropa lain, Asia, Amerika Selatan/Utara), promosi/degradasi, dan panah naik/turun peringkat.
10. **`FixturesTab` (`fixtures_tab.dart`):** Kalender musim klub dikelompokkan per ronde/kompetisi, menggabungkan 2-leg knockout jadi satu kartu agregat + penalti, auto-scroll ke tanggal aktif.
11. **`InternationalTab` (`international_tab.dart`):** 4 sub-tab pil:
    - *Jadwal & Tugas:* Daftar pemain klub yang sedang membela timnas + jadwal 3 jendela FIFA musim ini.
    - *Turnamen:* Filter 6 turnamen (World Cup, EURO, Asian Cup, Nations League, FIFA ASEAN Cup, Friendlies), kartu Podium 🥇🥈🥉, bracket gugur horizontal lengkap (termasuk 3rd-place playoff), klasemen grup, daftar laga, dan `_CountryMatchDetailSheet` (lineup + rating pemain 2 tab).
    - *Ranking FIFA:* Filter konfederasi + pencarian, peringkat + delta `▲/▼`, poin Elo & OVR.
    - *Skuad Timnas:* Pemilih negara searchable, 3 mode (*Skuad Terpilih 23*, *Semua Pemain Eligible*, *Rekor Top Skor/Assist/Caps/Rating*).
12. **`MarketTab` (`market_tab.dart`):** Header budget + Filter lanjutan (Usia, Harga, Negara) + `_TransferWindowBanner` (Hijau Terbuka / Amber Deadline Day / Merah Tutup). 5 sub-tab: *Listed players*, *All Players*, *Terjadwal* (bisa dibatalkan), *Scouting* (rekrut/pecat maks 4 scout, kirim ke 4 region, review & sign wonderkid 75% MV), *Transfer history*.
13. **`InboxScreen` (`inbox_tab.dart`):** Daftar pesan + routing tombol aksi kontekstual (Tawaran Manajer, Tawaran Transfer/Pinjam/Counter/Kontrak, Intake Akademi, Panggilan Timnas Uji Coba, Permintaan Kenaikan Gaji).

### 2.3 Pengalaman Pertandingan & Analisis (4 Layar)
14. **`MatchScreen` + `ConfettiOverlay` (`match_screen.dart`):**
    > **Realitas Penting Implementasi:** Pertandingan **telah disimulasikan penuh** sebelum layar dibuka; `MatchScreen` memutar ulang timeline menit `0'→90'` (`Timer 400ms / speed`, kecepatan `1x / 2x / 4x` atau *To end*), memperbarui skor & memproses VAR rollback secara real-time, menampilkan daftar tendangan adu penalti di `90'`, serta **Confetti 60 partikel + banner Juara** pada laga Final/Title Decider. Belum ada substitusi/taktik live manual (di versi Desktop Rust bisa dipertahankan sebagai *fast replay* sekaligus opsi *interactive halftime/live subs* di fase lanjut).
15. **`MatchStatsSheet` (`match_stats_sheet.dart`):** Perbandingan bar 2 warna (Possession, Shots, On Target, Accuracy, Saves, YC, RC) + daftar rating semua pemain (MVP star, menit, gol/assist/kartu).
16. **`MatchdayResultsSheet` (`matchday_results_sheet.dart`):** Ringkasan hasil satu matchday (laga user dipin di paling atas, agregat Leg 2 & penalti).
17. **`TacticalAnalysisSheet` (`tactical_analysis_sheet.dart`):** Dossier intel lawan: atribusi Analis Taktis & persen keyakinan, formasi/gaya main/mentalitas lawan, 2 Pemain Kunci, Celah Kelemahan, Rekomendasi Kontra-Taktik, dan Prediksi Laga.

### 2.4 Manajemen Klub, Detail & Bracket (5 Layar)
18. **`ClubHubSheet` + `FinanceWidgets` (`club_hub_sheet.dart`):** 6 tab (*Overview*, *Finances* [Cash Health, Season P&L, Wage Bill + Top 5 Earners, Recent Ledger + Full History 100 baris, Sponsor 3 slot & Manage Deals], *Stadium* [4 tier kapasitas, edit 4 harga tiket, ekspansi +10%], *Facilities* [Training Ground Lv1–5, Medical Centre Lv1–5, rekrut/ganti/pecat 3 peran staf], *Academy* [Lv1–10, intake prospek], *Honours* [Banner Gala, Kabinet Trofi, Penghargaan Pemain]).
19. **`PlayerDetailSheet` + `ProspectDetailSheet`:** Profil 4 tab (*Profile* termasuk karier Timnas, *Contract* + tombol perpanjang kontrak +15%/2× bonus, *Stats*, *History*) + tombol cepat *Transfer List* / *Loan List* untuk pemain sendiri.
20. **`TeamDetailSheet` (`team_detail_sheet.dart`):** Inspektor klub universal 4 tab (*Formasi & XI* di lapangan 2D + cadangan, *Skuad* dgn filter posisi, *Jadwal* + rekor M-S-K, *Info Klub* stadion/pelatih/trofi/budget) + strip kekuatan lini `ATT/MID/DEF/GK`.
21. **`TrainingSheet` (`training_sheet.dart`):** 5 fokus latihan, status fokus selesai hari ini (`✓`), info level Training Ground, biaya `$75K`, tombol mulai sesi.
22. **`KnockoutBracketSheet` (`knockout_bracket_sheet.dart`):** Bracket horizontal piala domestik/kontinental dengan kartu 2-leg + agregat + kolom Champion.

### 2.5 Offseason, Gala, Sejarah & Tawaran Manajer (4 Layar)
23. **`OffseasonView` (`offseason_screen.dart`):** Layar wizard 7 tahap dengan progress node 1..7, kartu laporan semantik berkode warna/ikon (`+`/`-`/trofi/uang/warning), pembaruan kontrak massal, tab *Market & Squad* tertanam di tahap 4 (+1 Week / Skip to deadline), 3 kartu Camp pra-musim, 3 kartu Ambisi Direksi.
24. **`AnnualAwardsGalaSheet` (`annual_awards_gala_sheet.dart`):** Seremoni obsidian & emas 3 tab (*Individual Awards* Ballon d'Or/Golden Boot/Glove/Young/MVP/Manager, *Team of the Year* lapangan 2D 4-3-3, *Club Honours*).
25. **`SeasonHistoryScreen` (`season_history_sheet.dart`):** Arsip semua musim (dropdown Musim + Kompetisi) dengan toggle *Klasemen* vs *Daftar Juara (Roll of Honour)* (Domestik, Internasional Klub, Timnas).
26. **`ManagerOfferSheet` + `SigningScreen` (`manager_offer_sheet.dart`):** Dialog tawaran klub baru (bintang prestise, target, budget, kontrak) → animasi 3.6 detik tanda tangan pena tinta biru di atas kertas kontrak krem + stempel merah **`APPROVED`** elastis.

---

## 3. Cetak Biru Adaptasi Mobile → Desktop Multi-Pane Workspace

Saat menulis ulang ke Desktop Rust, **jangan menyalin mentah-mentah Bottom Navigation Bar dan Bottom Sheet 90% tinggi** — ubah menjadi antarmuka *desktop football manager* yang lega, cepat dinavigasi dengan mouse & keyboard shortcut:

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ [Logo/Crest] Persib · Season 2025/26 · 14 Sep 2025 │ Cash: $18.4M (Healthy)      │
│ [Window: OPEN 12d] │ [Mail (3)] [Save]             │  [ ▶ PLAY MATCH / ADVANCE ] │
├──────────────┬───────────────────────────────────────────────┬───────────────────┤
│ LEFT SIDEBAR │              MAIN WORKSPACE                   │  RIGHT INSPECTOR  │
│              │                                               │  (Slide-over /    │
│ • Dashboard  │  Konten utama multi-kolom / DataTable padat   │   Master-Detail)  │
│ • Squad      │  (mis: Papan Taktik 2D kiri + Roster kanan;   │                   │
│ • Kompetisi  │   atau Broadcast Match: Event Feed kiri +     │  Detail Pemain /  │
│ • Timnas     │   Live Stats & Ratings kanan secara bersamaan)│  Detail Klub /    │
│ • Market     │                                               │  Intel Lawan      │
│ • Club Hub   │                                               │  tanpa menutup    │
│ • Inbox (3)  │                                               │  tabel utama!     │
│ • News       │                                               │                   │
│ • History    │                                               │                   │
└──────────────┴───────────────────────────────────────────────┴───────────────────┘
```

| Pola Mobile Flutter | Adaptasi Desktop Native Rust |
|---|---|
| 4 Tab Bottom Nav + ikon AppBar | **Persistent Left Sidebar** (8 rute utama: Dashboard, Squad & Tactics, Competitions, International Hub, Transfer Market & Scouting, Club Hub, Inbox, News/History). |
| AppBar sempit | **Top Command Bar:** Crest klub, Musim, Tanggal in-game, Saldo Kas + status kesehatan, status Jendela Transfer, dan tombol aksi utama global **`Play Match` / `Advance Week` / `Simulate`** (`Space` shortcut). |
| `ClubHubSheet` (modal 90%, 6 tab) | Rute sidebar penuh; tab Keuangan memakai **grid 2 kolom** (Kiri: Health + P&L + Wage Bill; Kanan: Ledger searchable/filterable + Sponsor). |
| `PlayerDetailSheet` & `TeamDetailSheet` | **Right-Hand Inspector Panel** (split master-detail) agar klik baris di tabel Skuad/Market/Klasemen langsung menampilkan profil di kanan tanpa menutup tabel. |
| `SquadTab` (Pitch di atas Bench, `LongPressDraggable`) | **Two-Pane Tactics Board:** Kiri = Lapangan 2D penuh (dukung **klik-untuk-tukar** & **drag langsung tanpa long-press**); Kanan = Daftar Skuad/Cadangan sortable + tombol Training & Auto XI. |
| `MarketTab` (`_FilterDrawer` modal) | **Left Filter Rail permanen** (Usia, Harga, Negara, Posisi, Free Agent) berdampingan dengan `DataTable` sortable padat + panel penawaran cepat. |
| `MatchScreen` + `MatchStatsSheet` terpisah | **Split Broadcast Match View:** Atas = Scoreboard + Progress + Speed (`1x/2x/4x/Skip`); Kiri = Timeline Event menit-per-menit; Kanan = Live/FT Statistik 2-warna & Rating Pemain berdampingan. |
| `KnockoutBracketSheet` / Bracket Timnas | **Zoomable / Pannable Canvas Bracket** (dukung drag mouse & scroll zoom) untuk melihat bagan 32-tim Piala Dunia sekaligus. |
| `OffseasonView` / `AnnualAwardsGala` | **Centered Workspace Modal / Stepper** dengan layout 2-kolom (tahap Transfer langsung menampilkan Market + Squad berdampingan; Gala menampilkan Awards + Lapangan 4-3-3 berdampingan). |

