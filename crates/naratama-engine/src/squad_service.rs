use chrono::{Datelike, NaiveDate};
use naratama_db::club_repo::{ClubRepo, YouthProspectRow};
use naratama_db::db_err;
use naratama_db::finance_repo::{FinanceRepo, NewLedgerEntry};
use naratama_db::market_repo::MarketRepo;
use naratama_model::{
    ClubFacilities, Confederation, CountryId, LedgerCategory, LedgerPeriod, Money, NaratamaError,
    NaratamaResult, PlayerId, PreferredFoot, SeasonId, StaffRole, TeamId,
};
use naratama_sim::{PlayerValuationInput, PlayerValuationService};
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainingFocus {
    Attacking,
    Defending,
    Fitness,
    Playmaking,
    Goalkeeping,
}

impl TrainingFocus {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Attacking => "attacking",
            Self::Defending => "defending",
            Self::Fitness => "fitness",
            Self::Playmaking => "playmaking",
            Self::Goalkeeping => "goalkeeping",
        }
    }
}

/// Manages squad tactics, training sessions, facilities, staff, stadium upgrades, and youth academies.
pub struct SquadTacticsService;

impl SquadTacticsService {
    pub const TRAINING_COST: Money = 75_000;
    pub const MAX_TRAINING_SESSIONS_PER_MATCH: i32 = 2;
    pub const MAX_PLAYERS_PER_SESSION: usize = 3;
    pub const TRAINING_UPGRADE_COSTS: [Money; 4] = [1_000_000, 2_500_000, 6_000_000, 14_000_000];
    pub const MEDICAL_UPGRADE_COSTS: [Money; 4] = [1_000_000, 2_500_000, 6_000_000, 14_000_000];

    #[allow(clippy::too_many_lines)]
    pub fn train_focus(
        conn: &mut Connection,
        team_id: TeamId,
        season_id: SeasonId,
        focus: TrainingFocus,
        date: NaiveDate,
    ) -> NaratamaResult<Vec<PlayerId>> {
        let match_count = ClubRepo::finished_match_count_for_team(conn, season_id, team_id)?;
        let mut state = ClubRepo::training_state(conn, team_id, season_id)?;
        if state.last_match_count != match_count {
            state.last_match_count = match_count;
            state.sessions = 0;
            state.done_focuses.clear();
        }
        if state.sessions >= Self::MAX_TRAINING_SESSIONS_PER_MATCH {
            return Err(NaratamaError::Validation(
                "Maximum 2 training sessions per match cycle reached".into(),
            ));
        }
        let key = focus.key().to_string();
        if state.done_focuses.contains(&key) {
            return Err(NaratamaError::Validation(
                "Training focus already completed in this match cycle".into(),
            ));
        }

        let budget: i64 = conn
            .query_row(
                "SELECT COALESCE(budget, 0) FROM teams WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or(0);
        if budget < Self::TRAINING_COST {
            return Err(NaratamaError::Validation(
                "Insufficient budget for training session ($75,000)".into(),
            ));
        }

        let pos_filter = match focus {
            TrainingFocus::Goalkeeping => "p.position_id = 1",
            TrainingFocus::Defending => "p.position_id BETWEEN 2 AND 4",
            TrainingFocus::Playmaking => "p.position_id BETWEEN 5 AND 8",
            TrainingFocus::Attacking => "p.position_id >= 9",
            TrainingFocus::Fitness => "p.position_id >= 1",
        };
        let query = format!(
            "SELECT p.player_id, p.power, p.potential, p.age, p.position_id, p.value
             FROM players p
             JOIN team_players tp ON tp.player_id = p.player_id
             WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1
               AND p.power < p.potential AND ({pos_filter})
             ORDER BY (p.potential - p.power) DESC, p.age ASC
             LIMIT ?2"
        );
        let mut stmt = conn.prepare(&query).map_err(db_err)?;
        let targets: Vec<(i64, i32, i32, i32, i32, i64)> = stmt
            .query_map(
                params![
                    team_id.0,
                    i64::try_from(Self::MAX_PLAYERS_PER_SESSION).unwrap_or(3)
                ],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let val_svc = PlayerValuationService::default();
        let tx = conn.transaction().map_err(db_err)?;
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Weekly,
                category: LedgerCategory::FacilityUpkeep,
                subcategory: "training_session",
                amount: -Self::TRAINING_COST,
                ref_type: "training",
                ref_id: Some(season_id.0),
                description: "Squad tactical & skills training session",
            },
        )?;

        let mut improved = Vec::new();
        for (pid, power, potential, age, pos_id, cur_val) in targets {
            if focus == TrainingFocus::Fitness {
                tx.execute(
                    "UPDATE players SET fitness = MIN(100, COALESCE(fitness, 70) + 12) WHERE player_id = ?1",
                    params![pid],
                )
                .map_err(db_err)?;
            } else {
                let new_power = (power + 1).min(potential).min(99);
                let new_val =
                    Self::value_after_ovr(&val_svc, new_power, potential, age, pos_id, cur_val);
                tx.execute(
                    "UPDATE players SET power = ?1, value = ?2 WHERE player_id = ?3",
                    params![new_power, new_val, pid],
                )
                .map_err(db_err)?;
            }
            improved.push(PlayerId(pid));
        }

        state.sessions += 1;
        state.done_focuses.push(key);
        let done_str = state.done_focuses.join(",");
        tx.execute(
            "INSERT INTO team_training (team_id, season_id, last_match_count, sessions, done_focuses)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(team_id) DO UPDATE SET
                season_id = excluded.season_id,
                last_match_count = excluded.last_match_count,
                sessions = excluded.sessions,
                done_focuses = excluded.done_focuses",
            params![
                team_id.0,
                season_id.0,
                state.last_match_count,
                state.sessions,
                done_str
            ],
        )
        .map_err(db_err)?;

        ClubRepo::refresh_team_power_tx(&tx, team_id)?;
        tx.commit().map_err(db_err)?;
        Ok(improved)
    }

    pub fn run_ai_training(
        conn: &mut Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT tm.team_id, COALESCE(tm.budget, 0)
                 FROM season_teams st
                 JOIN teams tm ON tm.team_id = st.team_id
                 WHERE st.season_id = ?1 AND tm.team_id != ?2",
            )
            .map_err(db_err)?;
        let ai_clubs: Vec<(TeamId, Money)> = stmt
            .query_map(params![season_id.0, user_team_id.0], |r| {
                Ok((TeamId(r.get(0)?), r.get(1)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        if ai_clubs.is_empty() {
            return Ok(0);
        }

        let val_svc = PlayerValuationService::default();
        let tx = conn.transaction().map_err(db_err)?;
        let mut improved_count = 0_usize;

        for (team_id, budget) in ai_clubs {
            if budget < Self::TRAINING_COST {
                continue;
            }
            let mut p_stmt = tx
                .prepare(
                    "SELECT p.player_id, p.power, p.potential, p.age, p.position_id, p.value
                     FROM players p
                     JOIN team_players tp ON tp.player_id = p.player_id
                     WHERE tp.team_id = ?1 AND COALESCE(p.status_id, 1) = 1 AND p.power < p.potential
                     ORDER BY (p.potential - p.power) DESC, p.age ASC
                     LIMIT 3",
                )
                .map_err(db_err)?;
            let candidates: Vec<(i64, i32, i32, i32, i32, i64)> = p_stmt
                .query_map(params![team_id.0], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            drop(p_stmt);

            let mut team_improved = false;
            for (pid, power, pot, age, pos_id, cur_val) in candidates {
                // Only small weekly growth chance so squad inflation across 5 seasons remains realistic.
                if rng.gen::<f64>() > 0.15 {
                    continue;
                }
                let new_power = (power + 1).min(pot);
                if new_power <= power {
                    continue;
                }
                let new_val = Self::value_after_ovr(&val_svc, new_power, pot, age, pos_id, cur_val);
                tx.execute(
                    "UPDATE players SET power = ?1, value = ?2 WHERE player_id = ?3",
                    params![new_power, new_val, pid],
                )
                .map_err(db_err)?;
                team_improved = true;
                improved_count += 1;
            }

            if team_improved {
                FinanceRepo::record_tx(
                    &tx,
                    &NewLedgerEntry {
                        team_id,
                        season_id,
                        entry_date: date,
                        period: LedgerPeriod::Weekly,
                        category: LedgerCategory::FacilityUpkeep,
                        subcategory: "ai_training",
                        amount: -Self::TRAINING_COST,
                        ref_type: "ai_training",
                        ref_id: Some(season_id.0),
                        description: "AI weekly training",
                    },
                )?;
                ClubRepo::refresh_team_power_tx(&tx, team_id)?;
            }
        }

        tx.commit().map_err(db_err)?;
        Ok(improved_count)
    }

    pub fn upgrade_training_ground(
        conn: &mut Connection,
        team_id: TeamId,
        season_id: SeasonId,
        date: NaiveDate,
    ) -> NaratamaResult<ClubFacilities> {
        let mut fac = ClubRepo::facilities(conn, team_id)?;
        if fac.training_level >= 5 {
            return Err(NaratamaError::Validation(
                "Training Ground is already at maximum Level 5".into(),
            ));
        }
        let idx = usize::try_from((fac.training_level - 1).max(0)).unwrap_or(0);
        let cost = Self::TRAINING_UPGRADE_COSTS[idx];
        let budget: i64 = conn
            .query_row(
                "SELECT COALESCE(budget, 0) FROM teams WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if budget < cost {
            return Err(NaratamaError::Validation(
                "Insufficient budget for Training Ground upgrade".into(),
            ));
        }

        let tx = conn.transaction().map_err(db_err)?;
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Annual,
                category: LedgerCategory::FacilityUpkeep,
                subcategory: "facility_upgrade",
                amount: -cost,
                ref_type: "facility_upgrade",
                ref_id: Some(team_id.0),
                description: "Training Ground upgrade",
            },
        )?;
        fac.training_level += 1;
        tx.execute(
            "UPDATE team_facilities SET training_level = ?1 WHERE team_id = ?2",
            params![fac.training_level, team_id.0],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(fac)
    }

    pub fn hire_staff(
        conn: &mut Connection,
        team_id: TeamId,
        season_id: SeasonId,
        role: StaffRole,
        name: &str,
        stars: i32,
        date: NaiveDate,
    ) -> NaratamaResult<()> {
        let clamped_stars = stars.clamp(1, 5);
        let hiring_fee: Money = match clamped_stars {
            1 => 50_000,
            2 => 150_000,
            3 => 350_000,
            4 => 750_000,
            _ => 1_500_000,
        };
        let weekly_salary: Money = match clamped_stars {
            1 => 4_000,
            2 => 9_000,
            3 => 18_000,
            4 => 35_000,
            _ => 65_000,
        };
        let budget: i64 = conn
            .query_row(
                "SELECT COALESCE(budget, 0) FROM teams WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if budget < hiring_fee {
            return Err(NaratamaError::Validation(
                "Insufficient club budget for staff signing fee".into(),
            ));
        }

        let date_str = date.format("%Y-%m-%d").to_string();
        let tx = conn.transaction().map_err(db_err)?;
        FinanceRepo::record_tx(
            &tx,
            &NewLedgerEntry {
                team_id,
                season_id,
                entry_date: date,
                period: LedgerPeriod::Annual,
                category: LedgerCategory::StaffWages,
                subcategory: "staff_hire_fee",
                amount: -hiring_fee,
                ref_type: "staff_hire",
                ref_id: Some(team_id.0),
                description: "Coaching/medical staff hiring fee",
            },
        )?;
        tx.execute(
            "INSERT INTO team_staff (team_id, role, name, stars, salary, specialty, hired_date)
             VALUES (?1, ?2, ?3, ?4, ?5, '', ?6)
             ON CONFLICT(team_id, role) DO UPDATE SET
                name = excluded.name,
                stars = excluded.stars,
                salary = excluded.salary,
                hired_date = excluded.hired_date",
            params![
                team_id.0,
                role.as_str(),
                name,
                clamped_stars,
                weekly_salary,
                date_str,
            ],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn graduate_youth_if_due(
        conn: &mut Connection,
        team_id: TeamId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<Vec<YouthProspectRow>> {
        conn.execute(
            "INSERT OR IGNORE INTO team_youth_academy (team_id, level) VALUES (?1, 1)",
            params![team_id.0],
        )
        .map_err(db_err)?;

        let (level, last_date_raw): (i32, Option<String>) = conn
            .query_row(
                "SELECT COALESCE(level, 1), last_graduated_date FROM team_youth_academy WHERE team_id = ?1",
                params![team_id.0],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(db_err)?;

        if let Some(last_str) = last_date_raw {
            if let Ok(last) =
                NaiveDate::parse_from_str(&last_str[..last_str.len().min(10)], "%Y-%m-%d")
            {
                if (date - last).num_days() < 90 {
                    return Ok(Vec::new());
                }
            }
        }

        let country_id: i64 = conn
            .query_row(
                "SELECT country_id FROM teams WHERE team_id = ?1",
                params![team_id.0],
                |r| r.get(0),
            )
            .optional()
            .map_err(db_err)?
            .unwrap_or(1);

        let date_str = date.format("%Y-%m-%d").to_string();
        let intake_count = if level >= 6 { 3 } else { 2 };
        let tx = conn.transaction().map_err(db_err)?;
        let mut created = Vec::with_capacity(intake_count);

        for idx in 0..intake_count {
            let age = rng.gen_range(16..=18);
            let pos = rng.gen_range(1..=11);
            let power = (48 + level * 2 + rng.gen_range(0..=10)).clamp(45, 75);
            let pot = (power + 10 + level * 2 + rng.gen_range(0..=12)).clamp(power + 5, 95);
            let stars = if pot >= 86 {
                5
            } else if pot >= 78 {
                4
            } else {
                3
            };
            let name = format!("Academy Prospect {}{idx}", date.ordinal());
            tx.execute(
                "INSERT INTO academy_prospects (
                    team_id, name, age, position_id, power, potential, stars,
                    profile, height, country_id, created_date, signed
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'D', 178, ?8, ?9, 0)",
                params![team_id.0, name, age, pos, power, pot, stars, country_id, date_str],
            )
            .map_err(db_err)?;
            let pid = tx.last_insert_rowid();
            created.push(YouthProspectRow {
                prospect_id: pid,
                team_id,
                name,
                age,
                position_id: pos,
                power,
                potential: pot,
                stars,
                profile: "D".to_string(),
                height: 178,
                country_id: CountryId(country_id),
                created_date: Some(date),
                signed: false,
            });
        }

        tx.execute(
            "UPDATE team_youth_academy SET last_graduated_date = ?1 WHERE team_id = ?2",
            params![date_str, team_id.0],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(created)
    }

    pub fn sign_academy_prospect(
        conn: &mut Connection,
        season_id: SeasonId,
        prospect: &YouthProspectRow,
        date: NaiveDate,
    ) -> NaratamaResult<PlayerId> {
        let val_svc = PlayerValuationService::default();
        let value = val_svc.calculate_target_market_value(PlayerValuationInput {
            power: prospect.power,
            potential: Some(prospect.potential),
            age: prospect.age,
            position_id: prospect.position_id,
            confederation: Confederation::Uefa,
            team_power: 70,
            contract_months_remaining: 48,
            ..PlayerValuationInput::default()
        });
        let salary = 1_500 + Money::from(prospect.power - 45).max(0) * 150;
        let fee = 25_000;
        MarketRepo::sign_new_prospect_to_club(
            conn,
            season_id,
            prospect.team_id,
            &prospect.name,
            prospect.age,
            prospect.position_id,
            prospect.power,
            prospect.potential,
            prospect.stars,
            PreferredFoot::from_db(&prospect.profile),
            prospect.height,
            prospect.country_id,
            value,
            salary,
            fee,
            date,
            None,
            Some(prospect.prospect_id),
        )
    }

    /// Ensures every registered AI club retains at least 20 active players across multi-season careers.
    pub fn replenish_ai_academies(
        conn: &mut Connection,
        user_team_id: TeamId,
        season_id: SeasonId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<usize> {
        let mut stmt = conn
            .prepare(
                "SELECT st.team_id, tm.country_id, COALESCE(tm.power, 60),
                        (SELECT COUNT(*) FROM team_players tp
                         JOIN players p ON p.player_id = tp.player_id
                         WHERE tp.team_id = st.team_id AND COALESCE(p.status_id, 1) = 1) AS sz
                 FROM season_teams st
                 JOIN teams tm ON tm.team_id = st.team_id
                 WHERE st.season_id = ?1 AND st.team_id != ?2
                 GROUP BY st.team_id
                 HAVING sz < 21",
            )
            .map_err(db_err)?;
        let low_clubs: Vec<(TeamId, CountryId, i32, i64)> = stmt
            .query_map(params![season_id.0, user_team_id.0], |r| {
                Ok((
                    TeamId(r.get(0)?),
                    CountryId(r.get(1)?),
                    r.get(2)?,
                    r.get(3)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let mut promoted = 0_usize;
        for (team_id, country_id, club_power, sz) in low_clubs {
            let needed = usize::try_from((21 - sz).max(0)).unwrap_or(0);
            for i in 0..needed {
                let age = rng.gen_range(17..=19);
                let pos = match (promoted + i) % 4 {
                    0 => 1,
                    1 => rng.gen_range(2..=4),
                    2 => rng.gen_range(5..=8),
                    _ => rng.gen_range(9..=11),
                };
                let power = (club_power - 8 + rng.gen_range(0..=8)).clamp(48, 82);
                let pot = (power + rng.gen_range(6..=16)).clamp(power, 92);
                let val = Money::from(power - 40).max(5) * 35_000;
                let sal = 2_000 + Money::from(power - 45).max(0) * 200;
                MarketRepo::sign_new_prospect_to_club(
                    conn,
                    season_id,
                    team_id,
                    &format!("Youth Regen {}", promoted + 1),
                    age,
                    pos,
                    power,
                    pot,
                    3,
                    PreferredFoot::Right,
                    178,
                    country_id,
                    val,
                    sal,
                    0,
                    date,
                    None,
                    None,
                )?;
                promoted += 1;
            }
        }
        Ok(promoted)
    }

    fn value_after_ovr(
        val_svc: &PlayerValuationService,
        power: i32,
        potential: i32,
        age: i32,
        position_id: i32,
        current_value: Money,
    ) -> Money {
        let target = val_svc.calculate_target_market_value(PlayerValuationInput {
            power,
            potential: Some(potential),
            age,
            position_id,
            confederation: Confederation::Uefa,
            team_power: 72,
            contract_months_remaining: 24,
            ..PlayerValuationInput::default()
        });
        PlayerValuationService::round_valuation_cleanly(val_svc.smooth_value_update(
            current_value,
            target,
            None,
        ))
    }
}
