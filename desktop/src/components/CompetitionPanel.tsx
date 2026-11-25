import { useEffect, useState } from 'react';
import {
  CompetitionOverviewDto,
  desktopApi,
  FixtureRowDto,
  GroupTableDto,
  logoSrc,
  StandingRowDto,
} from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface CompetitionPanelProps {
  locale: Locale;
  userTeamId: number;
  onOpenMatch: (matchId: number) => void;
  onOpenTeam: (teamId: number) => void;
}

type Tab = 'table' | 'fixtures' | 'bracket' | 'stats';

function StandingsTable({
  rows,
  userTeamId,
  onOpenTeam,
  logos,
}: {
  rows: StandingRowDto[];
  userTeamId: number;
  onOpenTeam: (teamId: number) => void;
  logos: Map<number, string>;
}) {
  const t = getDictionary('id');
  return (
    <table className="data-table tabular">
      <thead>
        <tr>
          <th>#</th>
          <th>{t.common.club}</th>
          <th>{t.common.played}</th>
          <th>{t.common.won}</th>
          <th>{t.common.drawn}</th>
          <th>{t.common.lost}</th>
          <th>{t.common.gf}</th>
          <th>{t.common.ga}</th>
          <th>{t.common.gd}</th>
          <th>{t.common.points}</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row, idx) => (
          <tr key={row.team_id} className={row.team_id === userTeamId ? 'user-row' : ''}>
            <td>{idx + 1}</td>
            <td
              style={{ fontWeight: 600, cursor: 'pointer' }}
              onClick={() => onOpenTeam(row.team_id)}
            >
              {logoSrc(logos.get(row.team_id)) && (
                <img
                  src={logoSrc(logos.get(row.team_id)) ?? ''}
                  alt=""
                  style={{ width: 16, height: 16, marginRight: 6, verticalAlign: 'middle' }}
                />
              )}
              {row.team_name}
            </td>
            <td>{row.played}</td>
            <td>{row.wins}</td>
            <td>{row.draws}</td>
            <td>{row.losses}</td>
            <td>{row.goals_for}</td>
            <td>{row.goals_against}</td>
            <td>{row.goals_for - row.goals_against}</td>
            <td style={{ fontWeight: 800, color: 'var(--accent-light)' }}>{row.points}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function FixtureList({
  fixtures,
  userTeamId,
  onOpenMatch,
  onOpenTeam,
}: {
  fixtures: FixtureRowDto[];
  userTeamId: number;
  onOpenMatch: (matchId: number) => void;
  onOpenTeam: (teamId: number) => void;
}) {
  const t = getDictionary('id');
  return (
    <table className="data-table tabular">
      <thead>
        <tr>
          <th>{t.competition.date}</th>
          <th>{t.competition.home}</th>
          <th style={{ textAlign: 'center' }}>{t.competition.score}</th>
          <th>{t.competition.away}</th>
        </tr>
      </thead>
      <tbody>
        {fixtures.map((f) => {
          const done = f.home_goals !== null && f.away_goals !== null && f.status === 2;
          const mine = f.home_team_id === userTeamId || f.away_team_id === userTeamId;
          return (
            <tr
              key={f.match_id}
              className={mine ? 'user-row' : ''}
              style={{ cursor: done ? 'pointer' : 'default' }}
              onClick={() => done && onOpenMatch(f.match_id)}
            >
              <td style={{ whiteSpace: 'nowrap' }}>{f.date ?? '—'}</td>
              <td
                style={{ fontWeight: 600 }}
                onClick={(e) => {
                  e.stopPropagation();
                  onOpenTeam(f.home_team_id);
                }}
              >
                {f.home_name}
              </td>
              <td style={{ textAlign: 'center', fontWeight: 800 }}>
                {done ? `${f.home_goals} - ${f.away_goals}` : 'vs'}
                {f.penalty_home_goals !== null &&
                  f.penalty_away_goals !== null &&
                  (f.penalty_home_goals > 0 || f.penalty_away_goals > 0) && (
                    <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>
                      {' '}
                      ({f.penalty_home_goals}-{f.penalty_away_goals})
                    </span>
                  )}
              </td>
              <td
                style={{ fontWeight: 600 }}
                onClick={(e) => {
                  e.stopPropagation();
                  onOpenTeam(f.away_team_id);
                }}
              >
                {f.away_name}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

export function CompetitionPanel({
  locale,
  userTeamId,
  onOpenMatch,
  onOpenTeam,
}: CompetitionPanelProps) {
  const t = getDictionary(locale);
  const [tournaments, setTournaments] = useState<[number, string, number][]>([]);
  const [tournamentId, setTournamentId] = useState<number>(0);
  const [overview, setOverview] = useState<CompetitionOverviewDto | null>(null);
  const [tab, setTab] = useState<Tab>('table');
  const [logos, setLogos] = useState<Map<number, string>>(new Map());

  useEffect(() => {
    void desktopApi.listSeasonTournaments().then((list) => {
      setTournaments(list);
      if (list.length > 0) setTournamentId((cur) => (cur === 0 ? list[0][0] : cur));
    });
  }, []);

  useEffect(() => {
    if (!tournamentId) return;
    let active = true;
    void desktopApi.getCompetitionOverview(tournamentId).then((data) => {
      if (!active) return;
      setOverview(data);
      if (!data) return;
      const ids = Array.from(
        new Set([
          ...data.standings.map((r) => r.team_id),
          ...data.groups.flatMap((g) => g.rows.map((r) => r.team_id)),
        ]),
      );
      void desktopApi.getLogoMap(ids).then((pairs) => setLogos(new Map(pairs)));
    });
    return () => {
      active = false;
    };
  }, [tournamentId]);

  const renderGroup = (group: GroupTableDto) => (
    <div key={group.group_id} style={{ marginBottom: 14 }}>
      <div style={{ fontSize: 12, fontWeight: 700, marginBottom: 4, color: 'var(--accent)' }}>
        {t.competition.group} {group.group_id}
      </div>
      <StandingsTable
        rows={group.rows}
        userTeamId={userTeamId}
        onOpenTeam={onOpenTeam}
        logos={logos}
      />
    </div>
  );

  return (
    <section className="card">
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 12, flexWrap: 'wrap' }}>
        <select
          className="select"
          value={tournamentId}
          onChange={(e) => setTournamentId(Number(e.target.value))}
        >
          {tournaments.map(([id, name, org]) => (
            <option key={id} value={id}>
              {name} {org >= 3 ? `· ${t.competition.international}` : ''}
            </option>
          ))}
        </select>
        <div style={{ display: 'flex', gap: 6, marginLeft: 'auto' }}>
          {(
            [
              ['table', t.competition.standings],
              ['fixtures', t.competition.fixtures],
              ['bracket', t.competition.bracket],
              ['stats', t.competition.stats],
            ] as [Tab, string][]
          ).map(([key, label]) => (
            <button
              key={key}
              className={`btn ${tab === key ? 'btn-primary' : ''}`}
              onClick={() => setTab(key)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {!overview && <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.common.loading}</div>}

      {overview && tab === 'table' && (
        <>
          {overview.groups.length > 0
            ? overview.groups.map(renderGroup)
            : overview.standings.length > 0
              ? (
                  <StandingsTable
                    rows={overview.standings}
                    userTeamId={userTeamId}
                    onOpenTeam={onOpenTeam}
                    logos={logos}
                  />
                )
              : <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.competition.noTable}</div>}
        </>
      )}

      {overview && tab === 'fixtures' && (
        <FixtureList
          fixtures={overview.fixtures}
          userTeamId={userTeamId}
          onOpenMatch={onOpenMatch}
          onOpenTeam={onOpenTeam}
        />
      )}

      {overview && tab === 'bracket' && (
        <>
          {overview.bracket.length === 0 && (
            <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.competition.noBracket}</div>
          )}
          <div style={{ display: 'flex', gap: 12, overflowX: 'auto' }}>
            {overview.bracket.map((round) => (
              <div key={round.phase} style={{ minWidth: 210, flex: 1 }}>
                <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--accent)', marginBottom: 6 }}>
                  {round.name}
                </div>
                {round.fixtures.map((f) => (
                  <div
                    key={f.match_id}
                    className="news-item"
                    style={{ display: 'flex', justifyContent: 'space-between', gap: 6, cursor: 'pointer' }}
                    onClick={() => f.home_goals !== null && onOpenMatch(f.match_id)}
                  >
                    <span style={{ fontSize: 12, fontWeight: f.winner_id === f.home_team_id ? 800 : 500 }}>
                      {f.home_name}
                    </span>
                    <span className="tabular" style={{ fontSize: 12 }}>
                      {f.home_goals ?? '-'} : {f.away_goals ?? '-'}
                      {f.leg > 1 ? ` (L${f.leg})` : ''}
                    </span>
                    <span style={{ fontSize: 12, fontWeight: f.winner_id === f.away_team_id ? 800 : 500 }}>
                      {f.away_name}
                    </span>
                  </div>
                ))}
              </div>
            ))}
          </div>
        </>
      )}

      {overview && tab === 'stats' && (
        <div className="grid-2">
          {(
            [
              [t.competition.topScorers, overview.top_scorers],
              [t.competition.topAssists, overview.top_assists],
              [t.competition.topRated, overview.top_rated],
            ] as [string, typeof overview.top_scorers][]
          ).map(([label, rows]) => (
            <div key={label}>
              <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--accent)', marginBottom: 6 }}>
                {label}
              </div>
              {rows.length === 0 && (
                <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>{t.competition.noData}</div>
              )}
              {rows.map((row, idx) => (
                <div key={`${row.player_id}-${idx}`} className="news-item" style={{ display: 'flex', gap: 8 }}>
                  <span className="tabular" style={{ width: 20, color: 'var(--text-dim)' }}>
                    {idx + 1}
                  </span>
                  <span style={{ fontWeight: 600, fontSize: 12.5 }}>{row.player_name}</span>
                  <span style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>{row.team_name}</span>
                  <span className="tabular" style={{ marginLeft: 'auto', fontWeight: 700 }}>
                    {label === t.competition.topRated ? (row.value / 10).toFixed(1) : row.value}
                  </span>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
