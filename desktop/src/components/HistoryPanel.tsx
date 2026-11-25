import { useEffect, useState } from 'react';
import { desktopApi, HonourRowDto, SeasonAwardRowDto } from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface HistoryPanelProps {
  locale: Locale;
  currentSeasonId: number;
}

const AWARD_LABELS: Record<number, string> = {
  1: 'Top Scorer',
  2: 'MVP',
  3: 'Young Player',
  4: "Ballon d'Or",
  5: 'Golden Glove',
  6: 'Team of the Year',
  7: 'Manager of the Year',
};

export function HistoryPanel({ locale, currentSeasonId }: HistoryPanelProps) {
  const t = getDictionary(locale);
  const [seasons, setSeasons] = useState<{ season_id: number; name: string }[]>([]);
  const [seasonId, setSeasonId] = useState<number>(currentSeasonId);
  const [honours, setHonours] = useState<HonourRowDto[]>([]);
  const [awards, setAwards] = useState<SeasonAwardRowDto[]>([]);

  useEffect(() => {
    void desktopApi.getSeasons().then((list) => {
      setSeasons(list);
      if (list.length > 0) setSeasonId((cur) => (list.some((s) => s.season_id === cur) ? cur : list[0].season_id));
    });
  }, []);

  useEffect(() => {
    if (!seasonId) return;
    void desktopApi.getSeasonHonours(seasonId).then(setHonours);
    void desktopApi.getSeasonAwards(seasonId).then(setAwards);
  }, [seasonId]);

  const group = (predicate: (h: HonourRowDto) => boolean) => honours.filter(predicate);
  const domestic = group((h) => !h.is_national_team && !h.is_international);
  const intlClubs = group((h) => !h.is_national_team && h.is_international);
  const national = group((h) => h.is_national_team);

  const honourCard = (h: HonourRowDto) => (
    <div key={`${h.tournament_id}-${h.edition_season_id}`} className="news-item" style={{ display: 'flex', gap: 8 }}>
      <span style={{ fontSize: 14 }}>🏆</span>
      <div>
        <div style={{ fontWeight: 700, fontSize: 12.5 }}>{h.tournament_name}</div>
        <div style={{ fontSize: 12 }}>
          {h.winner_name}
          {h.runner_up_name ? ` · vs ${h.runner_up_name}` : ''}
          {h.score_line ? ` (${h.score_line})` : ''}
        </div>
        <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
          {[h.third_place_name ? `3rd: ${h.third_place_name}` : null, h.notes]
            .filter(Boolean)
            .join(' · ')}
        </div>
      </div>
    </div>
  );

  const section = (title: string, rows: HonourRowDto[]) =>
    rows.length > 0 && (
      <>
        <div style={{ fontSize: 11, fontWeight: 800, letterSpacing: 0.8, color: 'var(--accent)', margin: '10px 0 4px' }}>
          {title}
        </div>
        {rows.map(honourCard)}
      </>
    );

  return (
    <div className="grid-2">
      <section className="card">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10 }}>
          <h3 className="card-title" style={{ margin: 0 }}>{t.history.honours}</h3>
          <select
            className="select"
            value={seasonId}
            onChange={(e) => setSeasonId(Number(e.target.value))}
          >
            {seasons.map((s) => (
              <option key={s.season_id} value={s.season_id}>
                {s.name}
              </option>
            ))}
          </select>
        </div>
        {honours.length === 0 && (
          <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.history.noHonours}</div>
        )}
        {section(t.history.domestic, domestic)}
        {section(t.history.intlClubs, intlClubs)}
        {section(t.history.nationalTeams, national)}
      </section>

      <section className="card">
        <h3 className="card-title">🏆 {t.history.awards}</h3>
        {awards.length === 0 && (
          <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.history.noAwards}</div>
        )}
        <div style={{ display: 'grid', gap: 8 }}>
          {awards.map((a, idx) => (
            <div
              key={idx}
              className="news-item"
              style={{
                display: 'flex',
                gap: 10,
                alignItems: 'center',
                borderLeft: '3px solid var(--gala-gold)',
                paddingLeft: 8,
              }}
            >
              <span style={{ fontSize: 18 }}>🎖</span>
              <div>
                <div style={{ fontSize: 11.5, fontWeight: 800, color: 'var(--gala-gold)' }}>
                  {(AWARD_LABELS[a.award_type] ?? `Award ${a.award_type}`).toUpperCase()}
                </div>
                <div style={{ fontSize: 13, fontWeight: 700 }}>{a.player_name ?? a.team_name ?? '—'}</div>
                <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                  {[a.team_name, a.subtitle, a.year ? String(a.year) : null].filter(Boolean).join(' · ')}
                </div>
              </div>
              <span className="tabular" style={{ marginLeft: 'auto', fontSize: 12.5 }}>
                {a.goals ? `${a.goals} ⚽` : ''}
                {a.rating ? ` ${a.rating.toFixed(1)}` : ''}
              </span>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
