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

