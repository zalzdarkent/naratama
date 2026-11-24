import { useEffect, useState } from 'react';
import {
  CountryRankDto,
  DashboardSnapshotDto,
  desktopApi,
  formatMoney,
  PlayerDto,
  positionMeta,
} from './api/bridge';
import { getDictionary, Locale } from './i18n';

type RouteKey =
  | 'dashboard'
  | 'squad'
  | 'competitions'
  | 'international'
  | 'market'
  | 'clubHub'
  | 'inbox'
  | 'news';

export function App() {
  const [locale, setLocale] = useState<Locale>('id');
  const [route, setRoute] = useState<RouteKey>('dashboard');
  const [newsScope, setNewsScope] = useState<string>('all');
  const [snapshot, setSnapshot] = useState<DashboardSnapshotDto | null>(null);
  const [squad, setSquad] = useState<PlayerDto[]>([]);
  const [rankings, setRankings] = useState<CountryRankDto[]>([]);
  const [selectedPlayer, setSelectedPlayer] = useState<PlayerDto | null>(null);
  const [busy, setBusy] = useState(false);

  const t = getDictionary(locale);

  const refresh = async () => {
    const [dash, sq, fifa] = await Promise.all([
      desktopApi.getDashboard(),
      desktopApi.getSquad(),
      desktopApi.getFifaRankings(),
    ]);
    setSnapshot(dash);
    setSquad(sq);
    setRankings(fifa);
    if (!selectedPlayer && sq.length > 0) {
      setSelectedPlayer(sq[0]);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const handlePlayMatchday = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await desktopApi.simulateNextMatchday();
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    const onKey = (ev: KeyboardEvent) => {
      if (ev.code === 'Space' && ev.target === document.body) {
        ev.preventDefault();
        void handlePlayMatchday();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  if (!snapshot) {
    return <div style={{ padding: 32 }}>Loading Naratama Gelanggang...</div>;
  }

  const starters = squad.slice(0, 11);
  const filteredNews =
    newsScope === 'all'
      ? snapshot.recent_news
      : snapshot.recent_news.filter((n) => n.scope === newsScope);

  return (
    <div className="shell">
      <header className="topbar">
        <div className="topbar-brand">
          <div className="crest-pill">{snapshot.club.abbreviation}</div>
          <div>
            <div style={{ fontWeight: 800, fontSize: 14 }}>{snapshot.club.name}</div>
            <div style={{ fontSize: 12, color: 'var(--text-dim)' }} className="tabular">
              {snapshot.season.name} · {snapshot.current_date}
            </div>
          </div>
        </div>

        <div className="topbar-meta">
          <span className="pill tabular">
            {t.topbar.cash}: <strong>{formatMoney(snapshot.club.budget)}</strong>
          </span>
          <span className={`pill ${snapshot.transfer_window_open ? 'pill-open' : 'pill-closed'}`}>
            {t.topbar.window}: {snapshot.transfer_window_label}
          </span>
          <span className="pill tabular">
            {t.topbar.fanMood}: <strong>{snapshot.fan_mood}/100</strong>
          </span>
        </div>

        <div className="topbar-actions">
          <button
            className="btn"
            onClick={() => setLocale(locale === 'id' ? 'en' : 'id')}
            title="Switch Language"
          >
            {locale.toUpperCase()}
          </button>
          <button className="btn" onClick={() => void desktopApi.saveCareerSlot()}>
            {t.topbar.save}
          </button>
          <button className="btn btn-primary" disabled={busy} onClick={() => void handlePlayMatchday()}>
            ▶ {t.topbar.playMatch}
          </button>
        </div>
      </header>

      <aside className="sidebar">
        {(
          [
            ['dashboard', t.nav.dashboard],
            ['squad', t.nav.squad],
            ['competitions', t.nav.competitions],
            ['international', t.nav.international],
            ['market', t.nav.market],
            ['clubHub', t.nav.clubHub],
            ['inbox', t.nav.inbox],
            ['news', t.nav.news],
          ] as [RouteKey, string][]
        ).map(([key, label]) => (
          <button
            key={key}
            className={`nav-item ${route === key ? 'active' : ''}`}
            onClick={() => setRoute(key)}
          >
            <span>{label}</span>
            {key === 'inbox' && snapshot.unread_inbox > 0 && (
              <span className="badge tabular">{snapshot.unread_inbox}</span>
            )}
          </button>
        ))}
      </aside>

      <main className="workspace">
        {route === 'dashboard' && (
          <>
            <section className="card">
              <h3 className="card-title">{t.dashboard.nextFixture}</h3>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div>
                  <div style={{ fontSize: 18, fontWeight: 800 }}>
                    {snapshot.club.name} vs Opponent #{snapshot.next_match?.away_team_id ?? '—'}
                  </div>
                  <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 4 }}>
                    {t.dashboard.simulateHint}
                  </div>
                </div>
                <div className="pill tabular">
                  {t.dashboard.payroll}: <strong>{formatMoney(snapshot.weekly_payroll)}</strong>
                </div>
              </div>
            </section>

            <div className="grid-2">
              <section className="card">
                <h3 className="card-title">{t.dashboard.leagueStandings}</h3>
                <table className="data-table tabular">
                  <thead>
                    <tr>
                      <th>#</th>
                      <th>Club</th>
                      <th>P</th>
                      <th>GD</th>
                      <th>Pts</th>
                    </tr>
                  </thead>
                  <tbody>
                    {snapshot.standings_preview.slice(0, 8).map((row, idx) => (
                      <tr
                        key={row.team_id}
                        className={row.team_id === snapshot.club.team_id ? 'user-row' : ''}
                      >
                        <td>{idx + 1}</td>
                        <td style={{ fontWeight: 600 }}>{row.team_name}</td>
                        <td>{row.played}</td>
                        <td>{row.goals_for - row.goals_against}</td>
                        <td style={{ fontWeight: 800, color: 'var(--accent-light)' }}>{row.points}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </section>

              <section className="card">
                <h3 className="card-title">{t.dashboard.latestNews}</h3>
                {snapshot.recent_news.slice(0, 4).map((item) => (
                  <div key={item.news_id} className="news-item">
                    <div style={{ fontWeight: 700, fontSize: 13 }}>{item.headline}</div>
                    <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 3 }}>
                      {item.body}
                    </div>
                  </div>
                ))}
              </section>
            </div>
          </>
        )}

        {route === 'squad' && (
          <div className="grid-2">
            <section className="card">
              <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 10 }}>
                <h3 className="card-title" style={{ margin: 0 }}>
                  {t.squad.tacticsBoard} (4-4-2)
                </h3>
                <button
                  className="btn"
                  onClick={() => void desktopApi.trainFocus('attacking').then(refresh)}
                >
                  {t.squad.train}
                </button>
              </div>
              <div className="pitch-2d">
                <div className="pitch-row">
                  {starters.slice(9, 11).map((p) => (
                    <div key={p.player_id} className="pitch-node" onClick={() => setSelectedPlayer(p)}>
                      <div style={{ fontSize: 11, fontWeight: 700 }}>{p.name}</div>
                      <div style={{ fontSize: 11, color: 'var(--accent-light)' }}>{p.power}</div>
                    </div>
                  ))}
                </div>
                <div className="pitch-row">
                  {starters.slice(5, 9).map((p) => (
                    <div key={p.player_id} className="pitch-node" onClick={() => setSelectedPlayer(p)}>
                      <div style={{ fontSize: 11, fontWeight: 700 }}>{p.name}</div>
                      <div style={{ fontSize: 11, color: 'var(--accent-light)' }}>{p.power}</div>
                    </div>
                  ))}
                </div>
                <div className="pitch-row">
                  {starters.slice(1, 5).map((p) => (
                    <div key={p.player_id} className="pitch-node" onClick={() => setSelectedPlayer(p)}>
                      <div style={{ fontSize: 11, fontWeight: 700 }}>{p.name}</div>
                      <div style={{ fontSize: 11, color: 'var(--accent-light)' }}>{p.power}</div>
                    </div>
                  ))}
                </div>
                <div className="pitch-row">
                  {starters.slice(0, 1).map((p) => (
                    <div key={p.player_id} className="pitch-node" onClick={() => setSelectedPlayer(p)}>
                      <div style={{ fontSize: 11, fontWeight: 700 }}>{p.name}</div>
                      <div style={{ fontSize: 11, color: 'var(--warn)' }}>{p.power}</div>
                    </div>
                  ))}
                </div>
              </div>
            </section>

            <section className="card">
              <h3 className="card-title">{t.squad.roster}</h3>
              <table className="data-table tabular">
                <thead>
                  <tr>
                    <th>{t.squad.position}</th>
                    <th>{t.squad.player}</th>
                    <th>{t.squad.ovr}</th>
                    <th>{t.squad.age}</th>
                    <th>{t.squad.fitness}</th>
                    <th>{t.squad.value}</th>
                  </tr>
                </thead>
                <tbody>
                  {squad.map((p) => {
                    const pm = positionMeta(p.position_id);
                    const fitColor =
                      p.fitness > 70 ? 'var(--success)' : p.fitness > 40 ? 'var(--warn)' : 'var(--danger)';
                    return (
                      <tr key={p.player_id} onClick={() => setSelectedPlayer(p)}>
                        <td>
                          <span className={`pos-badge ${pm.cls}`}>{pm.label}</span>
                        </td>
                        <td style={{ fontWeight: 600 }}>
                          {p.name} {p.is_captain ? '©' : ''}
                        </td>
                        <td>
                          <span className="ovr-chip">{p.power}</span>
                        </td>
                        <td>{p.age}</td>
                        <td>
                          <div className="fitness-bar">
                            <div
                              className="fitness-fill"
                              style={{ width: `${p.fitness}%`, background: fitColor }}
                            />
                          </div>
                        </td>
                        <td>{formatMoney(p.value)}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </section>
          </div>
        )}

        {route === 'international' && (
          <section className="card">
            <h3 className="card-title">{t.intl.rankings}</h3>
            <table className="data-table tabular">
              <thead>
                <tr>
                  <th>{t.intl.rank}</th>
                  <th>{t.intl.nation}</th>
                  <th>{t.intl.power}</th>
                  <th>{t.intl.points}</th>
                </tr>
              </thead>
              <tbody>
                {rankings.slice(0, 30).map((c, idx) => (
                  <tr key={c.country_id}>
                    <td>{idx + 1}</td>
                    <td style={{ fontWeight: 600 }}>{c.name}</td>
                    <td>{c.power}</td>
                    <td style={{ color: 'var(--accent-light)', fontWeight: 700 }}>
                      {c.points.toFixed(2)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}

        {route === 'news' && (
          <section className="card">
            <div style={{ display: 'flex', gap: 8, marginBottom: 12 }}>
              {(['all', 'world', 'region', 'league'] as const).map((sc) => (
                <button
                  key={sc}
                  className={`btn ${newsScope === sc ? 'btn-primary' : ''}`}
                  onClick={() => setNewsScope(sc)}
                >
                  {t.news[sc]}
                </button>
              ))}
            </div>
            {filteredNews.map((item) => (
              <div key={item.news_id} className="news-item">
                <div style={{ fontSize: 11, color: 'var(--accent)' }}>
                  {item.date} · {item.category.toUpperCase()}
                </div>
                <div style={{ fontWeight: 700, fontSize: 14, marginTop: 2 }}>{item.headline}</div>
                <div style={{ fontSize: 12.5, color: 'var(--text-dim)', marginTop: 4 }}>
                  {item.body}
                </div>
              </div>
            ))}
          </section>
        )}

        {route === 'inbox' && (
          <section className="card">
            <h3 className="card-title">{t.nav.inbox}</h3>
            {snapshot.recent_inbox.map((m) => (
              <div key={m.message_id} className="news-item">
                <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <strong>{m.title}</strong>
                  <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>{m.date}</span>
                </div>
                <div style={{ fontSize: 12, color: 'var(--accent)' }}>{m.sender}</div>
                <div style={{ fontSize: 12.5, color: 'var(--text-dim)', marginTop: 4 }}>{m.body}</div>
              </div>
            ))}
          </section>
        )}

        {(route === 'competitions' || route === 'market' || route === 'clubHub') && (
          <section className="card">
            <h3 className="card-title">{t.nav[route]}</h3>
            <table className="data-table tabular">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Club</th>
                  <th>P</th>
                  <th>W</th>
                  <th>D</th>
                  <th>L</th>
                  <th>GD</th>
                  <th>Pts</th>
                </tr>
              </thead>
              <tbody>
                {snapshot.standings_preview.map((row, idx) => (
                  <tr
                    key={row.team_id}
                    className={row.team_id === snapshot.club.team_id ? 'user-row' : ''}
                  >
                    <td>{idx + 1}</td>
                    <td style={{ fontWeight: 600 }}>{row.team_name}</td>
                    <td>{row.played}</td>
                    <td>{row.wins}</td>
                    <td>{row.draws}</td>
                    <td>{row.losses}</td>
                    <td>{row.goals_for - row.goals_against}</td>
                    <td style={{ fontWeight: 800, color: 'var(--accent-light)' }}>{row.points}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}
      </main>

      <aside className="inspector">
        <h3 className="card-title">{t.inspector.title}</h3>
        {selectedPlayer ? (
          <div className="card" style={{ background: 'var(--surface-card-high)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <span className={`pos-badge ${positionMeta(selectedPlayer.position_id).cls}`}>
                {positionMeta(selectedPlayer.position_id).label}
              </span>
              <span className="ovr-chip tabular" style={{ color: 'var(--gala-gold)' }}>
                {selectedPlayer.power} / {selectedPlayer.potential}
              </span>
            </div>
            <h4 style={{ margin: '10px 0 4px', fontSize: 16 }}>{selectedPlayer.name}</h4>
            <div style={{ fontSize: 12, color: 'var(--text-dim)' }} className="tabular">
              {t.squad.age}: {selectedPlayer.age} · {t.squad.fitness}: {selectedPlayer.fitness}%
            </div>
            <div style={{ marginTop: 12, display: 'grid', gap: 6, fontSize: 12.5 }} className="tabular">
              <div>
                {t.squad.value}: <strong>{formatMoney(selectedPlayer.value)}</strong>
              </div>
              <div>
                {t.inspector.salary}: <strong>{formatMoney(selectedPlayer.salary)}</strong>
              </div>
            </div>
          </div>
        ) : (
          <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.inspector.selectHint}</div>
        )}
      </aside>
    </div>
  );
}

