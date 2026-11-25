import { useState } from 'react';
import { DashboardSnapshotDto, desktopApi, OffseasonStateDto } from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface OffseasonModalProps {
  locale: Locale;
  offseason: OffseasonStateDto | null;
  onUpdated: (snap: DashboardSnapshotDto) => void;
  onClose: () => void;
}

export function OffseasonModal({ locale, offseason, onUpdated, onClose }: OffseasonModalProps) {
  const t = getDictionary(locale);
  const [camp, setCamp] = useState<'tactical' | 'fitness' | 'youth'>('tactical');
  const [ambition, setAmbition] = useState<'consolidate' | 'top_half' | 'title'>('top_half');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const step = offseason?.step ?? null;

  const runStep = async (action: string, param?: string) => {
    setBusy(true);
    setError(null);
    try {
      const snap = await desktopApi.offseasonAdvance(action, param);
      onUpdated(snap);
      if (action === 'start_next_season') {
        onClose();
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop">
      <div className="modal-box">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h3 className="card-title" style={{ margin: 0 }}>
            {t.offseason.title}
          </h3>
          <button className="btn" onClick={onClose}>
            {t.dashboard.close}
          </button>
        </div>

        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
          {(
            [
              ['Summary', t.offseason.begin],
              ['Contracts', t.offseason.contracts],
              ['Development', t.offseason.development],
              ['Transfers', t.offseason.transfers],
              ['Preseason', t.offseason.camp],
              ['Board', t.offseason.board],
              ['NewSeason', t.offseason.startNext],
            ] as const
          ).map(([key, label]) => (
            <span
              key={key}
              className="pill"
              style={
                step === key
                  ? { borderColor: 'var(--accent)', color: 'var(--accent-light)' }
                  : undefined
              }
            >
              {label}
            </span>
          ))}
        </div>

        {offseason?.report && (
          <div className="card" style={{ background: 'var(--surface-card-high)', fontSize: 13 }}>
            {offseason.report}
          </div>
        )}

        {error && (
          <div className="card" style={{ borderColor: 'var(--danger)', color: '#fca5a5' }}>
            {error}
          </div>
        )}

        {step === 'Transfers' && (
          <label className="form-field">
            {t.offseason.camp}
            <select
              className="select"
              value={camp}
              onChange={(e) => setCamp(e.target.value as 'tactical' | 'fitness' | 'youth')}
            >
              <option value="tactical">Tactical Retreat</option>
              <option value="fitness">Fitness Bootcamp</option>
              <option value="youth">Youth Showcase</option>
            </select>
          </label>
        )}

        {step === 'Preseason' && (
          <label className="form-field">
            {t.offseason.board}
            <select
              className="select"
              value={ambition}
              onChange={(e) => setAmbition(e.target.value as 'consolidate' | 'top_half' | 'title')}
            >
              <option value="consolidate">Consolidate Division</option>
              <option value="top_half">Top Half Finish</option>
              <option value="title">Title Challenge</option>
            </select>
          </label>
        )}

        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          {!step && (
            <button className="btn btn-primary" disabled={busy} onClick={() => void runStep('begin')}>
              {t.offseason.begin} →
            </button>
          )}
          {step === 'Summary' && (
            <button className="btn btn-primary" disabled={busy} onClick={() => void runStep('contracts')}>
              {t.offseason.contracts} →
            </button>
          )}
          {step === 'Contracts' && (
            <button className="btn btn-primary" disabled={busy} onClick={() => void runStep('development')}>
              {t.offseason.development} →
            </button>
          )}
          {step === 'Development' && (
            <button className="btn btn-primary" disabled={busy} onClick={() => void runStep('transfers')}>
              {t.offseason.transfers} →
            </button>
          )}
          {step === 'Transfers' && (
            <button className="btn btn-primary" disabled={busy} onClick={() => void runStep('camp', camp)}>
              {t.offseason.camp} →
            </button>
          )}
          {step === 'Preseason' && (
            <button className="btn btn-primary" disabled={busy} onClick={() => void runStep('board', ambition)}>
              {t.offseason.board} →
            </button>
          )}
          {(step === 'Board' || step === 'NewSeason') && (
            <button
              className="btn btn-primary"
              disabled={busy}
              onClick={() => void runStep('start_next_season')}
            >
              ▶ {t.offseason.startNext}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

