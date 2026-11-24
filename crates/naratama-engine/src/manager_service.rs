use chrono::NaiveDate;
use naratama_db::club_repo::ClubRepo;
use naratama_db::db_err;
use naratama_db::reference_repo::ReferenceRepo;
use naratama_model::{Money, NaratamaError, NaratamaResult, PlayerId, SeasonId, TeamId};
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i64)]
pub enum ManagerOfferStatus {
    Pending = 0,
    Accepted = 1,
    Declined = 2,
    Completed = 3,
    Expired = 4,
}

impl ManagerOfferStatus {
    #[must_use]
    pub const fn from_code(code: i64) -> Self {
        match code {
            1 => Self::Accepted,
            2 => Self::Declined,
            3 => Self::Completed,
            4 => Self::Expired,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerOfferRow {
    pub offer_id: i64,
    pub team_id: TeamId,
    pub season_id: SeasonId,
    pub date: String,
    pub status: ManagerOfferStatus,
    pub club_name: String,
    pub league_name: String,
    pub target: String,
    pub contract_years: i32,
    pub budget: Money,
    pub power: i32,
    pub from_power: i32,
}

impl ManagerOfferRow {
    #[must_use]
    pub fn prestige_stars(&self) -> i32 {
        let diff = (self.power - self.from_power).max(1);
        ((diff + 3) / 4).clamp(1, 5)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessageRow {
    pub message_id: i64,
    pub title: String,
    pub body: String,
    pub date: String,
    pub sender: String,
    pub is_read: bool,
    pub msg_type: i32,
    pub player_id: Option<PlayerId>,
    pub team_id: Option<TeamId>,
    pub transfer_offer_id: Option<i64>,
}

/// Manages job offers from stronger clubs when the manager performs well.
pub struct ManagerOfferService;

impl ManagerOfferService {
    pub const MESSAGE_TYPE_OFFER: i32 = 77;

    pub fn maybe_generate_offer(
        conn: &Connection,
        season_id: SeasonId,
        user_team_id: TeamId,
        date: NaiveDate,
        rng: &mut impl Rng,
    ) -> NaratamaResult<Option<ManagerOfferRow>> {
        // Only one pending or accepted offer at a time
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM manager_offers WHERE status IN (0, 1)",
                [],
                |r| r.get(0),
            )
            .map_err(db_err)?;
        if active > 0 {
            return Ok(None);
        }

        let Some(current_team) = ReferenceRepo::new(conn).team_by_id(user_team_id)? else {
            return Ok(None);
        };

        let mut stmt = conn
            .prepare(
                "SELECT t.team_id, t.name, COALESCE(t.budget, 5000000), COALESCE(t.power, 70),
                        COALESCE((
                            SELECT tr.name FROM tournaments tr
                            JOIN tournament_countries tc ON tc.tournament_id = tr.tournament_id
                            WHERE tc.country_id = t.country_id AND tr.organization = 1 AND tr.division_id = t.division
                            LIMIT 1
                        ), 'Domestic League')
                 FROM teams t
                 WHERE t.team_id != ?1 AND t.division = 1
                   AND COALESCE(t.power, 60) >= ?2
                   AND COALESCE(t.power, 60) <= ?3
                 ORDER BY t.power DESC
                 LIMIT 20",
            )
            .map_err(db_err)?;
        let candidates: Vec<(i64, String, Money, i32, String)> = stmt
            .query_map(
                params![
                    user_team_id.0,
                    current_team.power + 2,
                    (current_team.power + 16).min(95)
                ],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        if candidates.is_empty() {
            return Ok(None);
        }

        let (tid, cname, budget, power, lname) = &candidates[rng.gen_range(0..candidates.len())];
        let date_str = date.format("%Y-%m-%d").to_string();
        let years = rng.gen_range(2..=4);
        let target = if *power >= 78 {
            "Title Challenge"
        } else {
            "Top 4 Finish"
        };

        conn.execute(
            "INSERT INTO manager_offers (
                team_id, season_id, date, status, club_name, league_name,
                target, contract_years, budget, power, from_power
             ) VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                tid,
                season_id.0,
                date_str,
                cname,
                lname,
                target,
                years,
                budget,
                power,
                current_team.power,
            ],
        )
        .map_err(db_err)?;
        let offer_id = conn.last_insert_rowid();

        let _ = ClubRepo::insert_message(
            conn,
            &format!("Managerial Offer from {cname}"),
            &format!(
                "{cname} ({lname}) have offered you a {years}-year contract. Objective: {target}."
            ),
            date,
            cname,
            Some(TeamId(*tid)),
            Some(offer_id),
        )?;

        Ok(Some(ManagerOfferRow {
            offer_id,
            team_id: TeamId(*tid),
            season_id,
            date: date_str,
            status: ManagerOfferStatus::Pending,
            club_name: cname.clone(),
            league_name: lname.clone(),
            target: target.to_string(),
            contract_years: years,
            budget: *budget,
            power: *power,
            from_power: current_team.power,
        }))
    }

    pub fn respond_to_offer(
        conn: &Connection,
        offer_id: i64,
        accept: bool,
    ) -> NaratamaResult<ManagerOfferStatus> {
        let next = if accept {
            ManagerOfferStatus::Accepted
        } else {
            ManagerOfferStatus::Declined
        };
        let updated = conn
            .execute(
                "UPDATE manager_offers SET status = ?1 WHERE offer_id = ?2 AND status = 0",
                params![next as i64, offer_id],
            )
            .map_err(db_err)?;
        if updated == 0 {
            return Err(NaratamaError::NotFound(format!(
                "Pending manager offer {offer_id} not found"
            )));
        }
        Ok(next)
    }

    pub fn accepted_move(conn: &Connection) -> NaratamaResult<Option<ManagerOfferRow>> {
        conn.query_row(
            "SELECT offer_id, team_id, season_id, date, status, club_name, league_name,
                    target, contract_years, budget, power, from_power
             FROM manager_offers WHERE status = 1 ORDER BY offer_id DESC LIMIT 1",
            [],
            map_offer_row,
        )
        .optional()
        .map_err(db_err)
    }

    pub fn complete_accepted_move(conn: &Connection, offer_id: i64) -> NaratamaResult<()> {
        conn.execute(
            "UPDATE manager_offers SET status = 3 WHERE offer_id = ?1",
            params![offer_id],
        )
        .map_err(db_err)?;
        Ok(())
    }
}

/// Manages manager inbox messages (read/unread badge & filtering).
pub struct InboxService;

impl InboxService {
    pub fn list_messages(conn: &Connection, limit: usize) -> NaratamaResult<Vec<InboxMessageRow>> {
        let lim = i64::try_from(limit).unwrap_or(50);
        let mut stmt = conn
            .prepare(
                "SELECT message_id, COALESCE(title, ''), COALESCE(body, ''),
                        COALESCE(date, ''), COALESCE(sender, 'Club'),
                        COALESCE(is_read, 0), COALESCE(type, 0),
                        player_id, team_id, transfer_offer_id
                 FROM messages
                 ORDER BY message_id DESC
                 LIMIT ?1",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map(params![lim], |r| {
                Ok(InboxMessageRow {
                    message_id: r.get(0)?,
                    title: r.get(1)?,
                    body: r.get(2)?,
                    date: r.get(3)?,
                    sender: r.get(4)?,
                    is_read: r.get::<_, i32>(5)? != 0,
                    msg_type: r.get(6)?,
                    player_id: r.get::<_, Option<i64>>(7)?.map(PlayerId),
                    team_id: r.get::<_, Option<i64>>(8)?.map(TeamId),
                    transfer_offer_id: r.get(9)?,
                })
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn unread_count(conn: &Connection) -> NaratamaResult<i64> {
        conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE COALESCE(is_read, 0) = 0",
            [],
            |r| r.get(0),
        )
        .map_err(db_err)
    }

    pub fn mark_read(conn: &Connection, message_id: i64) -> NaratamaResult<()> {
        conn.execute(
            "UPDATE messages SET is_read = 1 WHERE message_id = ?1",
            params![message_id],
        )
        .map_err(db_err)?;
        Ok(())
    }

    pub fn mark_all_read(conn: &Connection) -> NaratamaResult<()> {
        conn.execute(
            "UPDATE messages SET is_read = 1 WHERE COALESCE(is_read, 0) = 0",
            [],
        )
        .map_err(db_err)?;
        Ok(())
    }
}

fn map_offer_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ManagerOfferRow> {
    Ok(ManagerOfferRow {
        offer_id: r.get(0)?,
        team_id: TeamId(r.get(1)?),
        season_id: SeasonId(r.get(2)?),
        date: r.get(3)?,
        status: ManagerOfferStatus::from_code(r.get(4)?),
        club_name: r.get(5)?,
        league_name: r.get(6)?,
        target: r.get(7)?,
        contract_years: r.get(8)?,
        budget: r.get(9)?,
        power: r.get(10)?,
        from_power: r.get(11)?,
    })
}
