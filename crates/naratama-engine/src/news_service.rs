use chrono::NaiveDate;
use naratama_db::db_err;
use naratama_model::{
    Confederation, MatchStatus, NaratamaResult, PlayerId, SeasonId, TeamId, TournamentId,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItemRow {
    pub news_id: i64,
    pub ref_key: String,
    pub season_id: SeasonId,
    pub date: String,
    pub scope: String,
    pub category: String,
    pub headline: String,
    pub body: String,
    pub importance: i32,
    pub team_id: Option<TeamId>,
    pub player_id: Option<PlayerId>,
}

/// Deterministic matchday, transfer, and title-race story generator (idempotent via unique `ref`).
pub struct NewsEngine;

impl NewsEngine {
    #[must_use]
    pub const fn region_for_confederation(conf: Confederation) -> &'static str {
        match conf {
            Confederation::Uefa => "europe",
            Confederation::Conmebol => "southAmerica",
            Confederation::Concacaf => "northAmerica",
            Confederation::Caf => "africa",
            Confederation::Afc | Confederation::Ofc | Confederation::None => "asia",
        }
    }

    /// Generates season-opening preview headlines so the news feed is populated on Day 1.
    pub fn preview(
        conn: &Connection,
        season_id: SeasonId,
        date: NaiveDate,
        user_league_id: TournamentId,
        user_region: &str,
    ) -> NaratamaResult<usize> {
        let mut stmt = conn
            .prepare(
                "SELECT tm.team_id, tm.name, COALESCE(tm.power, 60)
                 FROM teams tm
                 WHERE tm.team_id IN (
                     SELECT home_team_id FROM matches WHERE season_id = ?1 AND tournament_id = ?2
                 )
                 ORDER BY tm.power DESC, tm.name ASC
                 LIMIT 6",
            )
            .map_err(db_err)?;
        let favorites: Vec<(i64, String, i32)> = stmt
            .query_map(params![season_id.0, user_league_id.0], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        if favorites.len() < 2 {
            return Ok(0);
        }
        let date_str = date.format("%Y-%m-%d").to_string();
        let league_name: String = conn
            .query_row(
                "SELECT name FROM tournaments WHERE tournament_id = ?1",
                params![user_league_id.0],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "League".to_string());

        let mut inserted = 0_usize;
        inserted += insert_news_if_absent(
            conn,
            &format!("preview:league:{}:{}", season_id.0, user_league_id.0),
            season_id,
            &date_str,
            "league",
            "preview",
            &format!("{league_name} season preview: {} lead title favourites", favorites[0].1),
            &format!(
                "Analysts tip {} (OVR {}) and {} (OVR {}) to set the pace in the upcoming {league_name} campaign.",
                favorites[0].1, favorites[0].2, favorites[1].1, favorites[1].2
            ),
            4,
            Some(TeamId(favorites[0].0)),
            None,
        )?;
        inserted += insert_news_if_absent(
            conn,
            &format!("preview:region:{}:{user_region}", season_id.0),
            season_id,
            &date_str,
            "region",
            "preview",
            &format!("Continental spotlight: {user_region} clubs brace for new campaign"),
            &format!(
                "Clubs across {user_region} enter the new season with reshaped squads and continental ambitions."
            ),
            3,
            None,
            None,
        )?;
        Ok(inserted)
    }

    /// Detects matchday upsets, goal-fests, hat-tricks, and big transfers on `date`.
    #[allow(clippy::too_many_lines)]
    pub fn on_matchday(
        conn: &Connection,
        season_id: SeasonId,
        date: NaiveDate,
        user_league_id: TournamentId,
        _user_region: &str,
    ) -> NaratamaResult<usize> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let mut stmt = conn
            .prepare(
                "SELECT m.match_id, m.tournament_id, m.home_team_id, m.away_team_id,
                        COALESCE(m.home_goals, 0), COALESCE(m.away_goals, 0),
                        ht.name, at.name, COALESCE(ht.power, 60), COALESCE(at.power, 60),
                        COALESCE(tu.name, 'Competition')
                 FROM matches m
                 JOIN teams ht ON ht.team_id = m.home_team_id
                 JOIN teams at ON at.team_id = m.away_team_id
                 JOIN tournaments tu ON tu.tournament_id = m.tournament_id
                 WHERE m.season_id = ?1 AND m.date = ?2 AND m.status = ?3",
            )
            .map_err(db_err)?;

        #[allow(clippy::type_complexity)]
        let matches: Vec<(
            i64,
            i64,
            i64,
            i64,
            i32,
            i32,
            String,
            String,
            i32,
            i32,
            String,
        )> = stmt
            .query_map(
                params![season_id.0, date_str, MatchStatus::Finished as i64],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                    ))
                },
            )
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        drop(stmt);

        let mut inserted = 0_usize;
        for (mid, tid, hid, aid, hg, ag, hname, aname, hpwr, apwr, tname) in matches {
            let scope = if tid == user_league_id.0 {
                "league"
            } else {
                "world"
            };
            // Giant killing / upset (underdog >= 10 OVR weaker wins)
            if hg > ag && apwr - hpwr >= 10 {
                inserted += insert_news_if_absent(
                    conn,
                    &format!("upset:{mid}"),
                    season_id,
                    &date_str,
                    scope,
                    "upset",
                    &format!("Shock in {tname}: {hname} stun {aname} {hg}-{ag}"),
                    &format!(
                        "Underdogs {hname} defied the odds to defeat {aname} in a dramatic {tname} clash."
                    ),
                    4,
                    Some(TeamId(hid)),
                    None,
                )?;
            } else if ag > hg && hpwr - apwr >= 10 {
                inserted += insert_news_if_absent(
                    conn,
                    &format!("upset:{mid}"),
                    season_id,
                    &date_str,
                    scope,
                    "upset",
                    &format!("Away upset in {tname}: {aname} beat {hname} {ag}-{hg}"),
                    &format!(
                        "{aname} pulled off a famous away victory over favourites {hname} in {tname}."
                    ),
                    4,
                    Some(TeamId(aid)),
                    None,
                )?;
            }

            // Goal thriller (>= 6 total goals)
            if hg + ag >= 6 {
                inserted += insert_news_if_absent(
                    conn,
                    &format!("thriller:{mid}"),
                    season_id,
                    &date_str,
                    scope,
                    "result",
                    &format!("Goal fest: {hname} {hg}-{ag} {aname}"),
                    &format!(
                        "Fans witnessed {} goals as {hname} and {aname} traded blows in {tname}.",
                        hg + ag
                    ),
                    3,
                    Some(TeamId(hid)),
                    None,
                )?;
            }
        }

        // Hat-tricks on date
        let mut ht_stmt = conn
            .prepare(
                "SELECT mp.match_id, mp.player_id, mp.team_id, mp.goals, p.name, t.name
                 FROM match_players mp
                 JOIN matches m ON m.match_id = mp.match_id
                 JOIN players p ON p.player_id = mp.player_id
                 JOIN teams t ON t.team_id = mp.team_id
                 WHERE m.season_id = ?1 AND m.date = ?2 AND mp.goals >= 3
                 LIMIT 5",
            )
            .map_err(db_err)?;
        let hattricks: Vec<(i64, i64, i64, i32, String, String)> = ht_stmt
            .query_map(params![season_id.0, date_str], |r| {
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
        drop(ht_stmt);

        for (mid, pid, tid, goals, pname, tname) in hattricks {
            inserted += insert_news_if_absent(
                conn,
                &format!("hattrick:{mid}:{pid}"),
                season_id,
                &date_str,
                "world",
                "hattrick",
                &format!("{pname} nets {goals} goals for {tname}"),
                &format!(
                    "Clinical finishing from {pname} powered {tname} with a {goals}-goal haul."
                ),
                4,
                Some(TeamId(tid)),
                Some(PlayerId(pid)),
            )?;
        }

        // Big transfers on date (>= $5M)
        let mut tr_stmt = conn
            .prepare(
                "SELECT pt.transfer_id, pt.player_id, pt.to_team_id, pt.value, p.name, t.name
                 FROM player_transfers pt
                 JOIN players p ON p.player_id = pt.player_id
                 JOIN teams t ON t.team_id = pt.to_team_id
                 WHERE pt.date = ?1 AND pt.value >= 5000000
                 LIMIT 5",
            )
            .map_err(db_err)?;
        let transfers: Vec<(i64, i64, i64, i64, String, String)> = tr_stmt
            .query_map(params![date_str], |r| {
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
        drop(tr_stmt);

        for (tr_id, pid, to_tid, fee, pname, tname) in transfers {
            #[allow(clippy::cast_precision_loss)]
            let fee_m = fee as f64 / 1_000_000.0;
            inserted += insert_news_if_absent(
                conn,
                &format!("transfer:{tr_id}"),
                season_id,
                &date_str,
                "world",
                "transfer",
                &format!("DONE DEAL: {tname} sign {pname} for ${fee_m:.1}M"),
                &format!("{tname} have completed the marquee signing of {pname}."),
                4,
                Some(TeamId(to_tid)),
                Some(PlayerId(pid)),
            )?;
        }

        Ok(inserted)
    }

    pub fn feed(
        conn: &Connection,
        scope: Option<&str>,
        limit: usize,
    ) -> NaratamaResult<Vec<NewsItemRow>> {
        let lim = i64::try_from(limit).unwrap_or(50);
        let mut out = Vec::new();
        if let Some(sc) = scope {
            let mut stmt = conn
                .prepare(
                    "SELECT news_id, ref, season_id, date, scope, category, headline, body,
                            COALESCE(importance, 3), team_id, player_id
                     FROM news WHERE scope = ?1
                     ORDER BY date DESC, importance DESC, news_id DESC
                     LIMIT ?2",
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map(params![sc, lim], map_news_row)
                .map_err(db_err)?;
            for r in rows {
                out.push(r.map_err(db_err)?);
            }
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT news_id, ref, season_id, date, scope, category, headline, body,
                            COALESCE(importance, 3), team_id, player_id
                     FROM news
                     ORDER BY date DESC, importance DESC, news_id DESC
                     LIMIT ?1",
                )
                .map_err(db_err)?;
            let rows = stmt.query_map(params![lim], map_news_row).map_err(db_err)?;
            for r in rows {
                out.push(r.map_err(db_err)?);
            }
        }
        Ok(out)
    }
}

/// Derives fan mood (`0..=100`) and generates club-life atmosphere stories around the user's club.
pub struct ClubLifeNews;

impl ClubLifeNews {
    pub fn fan_mood(
        conn: &Connection,
        season_id: SeasonId,
        team_id: TeamId,
        recent_n: usize,
    ) -> NaratamaResult<i32> {
        let form = recent_form(conn, season_id, team_id, recent_n)?;
        if form.is_empty() {
            return Ok(60);
        }
        let mut mood = 55_i32;
        for (gf, ga) in form {
            mood += match gf.cmp(&ga) {
                std::cmp::Ordering::Greater => 8,
                std::cmp::Ordering::Equal => -1,
                std::cmp::Ordering::Less => -9,
            };
        }
        Ok(mood.clamp(0, 100))
    }

    pub fn on_matchday(
        conn: &Connection,
        season_id: SeasonId,
        date: NaiveDate,
        user_team_id: TeamId,
        user_team_name: &str,
    ) -> NaratamaResult<usize> {
        let form = recent_form(conn, season_id, user_team_id, 6)?;
        if form.is_empty() {
            return Ok(0);
        }
        let mood = Self::fan_mood(conn, season_id, user_team_id, 6)?;
        let win_run = form.iter().take_while(|&&(gf, ga)| gf > ga).count();
        let loss_run = form.iter().take_while(|&&(gf, ga)| gf < ga).count();
        let date_str = date.format("%Y-%m-%d").to_string();

        let mut inserted = 0_usize;
        if win_run >= 3 {
            inserted += insert_news_if_absent(
                conn,
                &format!("life:fans_joy:{}:{date_str}", season_id.0),
                season_id,
                &date_str,
                "league",
                "fans",
                &format!("Euphoria among {user_team_name} fans after {win_run} straight wins"),
                &format!("Supporter mood climbs to {mood}/100 with home tickets selling fast."),
                4,
                Some(user_team_id),
                None,
            )?;
        } else if loss_run >= 3 {
            inserted += insert_news_if_absent(
                conn,
                &format!("life:fans_anger:{}:{date_str}", season_id.0),
                season_id,
                &date_str,
                "league",
                "fans",
                &format!("{user_team_name} fans frustrated after {loss_run} consecutive defeats"),
                &format!("Supporter mood drops to {mood}/100 as pressure mounts on matchday."),
                4,
                Some(user_team_id),
                None,
            )?;
        }
        Ok(inserted)
    }
}

fn recent_form(
    conn: &Connection,
    season_id: SeasonId,
    team_id: TeamId,
    n: usize,
) -> NaratamaResult<Vec<(i32, i32)>> {
    let lim = i64::try_from(n).unwrap_or(6);
    let mut stmt = conn
        .prepare(
            "SELECT home_team_id, COALESCE(home_goals, 0), COALESCE(away_goals, 0)
             FROM matches
             WHERE season_id = ?1 AND status = ?2
               AND (home_team_id = ?3 OR away_team_id = ?3)
             ORDER BY date DESC, match_id DESC
             LIMIT ?4",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map(
            params![season_id.0, MatchStatus::Finished as i64, team_id.0, lim],
            |r| {
                let hid: i64 = r.get(0)?;
                let hg: i32 = r.get(1)?;
                let ag: i32 = r.get(2)?;
                Ok(if hid == team_id.0 { (hg, ag) } else { (ag, hg) })
            },
        )
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    Ok(rows)
}

#[allow(clippy::too_many_arguments)]
fn insert_news_if_absent(
    conn: &Connection,
    ref_key: &str,
    season_id: SeasonId,
    date_str: &str,
    scope: &str,
    category: &str,
    headline: &str,
    body: &str,
    importance: i32,
    team_id: Option<TeamId>,
    player_id: Option<PlayerId>,
) -> NaratamaResult<usize> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM news WHERE ref = ?1",
            params![ref_key],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    if count > 0 {
        return Ok(0);
    }
    conn.execute(
        "INSERT INTO news (ref, season_id, date, scope, category, headline, body, importance, team_id, player_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            ref_key,
            season_id.0,
            date_str,
            scope,
            category,
            headline,
            body,
            importance,
            team_id.map(|t| t.0),
            player_id.map(|p| p.0),
        ],
    )
    .map_err(db_err)?;
    Ok(1)
}

fn map_news_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NewsItemRow> {
    Ok(NewsItemRow {
        news_id: row.get(0)?,
        ref_key: row.get(1)?,
        season_id: SeasonId(row.get(2)?),
        date: row.get(3)?,
        scope: row.get(4)?,
        category: row.get(5)?,
        headline: row.get(6)?,
        body: row.get(7)?,
        importance: row.get(8)?,
        team_id: row.get::<_, Option<i64>>(9)?.map(TeamId),
        player_id: row.get::<_, Option<i64>>(10)?.map(PlayerId),
    })
}
