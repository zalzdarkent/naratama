import { useEffect, useState } from 'react';
import { LiveMatchSnapshotDto, MatchEventDto, PlayerDto } from '../api/bridge';
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
  const [pending, setPending] = useState<[number, number][]>([]);

  const userIsHome = snapshot.user_side === 'home';
  const userGoals = userIsHome ? snapshot.home_goals : snapshot.away_goals;
  const oppGoals = userIsHome ? snapshot.away_goals : snapshot.home_goals;
  const clock = Math.min(snapshot.minute, 90);

  useEffect(() => {
    if (!playing || busy || clock >= 90) return;
    const id = window.setInterval(() => {
      onAdvance(Math.min(clock + 1, 90));
    }, 700 / speed);
    return () => window.clearInterval(id);
  }, [playing, busy, clock, speed, onAdvance]);

  useEffect(() => {
    if (clock >= 90) setPlaying(false);
  }, [clock]);

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
      default:
        return '';
    }
  };

  const highlights = snapshot.events
    .filter((e) => label(e) !== '')
    .slice(-6)
    .reverse();

  const addPending = () => {
    if (!outId || !inId) return;
    if (pending.length >= snapshot.subs_left) return;
    setPending((cur) => [...cur, [outId, inId]]);
    setOutId(0);
    setInId(0);
  };

  const applyAdjustments = () => {
    onAdjust(pending, mentality);
    setPending([]);
  };

  const playerOption = (p: PlayerDto) => (
    <option key={p.player_id} value={p.player_id}>
      {p.name} ({p.position_id} · OVR {p.power})
    </option>
  );

  return (
    <div className="modal-backdrop">
      <div className="modal-box" onClick={(e) => e.stopPropagation()}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 12 }}>
          <div>
            <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
              {t.live.title} · {t.match.minute} {clock}'
            </div>
            <div style={{ fontSize: 22, fontWeight: 800, marginTop: 2 }} className="tabular">
              {snapshot.home_name} {snapshot.home_goals} - {snapshot.away_goals} {snapshot.away_name}
            </div>
            <div style={{ fontSize: 12, marginTop: 2, color: 'var(--accent-light)' }}>
              {snapshot.minute < 45
                ? t.live.firstHalf
                : snapshot.minute < 90
                  ? t.live.secondHalf
                  : t.live.finished}
            </div>
          </div>
          <div style={{ textAlign: 'right' }} className="tabular">
            <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>
              {userGoals} - {oppGoals}
            </div>
            <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
              {t.live.subsLeft}: {snapshot.subs_left}
            </div>
          </div>
        </div>

        <div style={{ display: 'flex', gap: 6, margin: '12px 0', flexWrap: 'wrap' }}>
          <button
            className="btn btn-primary"
            disabled={busy || clock >= 90}
            onClick={() => setPlaying((p) => !p)}
          >
            {playing ? `❙❙ ${t.match.pause}` : `▶ ${t.match.play}`}
          </button>
          {[1, 2, 4].map((s) => (
            <button
              key={s}
              className={`btn ${speed === s ? 'btn-primary' : ''}`}
              onClick={() => setSpeed(s)}
            >
              {s}x
            </button>
          ))}
          <button className="btn" disabled={busy || clock >= 90} onClick={() => onAdvance(90)}>
            ⏭ {t.match.skip}
          </button>
          {clock >= 90 && (
            <button className="btn btn-primary" disabled={busy} onClick={onFinish}>
              ✓ {t.live.commit}
            </button>
          )}
          {clock < 90 && (
            <button className="btn" disabled={busy} onClick={onClose} style={{ marginLeft: 'auto' }}>
              {t.dashboard.close}
            </button>
          )}
        </div>

        {clock < 90 && (
          <div className="card" style={{ background: 'var(--surface-card-high)', marginBottom: 10 }}>
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
              <select
                className="select"
                value={outId}
                onChange={(e) => setOutId(Number(e.target.value))}
              >
                <option value={0}>{t.live.out}</option>
                {snapshot.on_pitch.map(playerOption)}
              </select>
              <select
                className="select"
                value={inId}
                onChange={(e) => setInId(Number(e.target.value))}
              >
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
                disabled={busy || (pending.length === 0 && mentality === 'balanced')}
                onClick={applyAdjustments}
              >
                {t.live.apply}
              </button>
            </div>
            {pending.length > 0 && (
              <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 6 }}>
                {pending
                  .map(([out, into]) => {
                    const outName = snapshot.on_pitch.find((p) => p.player_id === out)?.name ?? out;
                    const inName = snapshot.bench.find((p) => p.player_id === into)?.name ?? into;
                    return `${outName} → ${inName}`;
                  })
                  .join(', ')}
              </div>
            )}
          </div>
        )}

        <div style={{ maxHeight: 220, overflowY: 'auto' }}>
          {highlights.length === 0 && (
            <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.match.noEvents}</div>
          )}
          {highlights.map((ev, idx) => (
            <div key={`${ev.minute}-${idx}`} className="news-item" style={{ display: 'flex', gap: 8 }}>
              <span className="tabular" style={{ width: 34, color: 'var(--text-dim)' }}>
                {ev.minute}'
              </span>
              <span style={{ fontWeight: 700, fontSize: 12.5 }}>{label(ev)}</span>
              <span style={{ fontSize: 12.5 }}>{ev.player_name ?? ''}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
