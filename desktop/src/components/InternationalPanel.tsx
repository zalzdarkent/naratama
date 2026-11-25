import { useEffect, useState } from 'react';
import {
  desktopApi,
  DutyRequestDto,
  HonourRowDto,
  NationalLeaderDto,
  NationalSquadPlayerDto,
  positionMeta,
} from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface InternationalPanelProps {
  locale: Locale;
  notify: (message: string) => void;
}

export function InternationalPanel({ locale, notify }: InternationalPanelProps) {
  const t = getDictionary(locale);
  const [podiums, setPodiums] = useState<HonourRowDto[]>([]);
  const [duties, setDuties] = useState<DutyRequestDto[]>([]);
  const [countries, setCountries] = useState<[number, string][]>([]);
  const [countryId, setCountryId] = useState<number>(0);
  const [squad, setSquad] = useState<NationalSquadPlayerDto[]>([]);
  const [leaders, setLeaders] = useState<NationalLeaderDto[]>([]);

  const load = async () => {
    const [p, d, c] = await Promise.all([
      desktopApi.getInternationalPodiums(),
      desktopApi.getDutyRequests(),
      desktopApi.getCalledUpCountries(),
    ]);
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

  return (
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

        <h3 className="card-title" style={{ marginTop: 14 }}>{t.intl.calledSquad}</h3>
        {countries.length > 0 && (
          <select
            className="select"
            value={countryId}
            onChange={(e) => setCountryId(Number(e.target.value))}
            style={{ marginBottom: 8 }}
          >
            {countries.map(([id, name]) => (
              <option key={id} value={id}>
                {name}
              </option>
            ))}
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
        {leaders.length > 0 && (
          <div style={{ marginTop: 14 }}>
            {(
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
                <div key={label}>
                  <h4 style={{ fontSize: 12.5, margin: '10px 0 4px', color: 'var(--accent)' }}>
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
          </div>
        )}
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
  );
}
