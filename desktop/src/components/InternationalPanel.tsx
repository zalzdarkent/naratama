import { useEffect, useMemo, useState } from 'react';
import {
  countryFlag,
  CountryRankDto,
  desktopApi,
  DutyRequestDto,
  HonourRowDto,
  InternationalScheduleDto,
  NationalLeaderDto,
  NationalSquadPlayerDto,
  positionMeta,
} from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface InternationalPanelProps {
  locale: Locale;
  rankings: CountryRankDto[];
  notify: (message: string) => void;
}

type IntlTab = 'fixtures' | 'rankings' | 'podiums' | 'squads';

const CONFEDS = [
  ['all', 'ALL'],
  ['Uefa', 'UEFA'],
  ['Conmebol', 'CONMEBOL'],
  ['Afc', 'AFC'],
  ['Caf', 'CAF'],
  ['Concacaf', 'CONCACAF'],
  ['Ofc', 'OFC'],
] as const;

export function InternationalPanel({ locale, rankings, notify }: InternationalPanelProps) {
  const t = getDictionary(locale);
  const [tab, setTab] = useState<IntlTab>('fixtures');
  const [schedule, setSchedule] = useState<InternationalScheduleDto | null>(null);
  const [podiums, setPodiums] = useState<HonourRowDto[]>([]);
  const [duties, setDuties] = useState<DutyRequestDto[]>([]);
  const [countries, setCountries] = useState<[number, string][]>([]);
  const [countryId, setCountryId] = useState<number>(0);
  const [squad, setSquad] = useState<NationalSquadPlayerDto[]>([]);
  const [leaders, setLeaders] = useState<NationalLeaderDto[]>([]);

  // Fixtures filters
  const [tournamentFilter, setTournamentFilter] = useState<number>(0);
  const [statusFilter, setStatusFilter] = useState<'all' | 'upcoming' | 'finished'>('all');
  const [matchSearch, setMatchSearch] = useState('');

  // Rankings filters
  const [rankSearch, setRankSearch] = useState('');
  const [confedFilter, setConfedFilter] = useState<string>('all');

  const load = async () => {
    const [sched, p, d, c] = await Promise.all([
      desktopApi.getInternationalSchedule(),
      desktopApi.getInternationalPodiums(),
      desktopApi.getDutyRequests(),
      desktopApi.getCalledUpCountries(),
    ]);
    setSchedule(sched);
    setPodiums(p);
    setDuties(d);
    void desktopApi.getNationalLeaders().then(setLeaders).catch(() => setLeaders([]));
    setCountries(c);
    setCountryId((cur) => (cur === 0 && c.length > 0 ? c[0][0] : cur));
  };

  useEffect(() => {
    void load().catch((e) => notify(String(e)));
  }, []);

  useEffect(() => {
    if (!countryId) return;
    void desktopApi.getNationalSquad(countryId).then(setSquad).catch(() => setSquad([]));
  }, [countryId]);

  const isoByCountryId = useMemo(() => {
    const map = new Map<number, { iso2: string; name: string }>();
    for (const c of rankings) {
      map.set(c.country_id, { iso2: c.iso2, name: c.name });
    }
    return map;
  }, [rankings]);

  const rankedList = useMemo(() => {
    const q = rankSearch.trim().toLowerCase();
    return rankings
      .map((c, idx) => ({ ...c, rank: idx + 1 }))
      .filter((c) => {
        if (confedFilter !== 'all' && (c.confederation ?? '') !== confedFilter) return false;
        if (q && !c.name.toLowerCase().includes(q) && !c.iso2.toLowerCase().includes(q)) {
          return false;
        }
        return true;
      });
  }, [rankings, rankSearch, confedFilter]);

  const filteredMatches = useMemo(() => {
    const all = schedule?.matches ?? [];
    const q = matchSearch.trim().toLowerCase();
    return all.filter((m) => {
      if (tournamentFilter !== 0 && m.tournament_id !== tournamentFilter) return false;
      if (statusFilter === 'upcoming' && m.status === 2) return false;
      if (statusFilter === 'finished' && m.status !== 2) return false;
      if (
        q &&
        !m.home_name.toLowerCase().includes(q) &&
        !m.away_name.toLowerCase().includes(q) &&
        !m.tournament_name.toLowerCase().includes(q)
      ) {
        return false;
      }
      return true;
    });
  }, [schedule, tournamentFilter, statusFilter, matchSearch]);

  const intlGroups = useMemo(() => {
    if (tournamentFilter === 0) return [];
    const byGroup = new Map<number, typeof filteredMatches>();
    for (const m of schedule?.matches ?? []) {
      if (m.tournament_id !== tournamentFilter || m.group_id === null) continue;
      const list = byGroup.get(m.group_id) ?? [];
      list.push(m);
      byGroup.set(m.group_id, list);
    }
    return Array.from(byGroup.entries())
      .sort(([a], [b]) => a - b)
      .map(([groupId, ms]) => {
        const table = new Map<
          number,
          { id: number; name: string; iso2: string; p: number; w: number; d: number; l: number; gf: number; ga: number; pts: number }
        >();
        const ensure = (id: number, name: string, iso2: string) => {
          let row = table.get(id);
          if (!row) {
            row = { id, name, iso2, p: 0, w: 0, d: 0, l: 0, gf: 0, ga: 0, pts: 0 };
            table.set(id, row);
          }
          return row;
        };
        for (const m of ms) {
          const h = ensure(m.home_country_id, m.home_name, m.home_iso2);
          const a = ensure(m.away_country_id, m.away_name, m.away_iso2);
          if (m.status === 2 && m.home_goals !== null && m.away_goals !== null) {
            h.p += 1;
            a.p += 1;
            h.gf += m.home_goals;
            h.ga += m.away_goals;
            a.gf += m.away_goals;
            a.ga += m.home_goals;
            if (m.home_goals > m.away_goals) {
              h.w += 1;
              h.pts += 3;
              a.l += 1;
            } else if (m.away_goals > m.home_goals) {
              a.w += 1;
              a.pts += 3;
              h.l += 1;
            } else {
              h.d += 1;
              a.d += 1;
              h.pts += 1;
              a.pts += 1;
            }
          }
        }
        const rows = Array.from(table.values()).sort(
          (x, y) => y.pts - x.pts || (y.gf - y.ga) - (x.gf - x.ga) || y.gf - x.gf || x.name.localeCompare(y.name),
        );
        return { groupId, rows };
      });
  }, [schedule, tournamentFilter]);

  const intlBracket = useMemo(() => {
    if (tournamentFilter === 0) return [];
    const byPhase = new Map<number, typeof filteredMatches>();
    for (const m of schedule?.matches ?? []) {
      if (m.tournament_id !== tournamentFilter || m.group_id !== null || m.phase < 2) continue;
      const list = byPhase.get(m.phase) ?? [];
      list.push(m);
      byPhase.set(m.phase, list);
    }
    return Array.from(byPhase.entries())
      .sort(([a], [b]) => a - b)
      .map(([phase, ms]) => ({
        phase,
        name:
          ms.length === 1
            ? 'Final'
            : ms.length === 2
              ? 'Semi-Finals'
              : ms.length === 4
                ? 'Quarter-Finals'
                : `Round of ${ms.length * 2}`,
        matches: ms,
      }));
  }, [schedule, tournamentFilter]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
        {(
          [
            ['fixtures', `${t.intl.tabFixtures} (${schedule?.matches.length ?? 0})`],
            ['rankings', `${t.intl.tabRankings} (${rankings.length})`],
            ['podiums', t.intl.tabPodiums],
            ['squads', `${t.intl.tabSquads}${duties.length > 0 ? ` (${duties.length})` : ''}`],
          ] as [IntlTab, string][]
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

      {tab === 'fixtures' && (
        <>
          <section className="card">
            <h3 className="card-title">{t.intl.windows}</h3>
            {!schedule || schedule.windows.length === 0 ? (
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.intl.noWindows}</div>
            ) : (
              <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                {schedule.windows.map((w) => (
                  <div key={w.window_id} className="pill tabular" style={{ flexDirection: 'column', alignItems: 'flex-start', gap: 2 }}>
                    <strong style={{ fontSize: 12 }}>{w.name}</strong>
                    <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>
                      {w.start_date} → {w.end_date} · {w.window_type}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </section>

          <section className="card">
            <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'center', marginBottom: 12 }}>
              <h3 className="card-title" style={{ margin: 0, marginRight: 'auto' }}>
                {t.intl.matchesTitle} ({filteredMatches.length})
              </h3>
              <select
                className="select"
                value={tournamentFilter}
                onChange={(e) => setTournamentFilter(Number(e.target.value))}
              >
                <option value={0}>{t.intl.allTournaments}</option>
                {(schedule?.tournaments ?? []).map(([id, name, count]) => (
                  <option key={id} value={id}>
                    {name} ({count})
                  </option>
                ))}
              </select>
              <select
                className="select"
                value={statusFilter}
                onChange={(e) => setStatusFilter(e.target.value as 'all' | 'upcoming' | 'finished')}
              >
                <option value="all">{t.intl.allStatus}</option>
                <option value="upcoming">{t.intl.upcoming}</option>
                <option value="finished">{t.intl.finished}</option>
              </select>
              <input
                className="input"
                style={{ width: 170 }}
                placeholder={t.intl.searchNation}
                value={matchSearch}
                onChange={(e) => setMatchSearch(e.target.value)}
              />
            </div>

            {intlBracket.length > 0 && (
              <div style={{ marginBottom: 14 }}>
                <div className="card-title" style={{ marginBottom: 8 }}>{t.competition.bracket}</div>
                <div className="bracket-grid">
                  {intlBracket.map((round) => (
                    <div key={round.phase} className="bracket-col">
                      <div className="bracket-col-head">{round.name}</div>
                      <div className="bracket-matches">
                        {round.matches.map((m) => {
                          const done = m.status === 2 && m.home_goals !== null && m.away_goals !== null;
                          const hasPens = (m.penalty_home_goals ?? 0) > 0 || (m.penalty_away_goals ?? 0) > 0;
                          const homeWon =
                            done &&
                            ((m.home_goals ?? 0) > (m.away_goals ?? 0) ||
                              (hasPens && (m.penalty_home_goals ?? 0) > (m.penalty_away_goals ?? 0)));
                          const awayWon =
                            done &&
                            ((m.away_goals ?? 0) > (m.home_goals ?? 0) ||
                              (hasPens && (m.penalty_away_goals ?? 0) > (m.penalty_home_goals ?? 0)));
                          return (
                            <div key={m.match_id} className="bracket-card">
                              <div className={`bracket-row ${homeWon ? 'winner' : ''}`}>
                                <span className="bracket-team">
                                  <span>{countryFlag(m.home_iso2, m.home_name)}</span>
                                  {m.home_name}
                                </span>
                                <span className="bracket-score tabular">
                                  {done ? m.home_goals : '–'}
                                  {hasPens ? ` (${m.penalty_home_goals})` : ''}
                                </span>
                              </div>
                              <div className={`bracket-row ${awayWon ? 'winner' : ''}`}>
                                <span className="bracket-team">
                                  <span>{countryFlag(m.away_iso2, m.away_name)}</span>
                                  {m.away_name}
                                </span>
                                <span className="bracket-score tabular">
                                  {done ? m.away_goals : '–'}
                                  {hasPens ? ` (${m.penalty_away_goals})` : ''}
                                </span>
                              </div>
                              {m.date && <div className="bracket-foot tabular">{m.date}</div>}
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            {intlGroups.length > 0 && (
              <div className="grid-2" style={{ marginBottom: 14 }}>
                {intlGroups.map((g) => (
                  <div key={g.groupId} className="card" style={{ background: 'var(--surface-card-high)', padding: 10 }}>
                    <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--accent)', marginBottom: 6 }}>
                      {t.competition.group} {String.fromCharCode(64 + g.groupId)}
                    </div>
                    <table className="data-table tabular">
                      <thead>
                        <tr>
                          <th>#</th>
                          <th>{t.intl.nation}</th>
                          <th>{t.common.played}</th>
                          <th>{t.common.gd}</th>
                          <th>{t.common.points}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {g.rows.map((r, idx) => (
                          <tr key={r.id}>
                            <td>{idx + 1}</td>
                            <td style={{ fontWeight: 600 }}>
                              <span style={{ marginRight: 5 }}>{countryFlag(r.iso2, r.name)}</span>
                              {r.name}
                            </td>
                            <td>{r.p}</td>
                            <td>{r.gf - r.ga}</td>
                            <td style={{ fontWeight: 800, color: 'var(--accent-light)' }}>{r.pts}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ))}
              </div>
            )}

            {filteredMatches.length === 0 ? (
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.intl.noMatches}</div>
            ) : (
              <div style={{ maxHeight: 520, overflowY: 'auto' }}>
                <table className="data-table tabular">
                  <thead>
                    <tr>
                      <th>{t.competition.date}</th>
                      <th>{t.nav.competitions}</th>
                      <th style={{ textAlign: 'right' }}>{t.competition.home}</th>
                      <th style={{ textAlign: 'center' }}>{t.competition.score}</th>
                      <th>{t.competition.away}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredMatches.map((m) => {
                      const done = m.status === 2 && m.home_goals !== null && m.away_goals !== null;
                      const hasPens =
                        (m.penalty_home_goals ?? 0) > 0 || (m.penalty_away_goals ?? 0) > 0;
                      return (
                        <tr key={m.match_id}>
                          <td style={{ whiteSpace: 'nowrap', color: 'var(--text-dim)' }}>
                            {m.date ?? '—'}
                          </td>
                          <td style={{ fontSize: 12, color: 'var(--text-dim)' }}>
                            {m.tournament_name}
                            {m.group_id ? ` · ${t.competition.group} ${m.group_id}` : ''}
                          </td>
                          <td style={{ textAlign: 'right', fontWeight: 600 }}>
                            {m.home_name}{' '}
                            <span style={{ fontSize: 15 }}>{countryFlag(m.home_iso2, m.home_name)}</span>
                          </td>
                          <td style={{ textAlign: 'center', fontWeight: 800, color: done ? 'var(--accent-light)' : 'var(--text-dim)' }}>
                            {done ? `${m.home_goals} - ${m.away_goals}` : 'vs'}
                            {hasPens && (
                              <div style={{ fontSize: 10, color: 'var(--warn)' }}>
                                ({m.penalty_home_goals}-{m.penalty_away_goals})
                              </div>
                            )}
                          </td>
                          <td style={{ fontWeight: 600 }}>
                            <span style={{ fontSize: 15 }}>{countryFlag(m.away_iso2, m.away_name)}</span>{' '}
                            {m.away_name}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </section>
        </>
      )}

      {tab === 'rankings' && (
        <section className="card">
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'center', marginBottom: 12 }}>
            <h3 className="card-title" style={{ margin: 0, marginRight: 'auto' }}>
              {t.intl.rankings} ({rankedList.length}/{rankings.length})
            </h3>
            <input
              className="input"
              style={{ width: 180 }}
              placeholder={t.intl.searchNation}
              value={rankSearch}
              onChange={(e) => setRankSearch(e.target.value)}
            />
          </div>
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 12 }}>
            {CONFEDS.map(([code, label]) => (
              <button
                key={code}
                className={`btn ${confedFilter === code ? 'btn-primary' : ''}`}
                style={{ padding: '4px 10px', fontSize: 11.5 }}
                onClick={() => setConfedFilter(code)}
              >
                {code === 'all' ? t.intl.allConfeds : label}
              </button>
            ))}
          </div>
          <div style={{ maxHeight: 560, overflowY: 'auto' }}>
            <table className="data-table tabular">
              <thead>
                <tr>
                  <th>{t.intl.rank}</th>
                  <th>{t.intl.trend}</th>
                  <th>{t.intl.nation}</th>
                  <th>{t.intl.confed}</th>
                  <th>{t.intl.power}</th>
                  <th>{t.intl.points}</th>
                </tr>
              </thead>
              <tbody>
                {rankedList.map((c) => {
                  const delta = c.previous_rank ? c.previous_rank - c.rank : 0;
                  return (
                    <tr key={c.country_id}>
                      <td style={{ fontWeight: 700 }}>{c.rank}</td>
                      <td
                        style={{
                          fontSize: 11.5,
                          fontWeight: 700,
                          color:
                            delta > 0
                              ? 'var(--success)'
                              : delta < 0
                                ? 'var(--danger)'
                                : 'var(--text-dim)',
                        }}
                      >
                        {delta > 0 ? `▲ ${delta}` : delta < 0 ? `▼ ${Math.abs(delta)}` : '—'}
                      </td>
                      <td style={{ fontWeight: 600 }}>
                        <span style={{ fontSize: 16, marginRight: 6 }}>
                          {countryFlag(c.iso2, c.name)}
                        </span>
                        {c.name}{' '}
                        <span style={{ color: 'var(--text-dim)', fontSize: 11 }}>({c.iso2})</span>
                      </td>
                      <td style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                        {(c.confederation ?? 'FIFA').toUpperCase()}
                      </td>
                      <td>
                        <span className="ovr-chip">{c.power}</span>
                      </td>
                      <td style={{ color: 'var(--accent-light)', fontWeight: 700 }}>
                        {c.points.toFixed(2)}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {tab === 'podiums' && (
        <div className="grid-2">
          <section className="card">
            <h3 className="card-title">{t.intl.podiums}</h3>
            {podiums.length === 0 && (
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.intl.noPodiums}</div>
            )}
            {podiums.map((h) => (
              <div key={h.tournament_id} className="news-item">
                <div style={{ fontWeight: 700, fontSize: 12.5 }}>{h.tournament_name}</div>
                <div style={{ fontSize: 12 }}>
                  🥇 {h.winner_name}
                  {h.runner_up_name ? ` · 🥈 ${h.runner_up_name}` : ''}
                  {h.third_place_name ? ` · 🥉 ${h.third_place_name}` : ''}
                </div>
                {h.notes && (
                  <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>{h.notes}</div>
                )}
              </div>
            ))}
          </section>

          <section className="card">
            <h3 className="card-title">{t.intl.mostGoals}</h3>
            {leaders.length === 0 && (
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.competition.noData}</div>
            )}
            {leaders.length > 0 &&
              (
                [
                  [t.intl.mostGoals, (l: NationalLeaderDto) => `${l.goals} ⚽`],
                  [t.intl.mostAssists, (l: NationalLeaderDto) => `${l.assists} 🅰`],
                  [t.intl.mostCaps, (l: NationalLeaderDto) => `${l.caps} caps`],
                ] as [string, (l: NationalLeaderDto) => string][]
              ).map(([label, value]) => {
                const sorted = [...leaders]
                  .sort((a, b) => {
                    if (label === t.intl.mostGoals) return b.goals - a.goals;
                    if (label === t.intl.mostAssists) return b.assists - a.assists;
                    return b.caps - a.caps;
                  })
                  .slice(0, 5);
                return (
                  <div key={label} style={{ marginBottom: 10 }}>
                    <h4 style={{ fontSize: 12.5, margin: '6px 0 4px', color: 'var(--accent)' }}>
                      {label}
                    </h4>
                    {sorted.map((l, idx) => (
                      <div
                        key={`${label}-${l.player_id}`}
                        className="news-item"
                        style={{ display: 'flex', gap: 8, fontSize: 12 }}
                      >
                        <span className="tabular" style={{ width: 18, color: 'var(--text-dim)' }}>
                          {idx + 1}
                        </span>
                        <span style={{ fontWeight: 600 }}>{l.player_name}</span>
                        <span style={{ color: 'var(--text-dim)' }}>{l.country_name}</span>
                        <span className="tabular" style={{ marginLeft: 'auto' }}>
                          {value(l)}
                        </span>
                      </div>
                    ))}
                  </div>
                );
              })}
          </section>
        </div>
      )}

      {tab === 'squads' && (
        <div className="grid-2">
          <section className="card">
            <h3 className="card-title">{t.intl.calledSquad}</h3>
            {countries.length > 0 && (
              <select
                className="select"
                value={countryId}
                onChange={(e) => setCountryId(Number(e.target.value))}
                style={{ marginBottom: 8 }}
              >
                {countries.map(([id, name]) => {
                  const meta = isoByCountryId.get(id);
                  return (
                    <option key={id} value={id}>
                      {countryFlag(meta?.iso2, name)} {name}
                    </option>
                  );
                })}
              </select>
            )}
            {squad.length === 0 && (
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.intl.noSquad}</div>
            )}
            {squad.map((p) => {
              const pm = positionMeta(p.position_id);
              return (
                <div key={p.player_id} className="news-item" style={{ display: 'flex', gap: 8, alignItems: 'center', fontSize: 12.5 }}>
                  <span className="tabular" style={{ width: 24, color: 'var(--text-dim)' }}>
                    {p.shirt_number}
                  </span>
                  <span className={`pos-badge ${pm.cls}`}>{pm.label}</span>
                  <span style={{ fontWeight: 600 }}>{p.player_name}</span>
                  {p.is_captain && <span>©</span>}
                  <span style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>{p.age}</span>
                  <span className="tabular" style={{ marginLeft: 'auto' }}>
                    OVR {p.power} · {p.caps} caps
                  </span>
                </div>
              );
            })}
          </section>

          <section className="card">
            <h3 className="card-title">{t.intl.callupRequests}</h3>
            {duties.length === 0 && (
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.intl.noCallups}</div>
            )}
            {duties.map((d) => (
              <div key={d.duty_id} className="news-item" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                <div>
                  <div style={{ fontWeight: 700, fontSize: 12.5 }}>{d.player_name}</div>
                  <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                    {d.country_name} · {d.window_id} · {d.release_date} → {d.return_date}
                  </div>
                </div>
                <div style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
                  <button
                    className="btn btn-primary"
                    onClick={() =>
                      void desktopApi
                        .respondFriendlyCallup(d.duty_id, true)
                        .then(load)
                        .catch((e) => notify(String(e)))
                    }
                  >
                    {t.intl.release}
                  </button>
                  <button
                    className="btn btn-danger"
                    onClick={() =>
                      void desktopApi
                        .respondFriendlyCallup(d.duty_id, false)
                        .then(load)
                        .catch((e) => notify(String(e)))
                    }
                  >
                    {t.intl.hold}
                  </button>
                </div>
              </div>
            ))}
          </section>
        </div>
      )}
    </div>
  );
}
