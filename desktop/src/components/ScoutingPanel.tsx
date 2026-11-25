import { useEffect, useState } from 'react';
import {
  AcademyProspectDto,
  desktopApi,
  formatMoney,
  positionMeta,
  ScoutInfoDto,
  ScoutReportDto,
} from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface ScoutingPanelProps {
  locale: Locale;
  notify: (message: string) => void;
}

export function ScoutingPanel({ locale, notify }: ScoutingPanelProps) {
  const t = getDictionary(locale);
  const [scouts, setScouts] = useState<ScoutInfoDto[]>([]);
  const [reports, setReports] = useState<ScoutReportDto[]>([]);
  const [prospects, setProspects] = useState<AcademyProspectDto[]>([]);
  const [name, setName] = useState('Scout Nusantara');
  const [stars, setStars] = useState(3);
  const [region, setRegion] = useState('europe');
  const [deployRegions, setDeployRegions] = useState<Record<number, string>>({});
  const [busy, setBusy] = useState(false);

  const load = async () => {
    const [s, r, p] = await Promise.all([
      desktopApi.getScouts(),
      desktopApi.getScoutReports(),
      desktopApi.getAcademyProspects(),
    ]);
    setScouts(s);
    setReports(r);
    setProspects(p);
  };

  useEffect(() => {
    void load().catch((e) => notify(String(e)));
  }, []);

  const run = async (fn: () => Promise<unknown>, message?: string) => {
    setBusy(true);
    try {
      const result = await fn();
      await load();
      if (message) notify(message);
      return result;
    } catch (e) {
      notify(String(e));
    } finally {
      setBusy(false);
    }
  };

  const regions = ['europe', 'south_america', 'africa', 'asia', 'north_america'];

  return (
    <div className="grid-2">
      <section className="card">
        <h3 className="card-title">{t.scouting.network}</h3>
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 10 }}>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} style={{ minWidth: 130 }} />
          <select className="select" value={stars} onChange={(e) => setStars(Number(e.target.value))}>
            {[1, 2, 3, 4, 5].map((s) => (
              <option key={s} value={s}>
                {s}★
              </option>
            ))}
          </select>
          <select className="select" value={region} onChange={(e) => setRegion(e.target.value)}>
            {regions.map((r) => (
              <option key={r} value={r}>
                {t.scouting.regions[r as keyof typeof t.scouting.regions] ?? r}
              </option>
            ))}
          </select>
          <button
            className="btn btn-primary"
            disabled={busy || !name.trim()}
            onClick={() =>
              void run(
                () => desktopApi.hireScout(name.trim(), stars, region),
                t.scouting.hired,
              )
            }
          >
            {t.scouting.hire}
          </button>
        </div>

        {scouts.length === 0 && (
          <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.scouting.noScouts}</div>
        )}
        {scouts.map((s) => {
          const targetRegion = deployRegions[s.scout_id] ?? (s.region || 'europe');
          const isDeployed = s.status.toLowerCase() === 'active' || s.status.toLowerCase() === 'scouting';
          return (
            <div key={s.scout_id} className="news-item" style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
              <span className="pill">{s.stars}★</span>
              <span style={{ fontWeight: 700, fontSize: 12.5 }}>{s.name}</span>
              <span style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                {s.status} · {t.scouting.regions[(s.region || '') as keyof typeof t.scouting.regions] ?? (s.region || '—')} · {formatMoney(s.salary)}/w
                {s.return_date ? ` · ${t.scouting.back} ${s.return_date}` : ''}
              </span>
              <div style={{ marginLeft: 'auto', display: 'flex', gap: 6, alignItems: 'center' }}>
                <select
                  className="select"
                  style={{ maxWidth: 130 }}
                  disabled={busy || isDeployed}
                  value={targetRegion}
                  onChange={(e) =>
                    setDeployRegions((cur) => ({ ...cur, [s.scout_id]: e.target.value }))
                  }
                >
                  {regions.map((r) => (
                    <option key={r} value={r}>
                      {t.scouting.regions[r as keyof typeof t.scouting.regions] ?? r}
                    </option>
                  ))}
                </select>
                <button
                  className="btn"
                  disabled={busy || isDeployed}
                  onClick={() =>
                    void run(
                      () => desktopApi.deployScout(s.scout_id, targetRegion),
                      t.scouting.deployed,
                    )
                  }
                >
                  {t.scouting.deploy}
                </button>
              </div>
            </div>
          );
        })}

        <button
          className="btn"
          style={{ marginTop: 10 }}
          disabled={busy}
          onClick={() => void run(() => desktopApi.collectScoutReports(), t.scouting.collected)}
        >
          {t.scouting.collect}
        </button>
      </section>

      <section className="card">
        <h3 className="card-title">{t.scouting.reports}</h3>
        {reports.length === 0 && (
          <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.scouting.noReports}</div>
        )}
        {reports.map((r) => {
          const pm = positionMeta(r.position_id);
          return (
            <div key={r.report_id} className="news-item" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <span className="ovr-chip">{r.power}</span>
              <span className={`pos-badge ${pm.cls}`}>{pm.label}</span>
              <div>
                <div style={{ fontWeight: 700, fontSize: 12.5 }}>{r.player_name}</div>
                <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                  {r.age} · POT {r.potential} · {t.scouting.regions[r.region as keyof typeof t.scouting.regions] ?? r.region} · {r.date}
                </div>
              </div>
              <span className="tabular" style={{ marginLeft: 'auto', fontSize: 11.5 }}>
                {formatMoney(r.value)}
              </span>
              <button
                className="btn"
                disabled={busy}
                onClick={() =>
                  void run(() => desktopApi.signScoutReport(r.report_id), t.scouting.signed)
                }
              >
                {t.scouting.sign}
              </button>
            </div>
          );
        })}

        <h3 className="card-title" style={{ marginTop: 16 }}>{t.scouting.academy}</h3>
        {prospects.length === 0 && (
          <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.scouting.noProspects}</div>
        )}
        {prospects.map((p) => {
          const pm = positionMeta(p.position_id);
          return (
            <div key={p.prospect_id} className="news-item" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <span className="ovr-chip">{p.power}</span>
              <span className={`pos-badge ${pm.cls}`}>{pm.label}</span>
              <div>
                <div style={{ fontWeight: 700, fontSize: 12.5 }}>{p.name}</div>
                <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>
                  {p.age} · POT {p.potential} · {p.stars}★ · {p.height} cm
                </div>
              </div>
              <button
                className="btn"
                style={{ marginLeft: 'auto' }}
                disabled={busy}
                onClick={() =>
                  void run(() => desktopApi.signAcademyProspect(p.prospect_id), t.scouting.promoted)
                }
              >
                {t.scouting.promote}
              </button>
            </div>
          );
        })}
      </section>
    </div>
  );
}
