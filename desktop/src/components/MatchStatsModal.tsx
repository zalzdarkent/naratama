import { MatchStatisticsDto } from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface MatchStatsModalProps {
  locale: Locale;
  stats: MatchStatisticsDto;
  userTeamId: number;
  onClose: () => void;
  onOpenTeam: (teamId: number) => void;
}

const pct = (value: number, total: number) => (total <= 0 ? 0 : Math.round((value * 100) / total));

export function MatchStatsModal({
  locale,
  stats,
  userTeamId,
  onClose,
  onOpenTeam,
}: MatchStatsModalProps) {
  const t = getDictionary(locale);

  const yellowHome = stats.players
    .filter((p) => p.team_id === stats.home_team_id)
    .reduce((sum, p) => sum + p.yellow_cards, 0);
  const yellowAway = stats.players
    .filter((p) => p.team_id === stats.away_team_id)
    .reduce((sum, p) => sum + p.yellow_cards, 0);
  const redHome = stats.players
    .filter((p) => p.team_id === stats.home_team_id)
    .reduce((sum, p) => sum + p.red_cards, 0);
  const redAway = stats.players
    .filter((p) => p.team_id === stats.away_team_id)
    .reduce((sum, p) => sum + p.red_cards, 0);

  const bar = (label: string, home: number, away: number, suffix = '') => {
    const total = home + away;
    const homeShare = total <= 0 ? 50 : Math.min(99, Math.max(1, Math.round((home * 100) / total)));
    return (
      <div key={label} style={{ marginBottom: 8 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
          <strong className="tabular">
            {home}
            {suffix}
          </strong>
          <span style={{ color: 'var(--text-dim)' }}>{label}</span>
          <strong className="tabular">
            {away}
            {suffix}
          </strong>
        </div>
        <div style={{ display: 'flex', height: 6, borderRadius: 3, overflow: 'hidden', marginTop: 3 }}>
          <div style={{ width: `${homeShare}%`, background: 'var(--accent)' }} />
          <div style={{ flex: 1, background: 'var(--surface-card-high)' }} />
        </div>
      </div>
    );
  };

  const teamBlock = (teamId: number, name: string) => {
    const rows = stats.players.filter((p) => p.team_id === teamId);
    const isUser = teamId === userTeamId;
    return (
      <div style={{ marginBottom: 14 }}>
        <div style={{ display: 'flex', gap: 6, alignItems: 'baseline', marginBottom: 4 }}>
          <span
            style={{ fontWeight: 800, fontSize: 13, cursor: 'pointer', color: isUser ? 'var(--accent-light)' : undefined }}
            onClick={() => onOpenTeam(teamId)}
          >
            {name}
          </span>
          {isUser && <span className="pill">{t.squad.starter}</span>}
        </div>
        <table className="data-table tabular">
          <thead>
            <tr>
              <th>{t.squad.player}</th>
              <th>{t.stats.minutes}</th>
              <th>{t.stats.goals}</th>
              <th>{t.stats.assists}</th>
              <th>{t.stats.cards}</th>
              <th>{t.stats.rating}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((p) => (
              <tr key={p.player_id}>
                <td style={{ fontWeight: p.mvp ? 800 : 500 }}>
                  {p.player_name}
                  {p.mvp ? ' ★' : ''}
                  {!p.starter ? <span style={{ color: 'var(--text-dim)' }}> (sub)</span> : ''}
                </td>
                <td>{p.minutes}</td>
                <td>{p.goals}</td>
                <td>{p.assists}</td>
                <td>
                  {p.yellow_cards > 0 && '🟨'.repeat(p.yellow_cards)}
                  {p.red_cards > 0 && '🟥'.repeat(p.red_cards)}
                </td>
                <td style={{ color: 'var(--accent-light)', fontWeight: 700 }}>
                  {(p.rating / 10).toFixed(1)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-box" onClick={(e) => e.stopPropagation()}>
        <div style={{ textAlign: 'center' }}>
          <div style={{ fontSize: 11, letterSpacing: 1, color: 'var(--text-dim)' }}>
            {stats.tournament_name.toUpperCase()} · {stats.date ?? ''}
          </div>
          <div style={{ fontSize: 11, letterSpacing: 1, color: 'var(--text-dim)', marginTop: 8 }}>
            {t.stats.fullTime}
          </div>
          <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 16, marginTop: 4 }}>
            <span
              style={{ fontSize: 18, fontWeight: 800, cursor: 'pointer', display: 'inline-flex', gap: 6, alignItems: 'center' }}
              onClick={() => onOpenTeam(stats.home_team_id)}
            >
              {stats.home_name}
            </span>
            <span className="tabular" style={{ fontSize: 26, fontWeight: 900 }}>
              {stats.home_goals} - {stats.away_goals}
            </span>
            <span
              style={{ fontSize: 18, fontWeight: 800, cursor: 'pointer' }}
              onClick={() => onOpenTeam(stats.away_team_id)}
            >
              {stats.away_name}
            </span>
          </div>
          {stats.penalty_home_goals !== null &&
            stats.penalty_away_goals !== null &&
            (stats.penalty_home_goals > 0 || stats.penalty_away_goals > 0) && (
              <div style={{ fontSize: 12, color: 'var(--accent-light)' }} className="tabular">
                {t.match.penalties}: {stats.penalty_home_goals} - {stats.penalty_away_goals}
              </div>
            )}
          <div style={{ fontSize: 11.5, color: 'var(--text-dim)', marginTop: 4 }} className="tabular">
            {t.stats.attendance}: {stats.attendance.toLocaleString()} · {t.stats.revenue}:{' '}
            {stats.revenue.toLocaleString()}
          </div>
        </div>

        <div style={{ marginTop: 14 }}>
          {bar(t.stats.possession, stats.home_possession, 100 - stats.home_possession, '%')}
          {bar(t.stats.shots, stats.home_shots, stats.away_shots)}
          {bar(t.stats.shotsOnTarget, stats.home_shots_on_target, stats.away_shots_on_target)}
          {bar(
            t.stats.accuracy,
            pct(stats.home_shots_on_target, stats.home_shots),
            pct(stats.away_shots_on_target, stats.away_shots),
            '%',
          )}
          {bar(t.stats.saves, stats.home_saves, stats.away_saves)}
          {bar(t.stats.yellowCards, yellowHome, yellowAway)}
          {bar(t.stats.redCards, redHome, redAway)}
        </div>

        <div style={{ display: 'flex', justifyContent: 'flex-end', margin: '6px 0 10px' }}>
          <button className="btn" onClick={onClose}>
            {t.dashboard.close}
          </button>
        </div>

        <div style={{ maxHeight: 320, overflowY: 'auto' }}>
          {teamBlock(stats.home_team_id, stats.home_name)}
          {teamBlock(stats.away_team_id, stats.away_name)}
        </div>
      </div>
    </div>
  );
}
