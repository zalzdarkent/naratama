import { useEffect, useState } from 'react';
import { desktopApi, formatMoney, TeamDetailDto } from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface TeamDetailModalProps {
  locale: Locale;
  teamId: number;
  userTeamId: number;
  onClose: () => void;
  onOpenPlayer: (playerId: number) => void;
  onOpenDossier: (teamId: number) => void;
}

export function TeamDetailModal({
  locale,
  teamId,
  userTeamId,
  onClose,
  onOpenPlayer,
  onOpenDossier,
}: TeamDetailModalProps) {
  const t = getDictionary(locale);
  const [detail, setDetail] = useState<TeamDetailDto | null>(null);

  useEffect(() => {
    let active = true;
    void desktopApi.getTeamDetail(teamId).then((d) => {
      if (active) setDetail(d);
    });
    return () => {
      active = false;
    };
  }, [teamId]);

  const capacity = detail?.stadium
    ? detail.stadium.capacity_general +
      detail.stadium.capacity_stand +
      detail.stadium.capacity_terrace +
      detail.stadium.capacity_boxes
    : 0;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-box" onClick={(e) => e.stopPropagation()}>
        {!detail ? (
          <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.common.loading}</div>
        ) : (
          <>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
              <div>
                <div style={{ fontSize: 20, fontWeight: 800 }}>{detail.team.name}</div>
                <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
                  {detail.team.abbreviation} · {detail.league_name} · {t.team.division} {detail.team.division}
                </div>
              </div>
              <button className="btn" onClick={onClose}>
                {t.dashboard.close}
              </button>
            </div>

            <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', margin: '12px 0' }}>
              <span className="pill tabular">
                {t.team.overall}: <strong>{detail.team.power}</strong>
              </span>
              <span className="pill tabular">
                {t.team.budget}: <strong>{formatMoney(detail.team.budget)}</strong>
              </span>
              <span className="pill tabular">
                {t.team.squadValue}: <strong>{formatMoney(detail.squad_value)}</strong>
              </span>
              <span className="pill tabular">
                {t.team.squad}: <strong>{detail.squad_size}</strong>
              </span>
              <span className="pill tabular">
                {t.team.avgPower}: <strong>{detail.average_power}</strong>
              </span>
              {detail.stadium && (
                <span className="pill tabular">
                  {detail.stadium.name} · {capacity.toLocaleString()}
                </span>
              )}
            </div>

            {teamId !== userTeamId && (
              <button className="btn btn-primary" style={{ marginBottom: 12 }} onClick={() => onOpenDossier(teamId)}>
                {t.team.dossier}
              </button>
            )}

            <div className="grid-2">
              <div>
                <h4 style={{ fontSize: 13, margin: '0 0 6px' }}>{t.team.keyPlayers}</h4>
                {detail.top_players.map((p) => (
                  <div
                    key={p.player_id}
                    className="news-item"
                    style={{ display: 'flex', gap: 8, cursor: 'pointer' }}
                    onClick={() => onOpenPlayer(p.player_id)}
                  >
                    <span className="ovr-chip">{p.power}</span>
                    <span style={{ fontWeight: 600, fontSize: 12.5 }}>{p.name}</span>
                    <span style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                      {p.age} · {p.position_id}
                    </span>
                    <span className="tabular" style={{ marginLeft: 'auto', fontSize: 11.5 }}>
                      {formatMoney(p.value)}
                    </span>
                  </div>
                ))}
              </div>
              <div>
                <h4 style={{ fontSize: 13, margin: '0 0 6px' }}>{t.team.recent}</h4>
                {detail.recent_results.length === 0 && (
                  <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>{t.team.noMatches}</div>
                )}
                {detail.recent_results.map((f) => (
                  <div key={f.match_id} className="news-item" style={{ display: 'flex', gap: 6, fontSize: 12 }}>
                    <span style={{ color: 'var(--text-dim)' }}>{f.date ?? ''}</span>
                    <span>
                      {f.home_name} {f.home_goals} - {f.away_goals} {f.away_name}
                    </span>
                  </div>
                ))}
                <h4 style={{ fontSize: 13, margin: '10px 0 6px' }}>{t.team.next}</h4>
                {detail.next_fixtures.map((f) => (
                  <div key={f.match_id} className="news-item" style={{ display: 'flex', gap: 6, fontSize: 12 }}>
                    <span style={{ color: 'var(--text-dim)' }}>{f.date ?? ''}</span>
                    <span>
                      {f.home_name} vs {f.away_name}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
