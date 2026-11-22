use crate::db_err;
use naratama_model::{
    CoachId, Confederation, Country, CountryId, LeagueSummary, NaratamaResult,
    PlayableCountrySummary, Stadium, StadiumId, Team, TeamId, Tournament, TournamentId,
    TournamentOrg,
};
use rusqlite::{params, Connection, OptionalExtension, Row};

/// Read-mostly queries against reference tables (`countries`, `teams`, `tournaments`, `stadiums`).
pub struct ReferenceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ReferenceRepo<'a> {
    #[must_use]
    pub const fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn playable_countries(&self) -> NaratamaResult<Vec<PlayableCountrySummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    c.country_id,
                    c.name,
                    COALESCE(c.iso2, '') AS iso2,
                    COALESCE(c.power, 50) AS power,
                    COALESCE(c.confederation, 0) AS confederation,
                    (SELECT COUNT(*)
                     FROM tournaments tr
                     JOIN tournament_countries tc ON tc.tournament_id = tr.tournament_id
                     WHERE tc.country_id = c.country_id AND tr.organization = 1) AS league_count,
                    (SELECT COUNT(*)
                     FROM teams t
                     WHERE t.country_id = c.country_id AND COALESCE(t.division, 0) > 0) AS team_count
                 FROM countries c
                 WHERE EXISTS (
                     SELECT 1
                     FROM tournaments tr
                     JOIN tournament_countries tc ON tc.tournament_id = tr.tournament_id
                     WHERE tc.country_id = c.country_id AND tr.organization = 1
                 )
                 ORDER BY c.power DESC, c.name ASC",
            )
            .map_err(db_err)?;

        let rows = stmt
            .query_map([], |row| {
                Ok(PlayableCountrySummary {
                    country_id: CountryId(row.get(0)?),
                    name: row.get(1)?,
                    iso2: row.get(2)?,
                    power: row.get(3)?,
                    confederation: Confederation::from_code(row.get(4)?),
                    league_count: row.get(5)?,
                    team_count: row.get(6)?,
                })
            })
            .map_err(db_err)?;

        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn all_countries(&self) -> NaratamaResult<Vec<Country>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT country_id, name, COALESCE(iso2, ''), COALESCE(power, 50),
                        COALESCE(confederation, 0), COALESCE(points, 0.0), previous_rank
                 FROM countries
                 ORDER BY points DESC, power DESC, name ASC",
            )
            .map_err(db_err)?;

        let rows = stmt.query_map([], map_country).map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn country_by_id(&self, id: CountryId) -> NaratamaResult<Option<Country>> {
        self.conn
            .query_row(
                "SELECT country_id, name, COALESCE(iso2, ''), COALESCE(power, 50),
                        COALESCE(confederation, 0), COALESCE(points, 0.0), previous_rank
                 FROM countries
                 WHERE country_id = ?1",
                params![id.get()],
                map_country,
            )
            .optional()
            .map_err(db_err)
    }

    pub fn leagues_for_country(&self, country_id: CountryId) -> NaratamaResult<Vec<LeagueSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    tr.tournament_id,
                    tr.name,
                    COALESCE(tr.division_id, 0),
                    COALESCE(tr.play_days, ''),
                    COALESCE(tr.slots, '0'),
                    (SELECT COUNT(*)
                     FROM teams t
                     WHERE t.country_id = tc.country_id AND t.division = tr.division_id) AS team_count,
                    tr.logo_base64
                 FROM tournaments tr
                 JOIN tournament_countries tc ON tc.tournament_id = tr.tournament_id
                 WHERE tc.country_id = ?1 AND tr.organization = 1
                 ORDER BY tr.division_id ASC, tr.tournament_id ASC",
            )
            .map_err(db_err)?;

        let rows = stmt
            .query_map(params![country_id.get()], map_league_summary)
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn domestic_leagues(&self) -> NaratamaResult<Vec<LeagueSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    tr.tournament_id,
                    tr.name,
                    COALESCE(tr.division_id, 0),
                    COALESCE(tr.play_days, ''),
                    COALESCE(tr.slots, '0'),
                    (SELECT COUNT(*)
                     FROM teams t
                     JOIN tournament_countries tc ON tc.country_id = t.country_id
                     WHERE tc.tournament_id = tr.tournament_id AND t.division = tr.division_id) AS team_count,
                    tr.logo_base64
                 FROM tournaments tr
                 WHERE tr.organization = 1
                 ORDER BY tr.division_id ASC, tr.name ASC",
            )
            .map_err(db_err)?;

        let rows = stmt.query_map([], map_league_summary).map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn national_cup(&self, country_id: CountryId) -> NaratamaResult<Option<Tournament>> {
        self.conn
            .query_row(
                "SELECT
                    tr.tournament_id, tr.name, COALESCE(tr.organization, 3),
                    COALESCE(tr.division_id, 0), COALESCE(tr.slots, '0'),
                    COALESCE(tr.promotion_slots, 0), COALESCE(tr.relegation_spots, 0),
                    COALESCE(tr.points, 1.0), COALESCE(tr.prize_teams, 0),
                    COALESCE(tr.prize_top_scorer, 0), COALESCE(tr.prize_mvp, 0),
                    COALESCE(tr.play_days, ''), COALESCE(tr.rounds, 1), tr.logo_base64
                 FROM tournaments tr
                 JOIN tournament_countries tc ON tc.tournament_id = tr.tournament_id
                 WHERE tc.country_id = ?1 AND tr.organization = 3
                 ORDER BY tr.tournament_id ASC
                 LIMIT 1",
                params![country_id.get()],
                map_tournament,
            )
            .optional()
            .map_err(db_err)
    }

    pub fn international_cups(&self) -> NaratamaResult<Vec<Tournament>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT
                    tournament_id, name, COALESCE(organization, 2),
                    COALESCE(division_id, 0), COALESCE(slots, '0'),
                    COALESCE(promotion_slots, 0), COALESCE(relegation_spots, 0),
                    COALESCE(points, 1.0), COALESCE(prize_teams, 0),
                    COALESCE(prize_top_scorer, 0), COALESCE(prize_mvp, 0),
                    COALESCE(play_days, ''), COALESCE(rounds, 1), logo_base64
                 FROM tournaments
                 WHERE organization = 2
                 ORDER BY tournament_id ASC",
            )
            .map_err(db_err)?;

        let rows = stmt.query_map([], map_tournament).map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn teams_for_division(&self, tournament_id: TournamentId) -> NaratamaResult<Vec<Team>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT t.team_id, t.name, COALESCE(t.abbreviation, ''), COALESCE(t.country_id, 0),
                        COALESCE(t.power, 50), COALESCE(t.value, 0), COALESCE(t.budget, 500000),
                        COALESCE(t.division, 0), COALESCE(t.stadium_id, 0), COALESCE(t.coach_id, 0),
                        COALESCE(t.founded_year, 1900), t.logo_base64
                 FROM teams t
                 JOIN tournaments tr ON tr.tournament_id = ?1
                 JOIN tournament_countries tc
                   ON tc.tournament_id = tr.tournament_id AND tc.country_id = t.country_id
                 WHERE t.division = tr.division_id
                 ORDER BY t.power DESC, t.name ASC",
            )
            .map_err(db_err)?;

        let rows = stmt
            .query_map(params![tournament_id.get()], map_team)
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn teams_for_country(&self, country_id: CountryId) -> NaratamaResult<Vec<Team>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT team_id, name, COALESCE(abbreviation, ''), COALESCE(country_id, 0),
                        COALESCE(power, 50), COALESCE(value, 0), COALESCE(budget, 500000),
                        COALESCE(division, 0), COALESCE(stadium_id, 0), COALESCE(coach_id, 0),
                        COALESCE(founded_year, 1900), logo_base64
                 FROM teams
                 WHERE country_id = ?1
                 ORDER BY division ASC, power DESC, name ASC",
            )
            .map_err(db_err)?;

        let rows = stmt
            .query_map(params![country_id.get()], map_team)
            .map_err(db_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(db_err)
    }

    pub fn team_by_id(&self, id: TeamId) -> NaratamaResult<Option<Team>> {
        self.conn
            .query_row(
                "SELECT team_id, name, COALESCE(abbreviation, ''), COALESCE(country_id, 0),
                        COALESCE(power, 50), COALESCE(value, 0), COALESCE(budget, 500000),
                        COALESCE(division, 0), COALESCE(stadium_id, 0), COALESCE(coach_id, 0),
                        COALESCE(founded_year, 1900), logo_base64
                 FROM teams
                 WHERE team_id = ?1",
                params![id.get()],
                map_team,
            )
            .optional()
            .map_err(db_err)
    }

    pub fn stadium_by_id(&self, id: StadiumId) -> NaratamaResult<Option<Stadium>> {
        self.conn
            .query_row(
                "SELECT stadium_id, name, city, COALESCE(country_id, 0),
                        COALESCE(capacity_general, 0), COALESCE(capacity_stand, 0),
                        COALESCE(capacity_terrace, 0), COALESCE(capacity_boxes, 0),
                        COALESCE(max_general, 0), COALESCE(max_stand, 0),
                        COALESCE(max_terrace, 0), COALESCE(max_boxes, 0),
                        COALESCE(ticket_price_general, 14), COALESCE(ticket_price_stand, 38),
                        COALESCE(ticket_price_terrace, 21), COALESCE(ticket_price_boxes, 120)
                 FROM stadiums
                 WHERE stadium_id = ?1",
                params![id.get()],
                |row| {
                    Ok(Stadium {
                        stadium_id: StadiumId(row.get(0)?),
                        name: row.get(1)?,
                        city: row.get(2)?,
                        country_id: CountryId(row.get(3)?),
                        capacity_general: row.get(4)?,
                        capacity_stand: row.get(5)?,
                        capacity_terrace: row.get(6)?,
                        capacity_boxes: row.get(7)?,
                        max_general: row.get(8)?,
                        max_stand: row.get(9)?,
                        max_terrace: row.get(10)?,
                        max_boxes: row.get(11)?,
                        ticket_price_general: row.get(12)?,
                        ticket_price_stand: row.get(13)?,
                        ticket_price_terrace: row.get(14)?,
                        ticket_price_boxes: row.get(15)?,
                    })
                },
            )
            .optional()
            .map_err(db_err)
    }

    pub fn tournament_by_id(&self, id: TournamentId) -> NaratamaResult<Option<Tournament>> {
        self.conn
            .query_row(
                "SELECT
                    tournament_id, name, COALESCE(organization, 1),
                    COALESCE(division_id, 0), COALESCE(slots, '0'),
                    COALESCE(promotion_slots, 0), COALESCE(relegation_spots, 0),
                    COALESCE(points, 1.0), COALESCE(prize_teams, 0),
                    COALESCE(prize_top_scorer, 0), COALESCE(prize_mvp, 0),
                    COALESCE(play_days, ''), COALESCE(rounds, 1), logo_base64
                 FROM tournaments
                 WHERE tournament_id = ?1",
                params![id.get()],
                map_tournament,
            )
            .optional()
            .map_err(db_err)
    }

    /// Recomputes `teams.power` from the average of each club's top-16 players.
    pub fn refresh_all_team_powers(&self) -> NaratamaResult<usize> {
        let updated = self
            .conn
            .execute(
                "UPDATE teams
                 SET power = COALESCE((
                     SELECT CAST(ROUND(AVG(sub.power)) AS INTEGER)
                     FROM (
                         SELECT p.power
                         FROM players p
                         JOIN team_players tp ON tp.player_id = p.player_id
                         WHERE tp.team_id = teams.team_id AND COALESCE(p.status_id, 1) = 1
                         ORDER BY p.power DESC
                         LIMIT 16
                     ) sub
                 ), power)
                 WHERE EXISTS (
                     SELECT 1
                     FROM team_players tp
                     JOIN players pl ON pl.player_id = tp.player_id
                     WHERE tp.team_id = teams.team_id AND COALESCE(pl.status_id, 1) = 1
                 )",
                [],
            )
            .map_err(db_err)?;
        Ok(updated)
    }
}

fn map_country(row: &Row<'_>) -> rusqlite::Result<Country> {
    Ok(Country {
        country_id: CountryId(row.get(0)?),
        name: row.get(1)?,
        iso2: row.get(2)?,
        power: row.get(3)?,
        confederation: Confederation::from_code(row.get(4)?),
        points: row.get(5)?,
        previous_rank: row.get(6)?,
    })
}

fn map_team(row: &Row<'_>) -> rusqlite::Result<Team> {
    Ok(Team {
        team_id: TeamId(row.get(0)?),
        name: row.get(1)?,
        abbreviation: row.get(2)?,
        country_id: CountryId(row.get(3)?),
        power: row.get(4)?,
        value: row.get(5)?,
        budget: row.get(6)?,
        division: row.get(7)?,
        stadium_id: StadiumId(row.get(8)?),
        coach_id: CoachId(row.get(9)?),
        founded_year: row.get(10)?,
        logo_base64: row.get(11)?,
    })
}

fn map_league_summary(row: &Row<'_>) -> rusqlite::Result<LeagueSummary> {
    let slots_raw: String = row.get(4)?;
    let slots = slots_raw.trim().parse::<i32>().unwrap_or(0);
    Ok(LeagueSummary {
        tournament_id: TournamentId(row.get(0)?),
        name: row.get(1)?,
        division_id: row.get(2)?,
        play_days: row.get(3)?,
        slots,
        team_count: row.get(5)?,
        logo_base64: row.get(6)?,
    })
}

fn map_tournament(row: &Row<'_>) -> rusqlite::Result<Tournament> {
    let slots_raw: String = row.get(4)?;
    let slots = slots_raw.trim().parse::<i32>().unwrap_or(0);
    Ok(Tournament {
        tournament_id: TournamentId(row.get(0)?),
        name: row.get(1)?,
        organization: TournamentOrg::from_code(row.get(2)?),
        division_id: row.get(3)?,
        slots,
        promotion_slots: row.get(5)?,
        relegation_spots: row.get(6)?,
        points: row.get(7)?,
        prize_teams: row.get(8)?,
        prize_top_scorer: row.get(9)?,
        prize_mvp: row.get(10)?,
        play_days: row.get(11)?,
        rounds: row.get(12)?,
        logo_base64: row.get(13)?,
    })
}
