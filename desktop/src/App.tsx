import { useEffect, useMemo, useRef, useState } from 'react';
import {
  ClubHubDto,
  CountryRankDto,
  DashboardSnapshotDto,
  desktopApi,
  formatMoney,
  InboxMessageDto,
  isTauriRuntime,
  DossierDto,
  LiveMatchSnapshotDto,
  ManagerOfferDto,
  MatchStatisticsDto,
  OfferInfoDto,
  SponsorOfferDto,
  PlayerDetailDto,
  MarketListingDto,
  MatchdayOutcomeDto,
  MatchReplayDto,
  NewsItemDto,
  PlayerDto,
  positionMeta,
} from './api/bridge';
import { CompetitionPanel } from './components/CompetitionPanel';
import { DossierModal } from './components/DossierModal';
import { HistoryPanel } from './components/HistoryPanel';
import { InternationalPanel } from './components/InternationalPanel';
import { LiveMatchPanel } from './components/LiveMatchPanel';
import { MatchStatsModal } from './components/MatchStatsModal';
import { ScoutingPanel } from './components/ScoutingPanel';
import { TeamDetailModal } from './components/TeamDetailModal';
import { MatchReplay } from './components/MatchReplay';
import { OffseasonModal } from './components/OffseasonModal';
import { StartScreen } from './components/StartScreen';
import { getDictionary, Locale } from './i18n';

type RouteKey =
  | 'dashboard'
  | 'squad'
  | 'competitions'
  | 'international'
  | 'market'
  | 'scouting'
  | 'clubHub'
  | 'inbox'
  | 'history'
  | 'news';

const FORMATIONS: Record<string, [number, number][]> = {
  '4-4-2': [
    [9, 11],
    [5, 9],
    [1, 5],
    [0, 1],
  ],
  '4-3-3': [
    [8, 11],
    [5, 8],
    [1, 5],
    [0, 1],
  ],
  '3-5-2': [
    [9, 11],
    [4, 9],
    [1, 4],
    [0, 1],
  ],
  '4-2-3-1': [
    [10, 11],
    [7, 10],
    [5, 7],
    [1, 5],
    [0, 1],
  ],
  '5-3-2': [
    [9, 11],
    [6, 9],
    [1, 6],
    [0, 1],
  ],
  '4-1-4-1': [
    [10, 11],
    [6, 10],
    [5, 6],
    [1, 5],
    [0, 1],
  ],
};

export function App() {
  const [locale, setLocale] = useState<Locale>('id');
  const [inCareer, setInCareer] = useState<boolean>(() => !isTauriRuntime());
  const [route, setRoute] = useState<RouteKey>('dashboard');
  const [newsScope, setNewsScope] = useState<string>('all');
  const [snapshot, setSnapshot] = useState<DashboardSnapshotDto | null>(null);
  const [squad, setSquad] = useState<PlayerDto[]>([]);
  const [formation, setFormation] = useState<string>('4-4-2');
  const [starterIds, setStarterIds] = useState<number[]>([]);
  const [swapPick, setSwapPick] = useState<number | null>(null);
  const [rankings, setRankings] = useState<CountryRankDto[]>([]);
  const [clubHub, setClubHub] = useState<ClubHubDto | null>(null);
  const [market, setMarket] = useState<MarketListingDto[]>([]);
  const [marketSearch, setMarketSearch] = useState('');
  const [marketMaxAge, setMarketMaxAge] = useState<number>(35);
  const [newsFeed, setNewsFeed] = useState<NewsItemDto[]>([]);
  const [inbox, setInbox] = useState<InboxMessageDto[]>([]);
  const [selectedPlayer, setSelectedPlayer] = useState<PlayerDto | null>(null);
  const [matchOutcomes, setMatchOutcomes] = useState<MatchdayOutcomeDto[] | null>(null);
  const [replay, setReplay] = useState<MatchReplayDto | null>(null);
  const [live, setLive] = useState<LiveMatchSnapshotDto | null>(null);
  const [stats, setStats] = useState<MatchStatisticsDto | null>(null);
  const [teamId, setTeamId] = useState<number | null>(null);
  const [dossier, setDossier] = useState<{ name: string; data: DossierDto } | null>(null);
  const [playerDetail, setPlayerDetail] = useState<PlayerDetailDto | null>(null);
  const [incomingOffers, setIncomingOffers] = useState<OfferInfoDto[]>([]);
  const [managerOffer, setManagerOffer] = useState<ManagerOfferDto | null>(null);
  const [loanOffer, setLoanOffer] = useState(false);
  const [loanSeasons, setLoanSeasons] = useState(1);
  const [sponsorOffers, setSponsorOffers] = useState<SponsorOfferDto[]>([]);

  const openMatch = (matchId: number) => {
    void desktopApi
      .getMatchStatistics(matchId)
      .then((data) => data && setStats(data))
      .catch((e) => notify(String(e)));
  };
  const openDossier = (id: number) => {
    void desktopApi
      .getTacticalDossier(id)
      .then((res) => res && setDossier({ name: res[0], data: res[1] }))
      .catch((e) => notify(String(e)));
  };
  const [showOffseason, setShowOffseason] = useState(false);
  const [staffRole, setStaffRole] = useState('head_physio');
  const [staffName, setStaffName] = useState('Coach Pratama');
  const [staffStars, setStaffStars] = useState(3);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const t = getDictionary(locale);

  const notify = (msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast((cur) => (cur === msg ? null : cur)), 3200);
  };

  const refresh = async () => {
    const [dash, sq, tac, fifa, hub, msgs, offers, move] = await Promise.all([
      desktopApi.getDashboard(),
      desktopApi.getSquad(),
      desktopApi.getTactics(),
      desktopApi.getFifaRankings(),
      desktopApi.getClubHub(),
      desktopApi.getInbox(),
      desktopApi.getIncomingOffers(),
      desktopApi.getManagerOffer(),
    ]);
    setSponsorOffers(await desktopApi.getSponsorOffers().catch(() => []));
    setIncomingOffers(offers ?? []);
    setManagerOffer(move ?? null);
    setSnapshot(dash);
    setSquad(sq);
    setFormation(FORMATIONS[tac.formation] ? tac.formation : '4-4-2');
    const validIds =
      tac.starter_ids.length >= 11
        ? tac.starter_ids.slice(0, 11)
        : sq.slice(0, 11).map((p) => p.player_id);
    setStarterIds(validIds);
    setRankings(fifa);
    setClubHub(hub);
    setInbox(msgs);
    setNewsFeed(dash.recent_news);
    if (!selectedPlayer && sq.length > 0) {
      setSelectedPlayer(sq[0]);
    }
  };

  useEffect(() => {
    if (inCareer) {
      void refresh();
    }
  }, [inCareer]);

  useEffect(() => {
    if (!selectedPlayer) {
      setPlayerDetail(null);
      return;
    }
    let active = true;
    void desktopApi.getPlayerDetail(selectedPlayer.player_id).then((detail) => {
      if (active) setPlayerDetail(detail);
    });
    return () => {
      active = false;
    };
  }, [selectedPlayer?.player_id]);

  useEffect(() => {
    if (!inCareer || route !== 'market') return;
    void desktopApi.getMarketPlayers(marketSearch, marketMaxAge).then(setMarket);
  }, [inCareer, route]);

  useEffect(() => {
    if (!inCareer || route !== 'news') return;
    void desktopApi.getNewsFeed(newsScope).then(setNewsFeed);
  }, [inCareer, route, newsScope]);

  const teamNameMap = useMemo(() => {
    const map = new Map<number, string>();
    if (snapshot) {
      map.set(snapshot.club.team_id, snapshot.club.name);
      for (const r of snapshot.standings_preview) {
        map.set(r.team_id, r.team_name);
      }
    }
    return map;
  }, [snapshot]);

  const advancingRef = useRef(false);
  const handleAdvanceLive = async (minute: number) => {
    // One advance in flight at a time: a late response must never rewind or duplicate minutes.
    if (busy || advancingRef.current) return;
    advancingRef.current = true;
    try {
      const next = await desktopApi.liveMatchAdvance(minute);
      if (next) setLive(next);
    } catch (err) {
      notify(String(err));
    } finally {
      advancingRef.current = false;
    }
  };

  const handleAdjustLive = async (subs: [number, number][], mentality: string | null) => {
    if (busy) return;
    setBusy(true);
    try {
      const next = await desktopApi.liveMatchAdjust(subs, mentality);
      if (next) setLive(next);
    } catch (err) {
      notify(String(err));
    } finally {
      setBusy(false);
    }
  };

  const finishLive = async () => {
    setBusy(true);
    try {
      const outcomes = await desktopApi.finishLiveMatch();
      setLive(null);
      if (outcomes.length > 0) setMatchOutcomes(outcomes);
      await refresh();
    } catch (err) {
      notify(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handlePlayMatchday = async () => {
    if (busy || !snapshot?.next_match) return;
    setBusy(true);
    try {
      const started = await desktopApi.startLiveMatch();
      if (started) {
        setLive(started);
        return;
      }
      const outcomes = await desktopApi.simulateNextMatchday();
      const userMatch = outcomes.find(
        (o) =>
          o.home_team_id === snapshot?.club.team_id || o.away_team_id === snapshot?.club.team_id,
      );
      if (outcomes.length > 0) {
        setMatchOutcomes(outcomes);
      }
      if (userMatch) {
        const stored = await desktopApi.getMatchReplay();
        if (stored && stored.match_id === userMatch.match_id) {
          setReplay(stored);
        }
      }
      await refresh();
    } catch (err) {
      notify(String(err));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (!inCareer) return;
    const routes: RouteKey[] = [
      'dashboard',
      'squad',
      'competitions',
      'international',
      'market',
      'scouting',
      'clubHub',
      'inbox',
      'history',
      'news',
    ];
    const onKey = (ev: KeyboardEvent) => {
      const tag = (ev.target as HTMLElement | null)?.tagName ?? '';
      if (['INPUT', 'TEXTAREA', 'SELECT'].includes(tag) || ev.ctrlKey || ev.metaKey || ev.altKey) {
        return;
      }
      if (ev.code === 'Space') {
        ev.preventDefault();
        setMatchOutcomes(null);
        setShowOffseason(false);
        setReplay(null);
        setLive(null);
        void handlePlayMatchday();
        return;
      }
      if (ev.key === 'Escape') {
        setMatchOutcomes(null);
        setShowOffseason(false);
        setReplay(null);
        setLive(null);
        return;
      }
      const key = ev.key.toLowerCase();
      if (key === 's') {
        void desktopApi
          .saveCareerSlot()
          .then(() => notify(t.topbar.savedToast))
          .catch((e) => notify(String(e)));
        return;
      }
      if (key === 'l') {
        setLocale((cur) => (cur === 'id' ? 'en' : 'id'));
        return;
      }
      const slot = Number(ev.key) - 1;
      if (Number.isInteger(slot) && slot >= 0 && slot < routes.length) {
        setRoute(routes[slot]);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [inCareer, snapshot, busy]);

  if (!inCareer) {
    return (
      <StartScreen
        locale={locale}
        onToggleLocale={() => setLocale(locale === 'id' ? 'en' : 'id')}
        onCareerReady={(snap) => {
          setSnapshot(snap);
          setInCareer(true);
          setRoute('dashboard');
        }}
      />
    );
  }

  if (!snapshot) {
    return <div style={{ padding: 32 }}>{t.common.loading}</div>;
  }

  const squadMap = new Map(squad.map((p) => [p.player_id, p]));
  const starters: PlayerDto[] = starterIds
    .map((id) => squadMap.get(id))
    .filter((p): p is PlayerDto => Boolean(p));
  const starterSet = new Set(starterIds);
  const rows = FORMATIONS[formation] ?? FORMATIONS['4-4-2'];

  const handlePlayerClick = (player: PlayerDto) => {
    setSelectedPlayer(player);
    if (route !== 'squad') return;
    if (swapPick === null) {
      setSwapPick(player.player_id);
      return;
    }
    if (swapPick === player.player_id) {
      setSwapPick(null);
      return;
    }
    const next = [...starterIds];
    const idxA = next.indexOf(swapPick);
    const idxB = next.indexOf(player.player_id);
    if (idxA !== -1 && idxB !== -1) {
      [next[idxA], next[idxB]] = [next[idxB], next[idxA]];
    } else if (idxA !== -1 && idxB === -1) {
      next[idxA] = player.player_id;
    } else if (idxA === -1 && idxB !== -1) {
      next[idxB] = swapPick;
    }
    setStarterIds(next);
    setSwapPick(null);
  };

  const handleAutoXi = () => {
    const gk = squad.filter((p) => p.position_id === 1).sort((a, b) => b.power - a.power);
    const outfield = squad.filter((p) => p.position_id !== 1).sort((a, b) => b.power - a.power);
    const picked = [...gk.slice(0, 1), ...outfield.slice(0, 10)].map((p) => p.player_id);
    setStarterIds(picked.slice(0, 11));
    setSwapPick(null);
  };

  const handleSaveTactics = async () => {
    try {
      await desktopApi.saveTactics(formation, starterIds);
      notify(t.squad.saved);
    } catch (err) {
      notify(String(err));
    }
  };

  const handleTrain = async (focus: string) => {
    try {
      const count = await desktopApi.trainFocus(focus);
      notify(`${t.squad.trainResult} ${count} (${focus})`);
      await refresh();
    } catch (err) {
      notify(String(err));
    }
  };

  const handleMakeOffer = async (listing: MarketListingDto) => {
    try {
      const msg = await desktopApi.makeTransferOffer(
        listing.player.player_id,
        listing.club_id,
        listing.player.value,
        loanOffer,
        loanSeasons,
      );
      notify(`${t.market.offerSent}: ${msg}`);
      await refresh();
      setMarket(await desktopApi.getMarketPlayers(marketSearch, marketMaxAge));
    } catch (err) {
      notify(String(err));
    }
  };

  const handleLeaveCareer = async () => {
    try {
      await desktopApi.leaveCareer(true);
      setInCareer(false);
      setSnapshot(null);
    } catch (err) {
      notify(String(err));
    }
  };

  const nextOppId =
    snapshot.next_match &&
    (snapshot.next_match.home_team_id === snapshot.club.team_id
      ? snapshot.next_match.away_team_id
      : snapshot.next_match.home_team_id);
  const nextOppName = nextOppId
    ? teamNameMap.get(nextOppId) ?? `${t.common.clubFallback} #${nextOppId}`
    : null;
  const isSeasonComplete = snapshot.next_match === null || snapshot.offseason !== null;

  return (
    <div className="shell">
      <header className="topbar">
        <div className="topbar-brand">
          {snapshot.club.logo_base64 ? (
            <img
              src={snapshot.club.logo_base64}
              alt={snapshot.club.abbreviation}
              style={{ width: 38, height: 38, borderRadius: 8, objectFit: 'contain', background: '#0b1424' }}
            />
          ) : (
            <div className="crest-pill">{snapshot.club.abbreviation}</div>
          )}
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
            title={t.topbar.language}
          >
            {locale.toUpperCase()}
          </button>
          <button
            className="btn"
            onClick={() =>
              void desktopApi
                .saveCareerSlot()
                .then(() => notify(t.topbar.savedToast))
                .catch((e) => notify(String(e)))
            }
          >
            {t.topbar.save}
          </button>
          <button className="btn" onClick={() => void handleLeaveCareer()}>
            {t.topbar.menu}
          </button>
          {isSeasonComplete ? (
            <button className="btn btn-primary" onClick={() => setShowOffseason(true)}>
              ★ {t.topbar.offseason}
            </button>
          ) : (
            <button className="btn btn-primary" disabled={busy} onClick={() => void handlePlayMatchday()}>
              ▶ {t.topbar.playMatch}
            </button>
          )}
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
            ['scouting', t.nav.scouting],
            ['clubHub', t.nav.clubHub],
            ['inbox', t.nav.inbox],
            ['history', t.nav.history],
            ['news', t.nav.news],
          ] as [RouteKey, string][]
        ).map(([key, label], idx) => (
          <button
            key={key}
            className={`nav-item ${route === key ? 'active' : ''}`}
            title={`${idx + 1} · ${label}`}
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
                    {nextOppName ? `${snapshot.club.name} vs ${nextOppName}` : t.offseason.title}
                  </div>
                  <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 4 }}>
                    {isSeasonComplete ? t.dashboard.seasonDone : t.dashboard.simulateHint}
                  </div>
                </div>
                <div className="pill tabular">
                  {t.dashboard.payroll}: <strong>{formatMoney(snapshot.weekly_payroll)}</strong>
                </div>
              </div>
            </section>

            {managerOffer && (
              <section className="card" style={{ borderColor: 'var(--accent)' }}>
                <h3 className="card-title">{t.manager.title}</h3>
                <div style={{ fontSize: 13, fontWeight: 700 }}>
                  {managerOffer.club_name} · {managerOffer.league_name}
                </div>
                <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 8 }}>
                  {managerOffer.contract_years} {t.manager.years} · {t.manager.target}: {managerOffer.target} ·{' '}
                  {formatMoney(managerOffer.budget)} · OVR {managerOffer.power}
                </div>
                <div style={{ display: 'flex', gap: 6 }}>
                  <button
                    className="btn btn-primary"
                    onClick={() =>
                      void desktopApi
                        .respondManagerOffer(managerOffer.offer_id, true)
                        .then(() => {
                          notify(t.manager.accepted);
                          setManagerOffer(null);
                        })
                        .catch((e) => notify(String(e)))
                    }
                  >
                    {t.manager.accept}
                  </button>
                  <button
                    className="btn btn-danger"
                    onClick={() =>
                      void desktopApi
                        .respondManagerOffer(managerOffer.offer_id, false)
                        .then(() => setManagerOffer(null))
                        .catch((e) => notify(String(e)))
                    }
                  >
                    {t.manager.decline}
                  </button>
                </div>
              </section>
            )}

            <div className="grid-2">
              <section className="card">
                <h3 className="card-title">{t.dashboard.leagueStandings}</h3>
                <table className="data-table tabular">
                  <thead>
                    <tr>
                      <th>#</th>
                      <th>{t.common.club}</th>
                      <th>{t.common.played}</th>
                      <th>{t.common.gd}</th>
                      <th>{t.common.points}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {snapshot.standings_preview.slice(0, 10).map((row, idx) => (
                      <tr
                        key={row.team_id}
                        className={row.team_id === snapshot.club.team_id ? 'user-row' : ''}
                      >
                        <td>{idx + 1}</td>
                        <td
                          style={{ fontWeight: 600, cursor: 'pointer' }}
                          onClick={() => setTeamId(row.team_id)}
                        >
                          {row.team_name}
                        </td>
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
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 8, gap: 8, flexWrap: 'wrap' }}>
                <h3 className="card-title" style={{ margin: 0 }}>
                  {t.squad.tacticsBoard}
                </h3>
                <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
                  <select
                    className="select"
                    value={formation}
                    onChange={(e) => setFormation(e.target.value)}
                  >
                    {Object.keys(FORMATIONS).map((f) => (
                      <option key={f} value={f}>
                        {f}
                      </option>
                    ))}
                  </select>
                  <button className="btn" onClick={handleAutoXi}>
                    {t.squad.autoXi}
                  </button>
                  <button className="btn btn-primary" onClick={() => void handleSaveTactics()}>
                    {t.squad.saveTactics}
                  </button>
                  {nextOppId && (
                    <button className="btn" onClick={() => openDossier(nextOppId)}>
                      {t.dossier.short}
                    </button>
                  )}
                </div>
              </div>
              <div style={{ fontSize: 11.5, color: 'var(--text-dim)', marginBottom: 10 }}>
                {t.squad.swapHint}
              </div>
              <div className="pitch-2d">
                {rows.map(([start, end], rowIdx) => (
                  <div key={rowIdx} className="pitch-row">
                    {starters.slice(start, end).map((p) => (
                      <div
                        key={p.player_id}
                        className={`pitch-node ${swapPick === p.player_id ? 'swap-selected' : ''}`}
                        onClick={() => handlePlayerClick(p)}
                      >
                        <div style={{ fontSize: 11, fontWeight: 700 }}>{p.name}</div>
                        <div
                          style={{
                            fontSize: 11,
                            color: rowIdx === rows.length - 1 ? 'var(--warn)' : 'var(--accent-light)',
                          }}
                        >
                          {p.power}
                        </div>
                      </div>
                    ))}
                  </div>
                ))}
              </div>
              <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginTop: 12 }}>
                {(['attacking', 'defending', 'playmaking', 'goalkeeping', 'fitness'] as const).map(
                  (focus) => (
                    <button key={focus} className="btn" onClick={() => void handleTrain(focus)}>
                      {t.squad.train}: {focus}
                    </button>
                  ),
                )}
              </div>
            </section>

            <section className="card">
              <h3 className="card-title">{t.squad.roster}</h3>
              <table className="data-table tabular">
                <thead>
                  <tr>
                    <th>{t.squad.starter}</th>
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
                    const isStarter = starterSet.has(p.player_id);
                    return (
                      <tr
                        key={p.player_id}
                        className={swapPick === p.player_id ? 'swap-selected' : isStarter ? 'user-row' : ''}
                        onClick={() => handlePlayerClick(p)}
                      >
                        <td>{isStarter ? '★' : ''}</td>
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

        {route === 'competitions' && (
          <CompetitionPanel
            locale={locale}
            userTeamId={snapshot.club.team_id}
            onOpenMatch={openMatch}
            onOpenTeam={(id) => setTeamId(id)}
          />
        )}

        {route === 'history' && (
          <HistoryPanel locale={locale} currentSeasonId={snapshot.season.season_id} />
        )}

        {route === 'international' && (
          <>
            <InternationalPanel locale={locale} notify={notify} />
            <section className="card" style={{ marginTop: 12 }}>
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
                  {rankings.slice(0, 50).map((c, idx) => (
                    <tr key={c.country_id}>
                      <td>{idx + 1}</td>
                      <td style={{ fontWeight: 600 }}>
                        {c.name}{' '}
                        <span style={{ color: 'var(--text-dim)', fontSize: 11 }}>({c.iso2})</span>
                      </td>
                      <td>{c.power}</td>
                      <td style={{ color: 'var(--accent-light)', fontWeight: 700 }}>
                        {c.points.toFixed(2)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          </>
        )}

        {route === 'market' && (
          <section className="card">
            <div style={{ display: 'flex', gap: 8, marginBottom: 12, flexWrap: 'wrap', alignItems: 'center' }}>
              <input
                className="input"
                style={{ flex: 1, minWidth: 180 }}
                placeholder={t.market.searchPlaceholder}
                value={marketSearch}
                onChange={(e) => setMarketSearch(e.target.value)}
              />
              <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12.5, color: 'var(--text-dim)' }}>
                {t.market.maxAge}
                <input
                  className="input tabular"
                  type="number"
                  min={16}
                  max={45}
                  style={{ width: 68 }}
                  value={marketMaxAge}
                  onChange={(e) => setMarketMaxAge(Number(e.target.value))}
                />
              </label>
              <button
                className="btn btn-primary"
                onClick={() => void desktopApi.getMarketPlayers(marketSearch, marketMaxAge).then(setMarket)}
              >
                {t.market.search}
              </button>
              <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 12.5, color: 'var(--text-dim)' }}>
                <input type="checkbox" checked={loanOffer} onChange={(e) => setLoanOffer(e.target.checked)} />
                {t.market.loan}
              </label>
              {loanOffer && (
                <select
                  className="select"
                  value={loanSeasons}
                  onChange={(e) => setLoanSeasons(Number(e.target.value))}
                >
                  {[1, 2, 3].map((n) => (
                    <option key={n} value={n}>
                      {n} {t.market.seasons}
                    </option>
                  ))}
                </select>
              )}
            </div>
            <table className="data-table tabular">
              <thead>
                <tr>
                  <th>{t.squad.position}</th>
                  <th>{t.squad.player}</th>
                  <th>{t.market.club}</th>
                  <th>{t.squad.ovr}</th>
                  <th>{t.squad.age}</th>
                  <th>{t.squad.value}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {market.map((m) => {
                  const pm = positionMeta(m.player.position_id);
                  return (
                    <tr key={m.player.player_id} onClick={() => setSelectedPlayer(m.player)}>
                      <td>
                        <span className={`pos-badge ${pm.cls}`}>{pm.label}</span>
                      </td>
                      <td style={{ fontWeight: 600 }}>{m.player.name}</td>
                      <td style={{ color: 'var(--text-dim)' }}>{m.club_name}</td>
                      <td>
                        <span className="ovr-chip">{m.player.power}</span>
                      </td>
                      <td>{m.player.age}</td>
                      <td>{formatMoney(m.player.value)}</td>
                      <td>
                        <button
                          className="btn"
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleMakeOffer(m);
                          }}
                        >
                          {t.market.makeOffer}
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>

            <h3 className="card-title" style={{ marginTop: 16 }}>
              {t.market.incoming} ({incomingOffers.length})
            </h3>
            {incomingOffers.length === 0 && (
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.market.noIncoming}</div>
            )}
            {incomingOffers.map((offer) => (
              <div key={offer.offer_id} className="news-item" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                <div>
                  <div style={{ fontWeight: 700, fontSize: 12.5 }}>{offer.player_name}</div>
                  <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                    {offer.from_team_name} · {formatMoney(offer.offer_amount)}
                    {offer.is_loan ? ` · ${t.market.loan} ${offer.loan_seasons}` : ''} · {offer.created_date}
                  </div>
                </div>
                <div style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
                  <button
                    className="btn btn-primary"
                    onClick={() =>
                      void desktopApi
                        .respondTransferOffer(offer.offer_id, true)
                        .then((msg) => {
                          notify(msg);
                          return refresh();
                        })
                        .catch((e) => notify(String(e)))
                    }
                  >
                    {t.market.accept}
                  </button>
                  <button
                    className="btn btn-danger"
                    onClick={() =>
                      void desktopApi
                        .respondTransferOffer(offer.offer_id, false)
                        .then((msg) => {
                          notify(msg);
                          return refresh();
                        })
                        .catch((e) => notify(String(e)))
                    }
                  >
                    {t.market.reject}
                  </button>
                </div>
              </div>
            ))}
          </section>
        )}

        {route === 'scouting' && <ScoutingPanel locale={locale} notify={notify} />}

        {route === 'clubHub' && clubHub && (
          <div className="grid-2">
            <section className="card">
              <h3 className="card-title">{t.clubHub.facilities}</h3>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 14 }}>
                <div className="tabular">
                  <div>
                    {t.clubHub.trainingLevel}: <strong>Lv {clubHub.facilities.training_level} / 5</strong>
                  </div>
                  <div style={{ marginTop: 4 }}>
                    {t.clubHub.medicalLevel}: <strong>Lv {clubHub.facilities.medical_level} / 5</strong>
                  </div>
                </div>
                <button
                  className="btn btn-primary"
                  disabled={clubHub.facilities.training_level >= 5}
                  onClick={() =>
                    void desktopApi
                      .upgradeTrainingGround()
                      .then(() => {
                        notify(t.clubHub.upgraded);
                        return refresh();
                      })
                      .catch((e) => notify(String(e)))
                  }
                >
                  {t.clubHub.upgradeTraining}
                </button>
              </div>

              <h3 className="card-title">{t.clubHub.staff}</h3>
              <div style={{ display: 'flex', gap: 6, marginBottom: 10, flexWrap: 'wrap' }}>
                <select className="select" value={staffRole} onChange={(e) => setStaffRole(e.target.value)}>
                  <option value="head_physio">Head Physio</option>
                  <option value="tactical_analyst">Tactical Analyst</option>
                  <option value="head_scout">Head Scout</option>
                </select>
                <input
                  className="input"
                  style={{ flex: 1, minWidth: 110 }}
                  value={staffName}
                  onChange={(e) => setStaffName(e.target.value)}
                />
                <select
                  className="select"
                  value={staffStars}
                  onChange={(e) => setStaffStars(Number(e.target.value))}
                >
                  {[1, 2, 3, 4, 5].map((s) => (
                    <option key={s} value={s}>
                      {s}★
                    </option>
                  ))}
                </select>
                <button
                  className="btn"
                  onClick={() =>
                    void desktopApi
                      .hireStaff(staffRole, staffName, staffStars)
                      .then(() => {
                        notify(`${t.clubHub.hired} ${staffName}`);
                        return refresh();
                      })
                      .catch((e) => notify(String(e)))
                  }
                >
                  {t.clubHub.hireStaff}
                </button>
              </div>
              {clubHub.staff.map((st) => (
                <div key={st.staff_id} className="news-item" style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <div>
                    <strong>{st.name}</strong>{' '}
                    <span style={{ color: 'var(--text-dim)', fontSize: 12 }}>({st.role})</span>
                  </div>
                  <span className="tabular" style={{ color: 'var(--accent-light)' }}>
                    {st.stars}★ · {formatMoney(st.salary)}/w
                  </span>
                </div>
              ))}
            </section>

            <section className="card">
              <h3 className="card-title">{t.clubHub.cashFlow}</h3>
              <table className="data-table tabular" style={{ marginBottom: 16 }}>
                <tbody>
                  {clubHub.cash_summary.map(([cat, amt]) => (
                    <tr key={cat}>
                      <td>{cat}</td>
                      <td
                        style={{
                          textAlign: 'right',
                          fontWeight: 700,
                          color: amt >= 0 ? 'var(--success)' : 'var(--danger)',
                        }}
                      >
                        {formatMoney(amt)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>

              <h3 className="card-title">{t.clubHub.stadium}</h3>
              <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 10, flexWrap: 'wrap' }}>
                <button
                  className="btn btn-primary"
                  onClick={() =>
                    void desktopApi
                      .expandStadium()
                      .then((res) => {
                        if (res) notify(`${t.clubHub.expanded} +${res.added_seats} (${formatMoney(res.cost)})`);
                        return refresh();
                      })
                      .catch((e) => notify(String(e)))
                  }
                >
                  {t.clubHub.expand}
                </button>
                {(['general', 'stand', 'terrace', 'boxes'] as const).map((seat) => (
                  <label key={seat} style={{ display: 'flex', gap: 4, alignItems: 'center', fontSize: 12 }}>
                    {t.clubHub[seat]}
                    <input
                      className="input tabular"
                      type="number"
                      min={0}
                      style={{ width: 74 }}
                      defaultValue={
                        seat === 'general'
                          ? clubHub.stadium?.ticket_price_general
                          : seat === 'stand'
                            ? clubHub.stadium?.ticket_price_stand
                            : seat === 'terrace'
                              ? clubHub.stadium?.ticket_price_terrace
                              : clubHub.stadium?.ticket_price_boxes
                      }
                      onBlur={(e) =>
                        void desktopApi
                          .setTicketPrice(seat, Number(e.target.value))
                          .then(() => notify(t.clubHub.ticketSaved))
                          .catch((err) => notify(String(err)))
                      }
                    />
                  </label>
                ))}
              </div>

              <h3 className="card-title">{t.clubHub.sponsorOffers}</h3>
              {sponsorOffers.length === 0 && (
                <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.clubHub.noOffers}</div>
              )}
              {sponsorOffers.map((offer) => (
                <div key={offer.slot_type} className="news-item" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                  <div>
                    <div style={{ fontWeight: 700, fontSize: 12.5 }}>
                      {offer.sponsor_name} · {offer.slot_type}
                    </div>
                    <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                      {formatMoney(offer.upfront)} upfront · {formatMoney(offer.weekly)}/w ·{' '}
                      {offer.target_wins} {t.clubHub.winsTarget}
                    </div>
                  </div>
                  <button
                    className="btn"
                    style={{ marginLeft: 'auto' }}
                    onClick={() =>
                      void desktopApi
                        .signSponsorship(offer)
                        .then(() => {
                          notify(t.clubHub.signed);
                          return refresh();
                        })
                        .catch((e) => notify(String(e)))
                    }
                  >
                    {t.clubHub.sign}
                  </button>
                </div>
              ))}

              <h3 className="card-title">{t.clubHub.sponsors}</h3>
              {clubHub.sponsorships.map((sp) => (
                <div key={sp.sponsorship_id} className="news-item" style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <div>
                    <strong>{sp.sponsor_name}</strong>{' '}
                    <span style={{ color: 'var(--text-dim)', fontSize: 12 }}>({sp.deal_type})</span>
                  </div>
                  <span className="tabular" style={{ color: 'var(--success)', fontWeight: 700 }}>
                    {formatMoney(sp.base_reward)}
                  </span>
                </div>
              ))}
            </section>
          </div>
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
            {newsFeed.map((item) => (
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
            {inbox.map((m) => (
              <div
                key={m.message_id}
                className={`news-item ${m.is_read ? '' : 'unread'}`}
                style={{ cursor: 'pointer' }}
                onClick={() =>
                  !m.is_read &&
                  void desktopApi.markMessageRead(m.message_id).then(refresh)
                }
              >
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
              {playerDetail?.contract && (
                <div>
                  {t.inspector.contract}:{' '}
                  <strong>{playerDetail.contract.end_date ?? '—'}</strong>
                </div>
              )}
              {selectedPlayer && squad.some((p) => p.player_id === selectedPlayer.player_id) && (
                <button
                  className="btn"
                  onClick={() =>
                    void desktopApi
                      .renewContract(selectedPlayer.player_id)
                      .then((res) => {
                        if (res) {
                          notify(`${t.inspector.renewed} ${formatMoney(res.new_salary)} · ${res.new_end_date}`);
                        }
                        return refresh();
                      })
                      .catch((e) => notify(String(e)))
                  }
                >
                  {t.inspector.renew}
                </button>
              )}
              {playerDetail && (
                <div>
                  {t.inspector.caps}: <strong>{playerDetail.national_caps}</strong>
                </div>
              )}
            </div>
            {playerDetail && playerDetail.season_stats.length > 0 && (
              <div style={{ marginTop: 10, fontSize: 12 }} className="tabular">
                <div style={{ color: 'var(--text-dim)', marginBottom: 3 }}>{t.inspector.seasonStats}</div>
                {playerDetail.season_stats.slice(0, 3).map((row) => (
                  <div key={row.season_id} style={{ display: 'flex', gap: 6, fontSize: 11.5 }}>
                    <span style={{ width: 26, color: 'var(--text-dim)' }}>S{row.season_id}</span>
                    <span>{row.matches_played} {t.stats.matches}</span>
                    <span>{row.goals} ⚽</span>
                    <span>{row.assists} 🅰</span>
                    <span style={{ marginLeft: 'auto' }}>
                      {(row.rating / 10).toFixed(1)}
                    </span>
                  </div>
                ))}
              </div>
            )}
            {playerDetail && playerDetail.recent_ratings.length > 0 && (
              <div style={{ marginTop: 8, fontSize: 11.5 }} className="tabular">
                <div style={{ color: 'var(--text-dim)', marginBottom: 3 }}>{t.inspector.recentRatings}</div>
                {playerDetail.recent_ratings.map(([date, rating, mvp], idx) => (
                  <div key={`${date}-${idx}`} style={{ display: 'flex', gap: 6 }}>
                    <span style={{ color: 'var(--text-dim)' }}>{date}</span>
                    <span>{(rating / 10).toFixed(1)}</span>
                    {mvp && <span>★</span>}
                  </div>
                ))}
              </div>
            )}
          </div>
        ) : (
          <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.inspector.selectHint}</div>
        )}
      </aside>

      {matchOutcomes && (
        <div className="modal-backdrop" onClick={() => setMatchOutcomes(null)}>
          <div className="modal-box" onClick={(e) => e.stopPropagation()}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <h3 className="card-title" style={{ margin: 0 }}>
                {t.dashboard.matchResults} ({matchOutcomes.length})
              </h3>
              <button className="btn btn-primary" onClick={() => setMatchOutcomes(null)}>
                {t.dashboard.close}
              </button>
            </div>
            <table className="data-table tabular">
              <tbody>
                {matchOutcomes.slice(0, 15).map((o) => {
                  const isUser =
                    o.home_team_id === snapshot.club.team_id ||
                    o.away_team_id === snapshot.club.team_id;
                  return (
                    <tr
                      key={o.match_id}
                      className={isUser ? 'user-row' : ''}
                      style={{ cursor: 'pointer' }}
                      onClick={() => openMatch(o.match_id)}
                    >
                      <td style={{ textAlign: 'right', fontWeight: 600 }}>
                        {teamNameMap.get(o.home_team_id) ?? `${t.common.clubFallback} #${o.home_team_id}`}
                      </td>
                      <td style={{ textAlign: 'center', fontWeight: 800, color: 'var(--accent-light)' }}>
                        {o.home_goals} - {o.away_goals}
                      </td>
                      <td style={{ fontWeight: 600 }}>
                        {teamNameMap.get(o.away_team_id) ?? `${t.common.clubFallback} #${o.away_team_id}`}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {stats && (
        <MatchStatsModal
          locale={locale}
          stats={stats}
          userTeamId={snapshot.club.team_id}
          onClose={() => setStats(null)}
          onOpenTeam={(id) => {
            setStats(null);
            setTeamId(id);
          }}
        />
      )}

      {teamId !== null && (
        <TeamDetailModal
          locale={locale}
          teamId={teamId}
          userTeamId={snapshot.club.team_id}
          onClose={() => setTeamId(null)}
          onOpenPlayer={(id) => {
            const found = squad.find((p) => p.player_id === id);
            if (found) setSelectedPlayer(found);
          }}
          onOpenDossier={(id) => {
            setTeamId(null);
            openDossier(id);
          }}
        />
      )}

      {dossier && (
        <DossierModal
          locale={locale}
          teamName={dossier.name}
          dossier={dossier.data}
          onClose={() => setDossier(null)}
        />
      )}

      {live && (
        <LiveMatchPanel
          locale={locale}
          snapshot={live}
          busy={busy}
          onAdvance={(minute) => void handleAdvanceLive(minute)}
          onAdjust={(subs, mentality) => void handleAdjustLive(subs, mentality)}
          onFinish={() => void finishLive()}
          onClose={() => setLive(null)}
        />
      )}

      {replay && (
        <MatchReplay
          locale={locale}
          userTeamId={snapshot.club.team_id}
          replay={replay}
          onClose={() => setReplay(null)}
        />
      )}

      {showOffseason && (
        <OffseasonModal
          locale={locale}
          offseason={snapshot.offseason}
          onUpdated={(snap) => {
            setSnapshot(snap);
            void refresh();
          }}
          onClose={() => setShowOffseason(false)}
        />
      )}

      {toast && <div className="toast">{toast}</div>}
    </div>
  );
}
