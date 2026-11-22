# Domain Glossary — Naratama Gelanggang (Dart → Rust)

| Istilah Asli (Dart/SQLite) | Makna Domain | Nama Kanonik di Rust (`naratama-*`) |
|---|---|---|
| `team_id` / `equid` | ID unik klub | `TeamId(pub i64)` |
| `player_id` / `jugid` | ID unik pemain | `PlayerId(pub i64)` |
| `country_id` / `paid` | ID unik negara / asosiasi nasional | `CountryId(pub i64)` |
| `tournament_id` / `torid` | ID unik turnamen / liga / piala | `TournamentId(pub i64)` |
| `season_id` / `tempid` | ID unik musim | `SeasonId(pub i64)` |
| `match_id` / `partid` | ID unik pertandingan | `MatchId(pub i64)` |
| `budget` (`teams.budget`) | Saldo kas riil klub (bisa negatif; `== SUM(finance_ledger.amount)`) | `Money` (`i64`, 1 unit = $1 USD) |
| `power` (`players.power`) | Rating OVR pemain saat ini (15..99) | `ovr: i32` / `power: i32` |
| `potential` (`players.potential`) | Batas atas perkembangan OVR pemain | `potential: i32` |
| `fitness` (`players.fitness`) | Kebugaran/stamina pemain (0..100) | `fitness: i32` |
| `value` (`players.value`) | Nilai pasar dasar (Market Value / MV) | `market_value: Money` |
| `salary` (`players.salary`) | Gaji mingguan gross (annual = weekly * 52) | `weekly_wage: Money` |
| `profile` (`'D'` / `'I'`) | Kaki dominan (Derecho=Kanan, Izquierdo=Kiri) | `PreferredFoot::Right | Left` |
| `squad_status` (`0/1/2`) | Status di skuad (`Reserve`, `FirstTeam`, `LoanListed`) | `SquadStatus` |
| `market_status` (`1/2/3`) | Status bursa (`Active`, `ForSale`, `OnLoan`) | `MarketStatus` |
| `organization` (`1/2/3/4`) | Jenis turnamen (`League`, `International`, `NationalCup`, `Friendly`) | `TournamentOrg` |
| `competition_type` | Scope skorsing kartu merah (`league`, `domesticCup`, `continental`, `superCup`, `international`, `friendly`) | `CompetitionType` |
| `Once Ideal` | Team of the Year (4-3-3) | `AwardType::TeamOfTheYear` |
| `Cap-tie` (`is_locked = 1`) | Pemain terkunci membela 1 tim nasional setelah tampil di laga resmi non-friendly | `PlayerNationalEligibility::is_locked` |

