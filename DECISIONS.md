# Architecture Decision Records — Naratama Gelanggang (Rust Desktop)

## ADR-001: Desktop UI Framework (Tauri v2)
Status: accepted (2026-11-22)
Context: Game manajer sepak bola didominasi oleh tabel data padat (klasemen, skuad, bursa transfer, ledger keuangan), papan taktik 2D drag-and-drop, dan bagan gugur horizontal.
Decision: Menggunakan **Tauri v2** (backend 100% Rust + frontend web modern) dengan pemisahan total antara engine simulasi (`naratama-model`, `naratama-sim`, `naratama-db`, `naratama-engine`) dan UI (`naratama-desktop`). Di Linux, build UI membutuhkan `libwebkit2gtk-4.1-dev` saat tahap Wave 5; seluruh Wave 0–4 dibangun dan diverifikasi penuh di Rust murni tanpa ketergantungan WebKit.
Consequences: Performa simulasi & SQLite native Rust, UX desktop multi-pane modern.

## ADR-002: Perbaikan 21 Bug Kritis Flutter Asli
Status: accepted (2026-11-22)
Context: Audit kode sumber Flutter di `superkickoff/lib/` menemukan 21 bug nyata (tercatat di `08_RUST_DESKTOP_ARCHITECTURE_AND_PORTING_GUIDE.md`), termasuk ID negara scouting salah, tanggal cedera & ledger memakai jam OS `DateTime.now()`, double-counting statistik timnas, `upsertClubState` menghapus kolom FFP, `makeTransferOffer` mengecek skuad pembeli alih-alih penjual, dan bug `isHomePlayer` untuk pemain pengganti kandang.
Decision: Seluruh 21 bug kritis langsung diperbaiki pada implementasi Rust dan dicegah di level tipe (newtype IDs + `NaiveDate` wajib).
Consequences: Perilaku game lebih akurat dan bebas regresi lama; setiap perbedaan sengaja dari Flutter dicatat di unit test.

## ADR-003: Sistem i18n Bersih (`id` & `en`)
Status: accepted (2026-11-22)
Context: Versi Flutter mencampur Bahasa Indonesia dan Inggris pada layar yang sama.
Decision: Seluruh string antarmuka dipisahkan ke kamus i18n (`id` sebagai default dan `en` sebagai opsi), sementara layer `naratama-sim` dan `naratama-engine` mengembalikan kode/event terstruktur yang diterjemahkan di UI.
Consequences: Backend Rust bersih dari string UI campuran.

## ADR-004: FFP Toggle, Extra Time 30 Menit & Neutral Venue
Status: accepted (2026-11-22)
Context: Di Flutter, sanksi FFP dinonaktifkan, laga gugur seri langsung adu penalti tanpa extra time, dan final/Piala Dunia masih memberi bonus kandang +2.0.
Decision: Menyediakan opsi karier `FfpEnforcement` (default `Off`, pilihan `Relaxed | Standard | Strict`), mengaktifkan simulasi Extra Time 30 menit (`91..=120`) saat `config.extra_time == true`, dan mendukung `neutral_venue: bool` di `MatchEngine`.
Consequences: Realisme pertandingan meningkat dengan tetap menjaga kompatibilitas mode default.

## ADR-005: Target Platform & Alur Checkpoint Wave
Status: accepted (2026-11-22)
Context: Pengembangan dilakukan di Linux dengan target distribusi lintas platform (Linux, Windows `.exe`, macOS).
Decision: Build & tes utama dijalankan di Linux menggunakan `rusqlite` (`bundled` SQLite) dan API cross-platform, dengan laporan checkpoint di setiap akhir wave (Wave 0..5).
Consequences: Zero ketergantungan library sistem SQLite; siap cross-compile.

## ADR-006: Arsip Save `.zip` & FFP Enforcement Aktif
Status: accepted (2026-11-25)
Context: Slot karier hanya hidup di `saves/`, sehingga backup/pemindahan antar mesin tidak mungkin; dan `FfpEnforcement` sebelumnya hanya telemetri tanpa konsekuensi.
Decision: `SessionManager` mengekspor slot ke `<saves>/exports/naratama_slot<N>_<epoch>.zip` (payload `naratama_slot.db`, format SQLite divalidasi saat impor dan hanya menerima arsip di dalam folder `exports`), dan impor menolak menimpa slot yang sedang aktif. Mode FFP disimpan di `finance_settings.mode`; saat mode bukan `off`, offseason mencatat level PSR + sanksi dan `standard`/`strict` memblokir registrasi transfer bila assessor menyatakan `transfer_blocked`.
Consequences: Backup/restore native tanpa dialog file eksternal; FFP punya konsekuensi nyata namun tetap `off` secara default agar paritas MVP aman.

## ADR-007: Tayangan Ulang Laga dari Timeline Tersimpan
Status: accepted (2026-11-25)
Context: PRD meminta pengalaman menonton menit-per-menit (1x/2x/4x/Skip + VAR + adu penalti), tetapi engine Rust belum punya API simulasi bertahap dengan intervensi di tengah laga.
Decision: `DesktopFacade::latest_match_replay` membaca laga user terakhir yang selesai beserta `match_incidents`, lalu UI memutar ulang kejadian itu sebagai timeline (skor berjalan, gol/bunuh diri/penalti, kartu, cedera, VAR, penalti shootout) tanpa mengubah hasil yang sudah tersimpan. Layout pertandingan tetap dihitung penuh di backend sehingga determinisme dan invariant ledger tidak berubah.
Consequences: Pengalaman menonton tersedia tanpa risiko state ganda; fitur interaktif (subs/mentality) tetap dicatat sebagai pekerjaan terpisah karena membutuhkan simulator inkremental.

## ADR-008: LiveMatch Inkremental & Commit Lewat Jalur Matchday Biasa
Status: accepted (2026-11-25)
Context: ADR-007 menunda substitusi/mentality karena engine mensimulasikan 90 menit sekaligus; memindahkan persistensi ke jalur kedua berisiko membuat ledger/statistik tidak konsisten.
Decision: `MatchEngine` dipecah menjadi `LiveMatch` yang menyimpan `SimState`, parameter basis skor, dan jam internal; `advance_to` memutar menit bertahap, `adjust` mengganti pemain di lapangan + mentality lalu menghitung ulang parameter dari XI yang sedang bermain, dan `run_full` tetap identik dengan `simulate` (dibuktikan test paritas). Hasil akhir live match dikirim ke `MatchdayService::simulate_matchday` sebagai `preset_result`, sehingga persistensi, finansial, skorsing, berita, dan housekeeping tetap satu jalur.
Consequences: Interaksi tersedia tanpa menduplikasi logika persistensi; batas 5 substitusi per sisi dan perubahan kekuatan hanya dihitung ulang saat ada penyesuaian (tanpa penyesuaian, hasil identik dengan simulasi lama).

## ADR-009: Operasi Klub Lewat Ledger Tunggal
Status: accepted (2026-11-25)
Context: Flutter mengubah `teams.budget` langsung lalu mencatat ledger terpisah; di Rust `FinanceRepo::record_tx` sudah memindahkan budget, jadi menambah pengurangan manual akan menduplikasi arus kas dan merusak invariant `budget == SUM(finance_ledger.amount)`.
Decision: `ClubOpsService` (kontrak, stadion, tiket, sponsor) hanya menulis `finance_ledger` untuk setiap mutasi kas; `teams.budget` tidak pernah disentuh langsung. Aturan angka mengikuti Flutter (gaji +15%, bonus 2 bulan, tier stadion +10% maksimal, $1.2k/kursi, 3 slot sponsor dengan upfront 1.2M/0.8M/0.5M × skala power).
Consequences: Semua operasi baru otomatis konsisten dengan uji invariant 5 musim; UI menampilkan efek kas tanpa perhitungan ganda.
