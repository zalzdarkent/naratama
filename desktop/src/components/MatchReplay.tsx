import { useEffect, useMemo, useState } from 'react';
import { logoSrc, MatchEventDto, MatchReplayDto } from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface MatchReplayProps {
  locale: Locale;
  userTeamId: number;
  replay: MatchReplayDto;
  onClose: () => void;
}

/** The engine serialises enums in PascalCase; normalise either casing to PascalCase-free ids. */
const normalize = (raw: string) => raw.replace(/([a-z])([A-Z])/g, '$1_$2').toLowerCase();

const SHOWN = new Set([
  'goal',
  'own_goal',
  'penalty_scored',
  'penalty_missed',
  'yellow_card',
  'red_card',
  'injury',
  'var_goal_disallowed',
  'var_check',
  'save',
  'assist',
  'shootout_kick',
]);

export function MatchReplay({ locale, userTeamId, replay, onClose }: MatchReplayProps) {
  const t = getDictionary(locale);
  const lastMinute = Math.max(90, ...replay.events.map((e) => e.minute));
  const [minute, setMinute] = useState(0);
  const [playing, setPlaying] = useState(true);
  const [speed, setSpeed] = useState(1);

  useEffect(() => {
    if (!playing || minute >= lastMinute) return;
    const id = window.setInterval(() => {
      setMinute((cur) => {
        if (cur + 1 >= lastMinute) {
          setPlaying(false);
          return lastMinute;
        }
        return cur + 1;
      });
    }, 1000 / speed);
    return () => window.clearInterval(id);
  }, [playing, speed, lastMinute, minute]);

  const liveScore = useMemo(() => {
    let home = 0;
    let away = 0;
    for (const ev of replay.events) {
      if (ev.minute > minute) break;
      const type = normalize(ev.event_type);
      const creditHome = type === 'own_goal' ? normalize(ev.side) === 'away' : normalize(ev.side) === 'home';
      const isGoal = ['goal', 'own_goal', 'penalty_scored'].includes(type);
      if (!isGoal) continue;
      if (creditHome) home += 1;
      else away += 1;
    }
    return { home, away };
  }, [replay.events, minute]);

  const visible = replay.events.filter(
    (e) => e.minute <= minute && SHOWN.has(normalize(e.event_type)),
  );
  const userIsHome = replay.home_team_id === userTeamId;

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
      case 'var_goal_disallowed':
        return `📺 ${t.match.varDisallowed}`;
      case 'var_check':
        return `📺 ${t.match.varCheck}`;
      case 'save':
        return `🧤 ${t.match.save}`;
      case 'assist':
        return `🅰 ${t.match.assist}`;
      default:
        return `⚽ ${t.match.shootout}`;
    }
  };

  const teamName = (side: string) =>
    (normalize(side) === 'home') === userIsHome ? replay.home_name : replay.away_name;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-box" onClick={(e) => e.stopPropagation()}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12 }}>
          <div>
            <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }} className="tabular">
              {replay.date} · {t.match.title}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginTop: 4 }}>
              {logoSrc(replay.home_logo) && (
                <img src={logoSrc(replay.home_logo) ?? ''} alt="" style={{ width: 32, height: 32 }} />
              )}
              <div style={{ fontSize: 18, fontWeight: 800 }} className="tabular">
                {replay.home_name}
              </div>
              <div style={{ fontSize: 22, fontWeight: 900 }} className="tabular">
                {liveScore.home} - {liveScore.away}
              </div>
              <div style={{ fontSize: 18, fontWeight: 800 }} className="tabular">
                {replay.away_name}
              </div>
              {logoSrc(replay.away_logo) && (
                <img src={logoSrc(replay.away_logo) ?? ''} alt="" style={{ width: 32, height: 32 }} />
              )}
            </div>
            {minute >= lastMinute &&
              replay.penalty_home_goals !== null &&
              replay.penalty_away_goals !== null && (
                <div style={{ fontSize: 12, color: 'var(--accent-light)' }} className="tabular">
                  {t.match.penalties}: {replay.penalty_home_goals} - {replay.penalty_away_goals}
                </div>
              )}
          </div>
          <div style={{ textAlign: 'right' }}>
            <div style={{ fontSize: 26, fontWeight: 800 }} className="tabular">
              {minute}'
            </div>
            <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>{t.match.minute}</div>
          </div>
        </div>

        <div style={{ display: 'flex', gap: 6, margin: '12px 0', flexWrap: 'wrap' }}>
          <button className="btn btn-primary" onClick={() => setPlaying((p) => !p)}>
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
          <button
            className="btn"
            onClick={() => {
              setPlaying(false);
              setMinute(lastMinute);
            }}
          >
            ⏭ {t.match.skip}
          </button>
          <button className="btn" onClick={onClose} style={{ marginLeft: 'auto' }}>
            {t.dashboard.close}
          </button>
        </div>

        <div style={{ maxHeight: 260, overflowY: 'auto' }}>
          {visible.length === 0 && (
            <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.match.noEvents}</div>
          )}
          {visible
            .slice()
            .reverse()
            .map((ev, idx) => (
              <div
                key={`${ev.minute}-${ev.event_type}-${idx}`}
                className={`event-row ${teamName(ev.side) === replay[userIsHome ? 'home_name' : 'away_name'] ? 'mine' : ''}`}
              >
                <span className="event-minute tabular">{ev.minute}'</span>
                <span className="event-label">{label(ev)}</span>
                <span className="event-player">{ev.player_name ?? '—'}</span>
                {ev.detail && <span className="event-detail">{ev.detail}</span>}
                <span className="event-team">{teamName(ev.side)}</span>
              </div>
            ))}
        </div>
      </div>
    </div>
  );
}
