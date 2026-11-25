import { DossierDto, positionMeta } from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface DossierModalProps {
  locale: Locale;
  teamName: string;
  dossier: DossierDto;
  onClose: () => void;
}

const humanize = (raw: string) =>
  raw
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/([A-Z])([A-Z][a-z])/g, '$1 $2')
    .trim();

export function DossierModal({ locale, teamName, dossier, onClose }: DossierModalProps) {
  const t = getDictionary(locale);
  const confidence = Math.min(5, Math.max(0, Math.round(dossier.confidence_rating / 20)));

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-box" onClick={(e) => e.stopPropagation()}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
          <div>
            <div style={{ fontSize: 11.5, color: 'var(--text-dim)' }}>{t.dossier.title}</div>
            <div style={{ fontSize: 19, fontWeight: 800 }}>{teamName}</div>
            <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
              {dossier.formation} · {humanize(dossier.tactical_style)} · {humanize(dossier.predicted_mentality)}
            </div>
          </div>
          <button className="btn" onClick={onClose}>
            {t.dashboard.close}
          </button>
        </div>

        <div style={{ display: 'flex', gap: 8, margin: '10px 0' }} className="tabular">
          <span className="pill">
            {t.dossier.outlook}: <strong>{humanize(dossier.match_outlook)}</strong>
          </span>
          <span className="pill">
            {t.dossier.confidence}: <strong>{confidence}/5</strong>
          </span>
        </div>

        <h4 style={{ fontSize: 13, margin: '10px 0 6px', color: 'var(--danger)' }}>
          {t.dossier.dangerMen}
        </h4>
        {dossier.danger_men.map((p) => {
          const pm = positionMeta(p.position_id);
          return (
            <div key={p.player_id} className="news-item" style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <span className="ovr-chip">{p.power}</span>
              <span className={`pos-badge ${pm.cls}`}>{pm.label}</span>
              <span style={{ fontWeight: 600, fontSize: 12.5 }}>{p.name}</span>
            </div>
          );
        })}

        <h4 style={{ fontSize: 13, margin: '10px 0 6px', color: 'var(--warn)' }}>
          {t.dossier.vulnerabilities}
        </h4>
        {dossier.vulnerabilities.map((v, idx) => {
          const entry = Object.entries(v)[0] ?? ['', ''];
          return (
            <div key={idx} className="news-item" style={{ fontSize: 12.5 }}>
              • {humanize(String(entry[0]))}
              {typeof entry[1] === 'object' && entry[1] !== null
                ? `: ${Object.values(entry[1] as Record<string, unknown>).join(' · ')}`
                : entry[1] !== '' && entry[1] !== undefined
                  ? `: ${String(entry[1])}`
                  : ''}
            </div>
          );
        })}

        <h4 style={{ fontSize: 13, margin: '10px 0 6px', color: 'var(--accent)' }}>
          {t.dossier.counterTactics}
        </h4>
        {dossier.counter_tactics.map((c, idx) => (
          <div key={idx} className="news-item" style={{ fontSize: 12.5 }}>
            • {humanize(c)}
          </div>
        ))}
      </div>
    </div>
  );
}
