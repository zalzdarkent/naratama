use std::collections::BTreeSet;

use chrono::{Duration, NaiveDate};
use naratama_db::season_repo::SeasonRepo;
use naratama_model::{
    InternationalWindow, InternationalWindowType, NaratamaError, NaratamaResult, SeasonId,
};
use rusqlite::Connection;

/// Generates and queries the 3 canonical FIFA international windows per season.
pub struct InternationalCalendarService;

impl InternationalCalendarService {
    pub fn generate_windows_for_season(
        conn: &Connection,
        season_id: SeasonId,
        start_year: i32,
    ) -> NaratamaResult<Vec<InternationalWindow>> {
        let d = |y: i32, m: u32, day: u32| -> NaratamaResult<NaiveDate> {
            NaiveDate::from_ymd_opt(y, m, day)
                .ok_or_else(|| NaratamaError::InvalidState(format!("invalid date {y}-{m}-{day}")))
        };

        let windows = vec![
            InternationalWindow {
                window_id: format!("win_{season_id}_oct"),
                season_id,
                name: format!("FIFA Window (October {start_year})"),
                window_type: InternationalWindowType::FifaWindow,
                start_date: d(start_year, 10, 6)?,
                end_date: d(start_year, 10, 14)?,
                release_start_date: d(start_year, 10, 5)?,
                return_date: d(start_year, 10, 15)?,
                is_mandatory_release: true,
                max_matches: 2,
                club_fixtures_allowed: false,
            },
            InternationalWindow {
                window_id: format!("win_{season_id}_mar"),
                season_id,
                name: format!("FIFA Window (March {})", start_year + 1),
                window_type: InternationalWindowType::FifaWindow,
                start_date: d(start_year + 1, 3, 23)?,
                end_date: d(start_year + 1, 3, 31)?,
                release_start_date: d(start_year + 1, 3, 22)?,
                return_date: d(start_year + 1, 4, 1)?,
                is_mandatory_release: true,
                max_matches: 2,
                club_fixtures_allowed: false,
            },
            InternationalWindow {
                window_id: format!("win_{season_id}_summer"),
                season_id,
                name: format!("Summer Tournament Window ({})", start_year + 1),
                window_type: InternationalWindowType::FinalTournament,
                start_date: d(start_year + 1, 6, 11)?,
                end_date: d(start_year + 1, 7, 19)?,
                release_start_date: d(start_year + 1, 6, 1)?,
                return_date: d(start_year + 1, 7, 20)?,
                is_mandatory_release: true,
                max_matches: 7,
                club_fixtures_allowed: false,
            },
        ];

        SeasonRepo::new(conn).upsert_international_windows(&windows)?;
        Ok(windows)
    }

    pub fn mandatory_unavailable_dates(
        conn: &Connection,
        season_id: SeasonId,
    ) -> NaratamaResult<Vec<NaiveDate>> {
        let windows = SeasonRepo::new(conn).windows_for_season(season_id)?;
        let mut dates = BTreeSet::new();
        for w in windows {
            if !w.is_mandatory_release && w.club_fixtures_allowed {
                continue;
            }
            let mut cur = w.release_start_date;
            while cur <= w.return_date {
                dates.insert(cur);
                cur += Duration::days(1);
            }
        }
        Ok(dates.into_iter().collect())
    }
}
