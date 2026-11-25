import { useEffect, useState } from 'react';
import { LiveMatchSnapshotDto, logoSrc, MatchEventDto, PlayerDto, positionMeta } from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface LiveMatchPanelProps {
  locale: Locale;
  snapshot: LiveMatchSnapshotDto;
  busy: boolean;
  onAdvance: (minute: number) => void;
  onAdjust: (substitutions: [number, number][], mentality: string | null) => void;
  onFinish: () => void;
  onClose: () => void;
}

const normalize = (raw: string) => raw.replace(/([a-z])([A-Z])/g, '$1_$2').toLowerCase();

export function LiveMatchPanel({
  locale,
  snapshot,
  busy,
  onAdvance,
  onAdjust,
  onFinish,
  onClose,
}: LiveMatchPanelProps) {
  const t = getDictionary(locale);
  const [playing, setPlaying] = useState(true);
  const [speed, setSpeed] = useState(1);
  const [outId, setOutId] = useState<number>(0);
  const [inId, setInId] = useState<number>(0);
  const [mentality, setMentality] = useState('balanced');
  const [appliedMentality, setAppliedMentality] = useState('balanced');
  const [pending, setPending] = useState<[number, number][]>([]);
  const [confirmAbandon, setConfirmAbandon] = useState(false);

  const userIsHome = snapshot.user_side === 'home';
  const clock = Math.min(snapshot.minute, 90);
  const finished = clock >= 90;

  useEffect(() => {
    if (!playing || busy || finished) return;
    const id = window.setInterval(() => {
      onAdvance(Math.min(clock + 1, 90));
    }, 700 / speed);
    return () => window.clearInterval(id);
  }, [playing, busy, clock, speed, finished, onAdvance]);

  useEffect(() => {
    if (finished) setPlaying(false);
  }, [finished]);

  const label = (ev: MatchEventDto) => {
    switch (normalize(ev.event_type)) {
      case 'goal':
        return `⚽ ${t.match.goal}`;
      case 'own_goal':
        return `⚽ ${t.match.ownGoal}`;
      case 'penalty_scored':
        return `⚽ ${t.match.penaltyScored}`;
      case 'penalty_missed':
        return `✖ ${t.match.penaltyMissed}`;
      case 'yellow_card':
        return `🟨 ${t.match.yellowCard}`;
      case 'red_card':
        return `🟥 ${t.match.redCard}`;
      case 'injury':
        return `✚ ${t.match.injury}`;
      case 'substitution_in':
      case 'substitution_out':
        return `⇄ ${t.live.substitution}`;
      case 'var_goal_disallowed':
        return `📺 ${t.match.varDisallowed}`;
      case 'var_check':
        return `📺 ${t.match.varCheck}`;
      case 'save':
        return `🧤 ${t.match.save}`;
      case 'assist':
        return `🅰 ${t.match.assist}`;
      case 'shootout_kick':
        return `⚽ ${t.live.shootout}`;
      default:
        return '';
    }
  };

  const events = snapshot.events.filter((e) => label(e) !== '');
  const status = finished
    ? t.live.finished
    : clock === 45
      ? t.live.halfTime
      : clock < 45
        ? t.live.firstHalf
        : t.live.secondHalf;

  const teamOf = (side: string) =>
    normalize(side) === 'home'
      ? { name: snapshot.home_name, isUser: userIsHome }
      : { name: snapshot.away_name, isUser: !userIsHome };

  const teamBadge = (name: string, logo: string | null) =>
    logoSrc(logo) ? (
      <img className="match-crest" src={logoSrc(logo) ?? ''} alt={name} />
    ) : (
      <div className="crest-pill" style={{ width: 38, height: 38, fontSize: 12 }}>
        {name.slice(0, 3).toUpperCase()}
      </div>
    );

  const addPending = () => {
    if (!outId || !inId || pending.length >= snapshot.subs_left) return;
    setPending((cur) => [...cur, [outId, inId]]);
    setOutId(0);
    setInId(0);
  };

  const playerOption = (p: PlayerDto) => (
    <option key={p.player_id} value={p.player_id}>
      {p.name} · {positionMeta(p.position_id).label} · OVR {p.power}
    </option>
  );

  return (
    <div className="modal-backdrop">
      <div className="modal-box match-panel">
        <header className="match-panel-head">
          <div className="match-panel-meta">
            <span className="match-status-pill">{status}</span>
            <span className="tabular match-clock">{clock}'</span>
            {!finished && (
              <span className="match-status-pill subtle">
                {t.live.subsLeft}: {snapshot.subs_left}
              </span>
            )}
            <span className="match-status-pill subtle">
              {userIsHome ? t.live.homeSide : t.live.awaySide}
            </span>
          </div>

          <div className="match-scoreboard">
            <div className="match-side">
              {teamBadge(snapshot.home_name, snapshot.home_logo)}
              <div className={`match-team-name ${userIsHome ? 'mine' : ''}`}>
                {snapshot.home_name}
              </div>
            </div>
            <div className="match-score tabular">
              {snapshot.home_goals} <span>–</span> {snapshot.away_goals}
            </div>
            <div className="match-side away">
              <div className={`match-team-name ${!userIsHome ? 'mine' : ''}`}>
                {snapshot.away_name}
              </div>
              {teamBadge(snapshot.away_name, snapshot.away_logo)}
            </div>
          </div>
        </header>

        <div className="match-controls">
          {!finished && (
            <>
              <button
                className="btn btn-primary"
                disabled={busy}
                onClick={() => setPlaying((p) => !p)}
              >
                {playing ? `❙❙ ${t.match.pause}` : `▶ ${t.match.play}`}
              </button>
              <div className="seg">
                {[1, 2, 4].map((s) => (
                  <button
                    key={s}
                    className={`seg-item ${speed === s ? 'active' : ''}`}
                    onClick={() => setSpeed(s)}
                  >
                    {s}x
                  </button>
                ))}
              </div>
              {clock < 45 && (
                <button className="btn" disabled={busy} onClick={() => onAdvance(45)}>
                  {t.live.toHalf}
                </button>
              )}
              <button className="btn" disabled={busy} onClick={() => onAdvance(90)}>
                ⏭ {t.match.skip}
              </button>
            </>
          )}
          {finished && (
            <button className="btn btn-primary" disabled={busy} onClick={onFinish}>
              ✓ {t.live.commit}
            </button>
          )}
          {finished ? (
            <button className="btn ghost" disabled={busy} onClick={onClose} style={{ marginLeft: 'auto' }}>
              {t.dashboard.close}
            </button>
          ) : confirmAbandon ? (
            <div style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
              <button className="btn btn-danger" disabled={busy} onClick={onClose}>
                {t.live.confirmAbandon}
              </button>
              <button className="btn" disabled={busy} onClick={() => setConfirmAbandon(false)}>
                {t.start.cancel}
              </button>
            </div>
          ) : (
            <button
              className="btn ghost"
              disabled={busy}
              onClick={() => setConfirmAbandon(true)}
              style={{ marginLeft: 'auto' }}
            >
              {t.live.abandon}
            </button>
          )}
        </div>

        {!finished && (
          <section className="match-adjust">
            <div className="match-adjust-row">
              <select className="select" value={outId} onChange={(e) => setOutId(Number(e.target.value))}>
                <option value={0}>{t.live.out}</option>
                {snapshot.on_pitch.map(playerOption)}
              </select>
              <select className="select" value={inId} onChange={(e) => setInId(Number(e.target.value))}>
                <option value={0}>{t.live.in}</option>
                {snapshot.bench.map(playerOption)}
              </select>
              <button
                className="btn"
                disabled={!outId || !inId || pending.length >= snapshot.subs_left}
                onClick={addPending}
              >
                + {t.live.substitution}
              </button>
              <select
                className="select"
                value={mentality}
                onChange={(e) => setMentality(e.target.value)}
              >
                <option value="defensive">{t.live.defensive}</option>
                <option value="balanced">{t.live.balanced}</option>
                <option value="attacking">{t.live.attacking}</option>
              </select>
              <button
                className="btn btn-primary"
                disabled={busy || (pending.length === 0 && mentality === appliedMentality)}
                onClick={() => {
                  onAdjust(pending, mentality);
                  setAppliedMentality(mentality);
                  setPending([]);
                }}
              >
                {t.live.apply}
              </button>
            </div>
            {pending.length > 0 && (
              <div className="match-adjust-pending" style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                {pending.map(([out, into], idx) => {
                  const outName = snapshot.on_pitch.find((p) => p.player_id === out)?.name ?? out;
                  const inName = snapshot.bench.find((p) => p.player_id === into)?.name ?? into;
                  return (
                    <span key={`${out}-${into}-${idx}`} className="pill">
                      {outName} → {inName}
                      <button
                        type="button"
                        style={{
                          background: 'transparent',
                          border: 'none',
                          color: 'var(--danger)',
                          cursor: 'pointer',
                          fontWeight: 700,
                          padding: '0 2px',
                        }}
                        onClick={() => setPending((cur) => cur.filter((_, i) => i !== idx))}
                      >
                        ×
                      </button>
                    </span>
                  );
                })}
              </div>
            )}
          </section>
        )}

        <section className="match-feed">
          {events.length === 0 ? (
            <div className="match-feed-empty">{t.match.noEvents}</div>
          ) : (
            events
              .slice()
              .reverse()
              .map((ev, idx) => {
                const team = teamOf(ev.side);
                return (
                  <div
                    key={`${ev.minute}-${ev.event_type}-${idx}`}
                    className={`event-row ${team.isUser ? 'mine' : ''}`}
                  >
                    <span className="event-minute tabular">{ev.minute}'</span>
                    <span className="event-label">{label(ev)}</span>
                    <span className="event-player">{ev.player_name ?? '—'}</span>
                    {ev.detail && <span className="event-detail">{ev.detail}</span>}
                    <span className="event-team">{team.name}</span>
                  </div>
                );
              })
          )}
        </section>
      </div>
    </div>
  );
}
