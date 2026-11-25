import { useEffect, useState } from 'react';
import {
  DashboardSnapshotDto,
  desktopApi,
  FfpMode,
  formatMoney,
  LeagueSummaryDto,
  PlayableCountryDto,
  SaveSlotInfoDto,
  TeamDto,
} from '../api/bridge';
import { getDictionary, Locale } from '../i18n';

interface StartScreenProps {
  locale: Locale;
  onToggleLocale: () => void;
  onCareerReady: (snap: DashboardSnapshotDto) => void;
}

type Mode = 'slots' | 'new_career' | 'custom_club';

export function StartScreen({ locale, onToggleLocale, onCareerReady }: StartScreenProps) {
  const t = getDictionary(locale);
  const [mode, setMode] = useState<Mode>('slots');
  const [slots, setSlots] = useState<SaveSlotInfoDto[]>([]);
  const [selectedSlot, setSelectedSlot] = useState<number>(1);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [archives, setArchives] = useState<string[]>([]);
  const [archive, setArchive] = useState<string>('');
  const [importSlot, setImportSlot] = useState<number>(1);
  const [confirmDeleteSlot, setConfirmDeleteSlot] = useState<number | null>(null);
  const [ffp, setFfp] = useState<FfpMode>('off');

  // New Career Wizard state
  const [managerName, setManagerName] = useState('Coach Naratama');
  const [countries, setCountries] = useState<PlayableCountryDto[]>([]);
  const [countryId, setCountryId] = useState<number>(0);
  const [leagues, setLeagues] = useState<LeagueSummaryDto[]>([]);
  const [leagueId, setLeagueId] = useState<number>(0);
  const [teams, setTeams] = useState<TeamDto[]>([]);
  const [teamId, setTeamId] = useState<number>(0);

  // Custom Club Creator state
  const [clubName, setClubName] = useState('Naratama FC');
  const [abbr, setAbbr] = useState('NRT');
  const [stadiumName, setStadiumName] = useState('Gelanggang Naratama');
  const [preset, setPreset] = useState<'youth' | 'competitive' | 'veteran'>('competitive');
  const [nickname, setNickname] = useState('');
  const [primaryColor, setPrimaryColor] = useState('#E5A93C');
  const [secondaryColor, setSecondaryColor] = useState('#070D18');
  const [badgeShape, setBadgeShape] = useState('shield');
  const [badgeSymbol, setBadgeSymbol] = useState('soccer');
  const [jerseyPattern, setJerseyPattern] = useState('plain');
  const [crest, setCrest] = useState('');
  const [budget, setBudget] = useState<number>(15_000_000);

  const loadSlots = async () => {
    try {
      setSlots(await desktopApi.listSaveSlots());
      const exported = await desktopApi.listSaveExports();
      setArchives(exported);
      setArchive((cur) => (cur && exported.includes(cur) ? cur : (exported[0] ?? '')));
    } catch (err) {
      setError(String(err));
    }
  };

  useEffect(() => {
    void loadSlots();
  }, []);

  const ensureCountries = async () => {
    if (countries.length > 0) return;
    const list = await desktopApi.listPlayableCountries();
    setCountries(list);
    if (list.length > 0) {
      const first = list.find((c) => c.iso2 === 'ID') ?? list[0];
      setCountryId(first.country_id);
    }
  };

  useEffect(() => {
    if (mode !== 'new_career' || !countryId) return;
    let active = true;
    void desktopApi.listLeaguesForCountry(countryId).then((ls) => {
      if (!active) return;
      setLeagues(ls);
      setLeagueId(ls[0]?.tournament_id ?? 0);
    });
    return () => {
      active = false;
    };
  }, [mode, countryId]);

  useEffect(() => {
    if (mode !== 'new_career' || !leagueId) return;
    let active = true;
    void desktopApi.listTeamsForDivision(leagueId).then((ts) => {
      if (!active) return;
      setTeams(ts);
      setTeamId(ts[0]?.team_id ?? 0);
    });
    return () => {
      active = false;
    };
  }, [mode, leagueId]);

  const handleExportSlot = async (slotIndex: number) => {
    setBusy(true);
    setError(null);
    try {
      const path = await desktopApi.exportSaveSlot(slotIndex);
      setNotice(`${t.start.exportedTo}: ${path}`);
      await loadSlots();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleImportSlot = async () => {
    if (!archive) return;
    setBusy(true);
    setError(null);
    try {
      await desktopApi.importSaveSlot(importSlot, archive);
      setNotice(`${t.start.imported} #${importSlot}`);
      await loadSlots();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleLoadSlot = async (slotIndex: number) => {
    setBusy(true);
    setError(null);
    try {
      const snap = await desktopApi.loadCareerSlot(slotIndex);
      onCareerReady(snap);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteSlot = async (slotIndex: number) => {
    setBusy(true);
    setError(null);
    try {
      await desktopApi.deleteSaveSlot(slotIndex);
      await loadSlots();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const openWizard = async (slotIndex: number, target: 'new_career' | 'custom_club') => {
    setSelectedSlot(slotIndex);
    setError(null);
    await ensureCountries();
    setMode(target);
  };

  const handleStartNew = async () => {
    if (!countryId || !teamId) return;
    setBusy(true);
    setError(null);
    try {
      const snap = await desktopApi.startNewCareer(
        selectedSlot,
        managerName,
        countryId,
        teamId,
        ffp,
      );
      onCareerReady(snap);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    if (mode !== 'custom_club') return;
    let active = true;
    void desktopApi
      .previewClubCrest(abbr, primaryColor, secondaryColor, badgeShape, badgeSymbol)
      .then((uri) => {
        if (active) setCrest(uri);
      });
    return () => {
      active = false;
    };
  }, [mode, abbr, primaryColor, secondaryColor, badgeShape, badgeSymbol]);

  const handleStartCustom = async () => {
    if (!countryId || !clubName.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const snap = await desktopApi.startCustomClubCareer({
        slot_index: selectedSlot,
        manager_name: managerName,
        name: clubName.trim(),
        abbreviation: abbr.trim().toUpperCase().slice(0, 3) || 'CLB',
        country_id: countryId,
        stadium_name: stadiumName.trim() || `${clubName.trim()} Arena`,
        preset,
        starting_budget: Math.max(1_000_000, budget),
        ffp_enforcement: ffp,
        nickname: nickname.trim() || undefined,
        primary_color: primaryColor,
        secondary_color: secondaryColor,
        badge_shape: badgeShape as 'shield' | 'circle' | 'diamond' | 'classic',
        badge_symbol: badgeSymbol as 'soccer' | 'star' | 'crown' | 'eagle' | 'flame',
        jersey_pattern: jerseyPattern as 'plain' | 'stripes_vertical' | 'stripes_horizontal' | 'diagonal',
      });
      onCareerReady(snap);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="start-screen">
      <div className="start-container">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div className="topbar-brand">
            <div className="crest-pill" style={{ width: 44, height: 44, fontSize: 16 }}>
              NG
            </div>
            <div>
              <h1 style={{ margin: 0, fontSize: 22 }}>{t.appTitle}</h1>
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>{t.start.tagline}</div>
            </div>
          </div>
          <button className="btn" onClick={onToggleLocale}>
            {locale.toUpperCase()}
          </button>
        </div>

        {error && (
          <div className="card" style={{ borderColor: 'var(--danger)', color: '#fca5a5' }}>
            {error}
          </div>
        )}

        {notice && (
          <div className="card" style={{ fontSize: 12.5 }}>
            {notice}
          </div>
        )}

        {mode === 'slots' && (
          <section className="card">
            <h3 className="card-title">{t.start.saveSlots}</h3>
            <div className="slot-grid">
              {slots.map((slot) => (
                <div key={slot.slot_index} className="slot-card">
                  <div>
                    <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
                      <span className="pill">Slot #{slot.slot_index}</span>
                      {slot.game_date && (
                        <span style={{ fontSize: 11.5, color: 'var(--text-dim)' }} className="tabular">
                          {slot.game_date}
                        </span>
                      )}
                    </div>
                    {slot.is_empty ? (
                      <div style={{ color: 'var(--text-dim)', fontSize: 13 }}>{t.start.emptySlot}</div>
                    ) : (
                      <>
                        <div style={{ fontWeight: 800, fontSize: 15 }}>{slot.club_name ?? 'Club'}</div>
                        <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>
                          {slot.manager_name ?? 'Manager'}
                        </div>
                      </>
                    )}
                  </div>
                  <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                    {!slot.is_empty ? (
                      <>
                        <button
                          className="btn btn-primary"
                          disabled={busy}
                          onClick={() => void handleLoadSlot(slot.slot_index)}
                        >
                          {t.start.load}
                        </button>
                        <button
                          className="btn"
                          disabled={busy}
                          onClick={() => void handleExportSlot(slot.slot_index)}
                        >
                          {t.start.export}
                        </button>
                        {confirmDeleteSlot === slot.slot_index ? (
                          <>
                            <button
                              className="btn btn-danger"
                              disabled={busy}
                              onClick={() => {
                                setConfirmDeleteSlot(null);
                                void handleDeleteSlot(slot.slot_index);
                              }}
                            >
                              {t.start.confirmDelete}
                            </button>
                            <button
                              className="btn"
                              disabled={busy}
                              onClick={() => setConfirmDeleteSlot(null)}
                            >
                              {t.start.cancel}
                            </button>
                          </>
                        ) : (
                          <button
                            className="btn btn-danger"
                            disabled={busy}
                            onClick={() => setConfirmDeleteSlot(slot.slot_index)}
                          >
                            {t.start.delete}
                          </button>
                        )}
                      </>
                    ) : (
                      <>
                        <button
                          className="btn btn-primary"
                          disabled={busy}
                          onClick={() => void openWizard(slot.slot_index, 'new_career')}
                        >
                          {t.start.newCareer}
                        </button>
                        <button
                          className="btn"
                          disabled={busy}
                          onClick={() => void openWizard(slot.slot_index, 'custom_club')}
                        >
                          {t.start.customClub}
                        </button>
                      </>
                    )}
                  </div>
                </div>
              ))}
            </div>
            <div className="form-grid" style={{ marginTop: 14 }}>
              <label className="form-field">
                {t.start.savesArchive}
                <select
                  className="select"
                  value={archive}
                  onChange={(e) => setArchive(e.target.value)}
                >
                  {archives.length === 0 ? (
                    <option value="">{t.start.noArchives}</option>
                  ) : (
                    archives.map((path) => (
                      <option key={path} value={path}>
                        {path.split(/[\\/]/).pop() ?? path}
                      </option>
                    ))
                  )}
                </select>
              </label>
              <label className="form-field">
                {t.start.importTarget}
                <select
                  className="select"
                  value={importSlot}
                  onChange={(e) => setImportSlot(Number(e.target.value))}
                >
                  {slots.map((s) => (
                    <option key={s.slot_index} value={s.slot_index}>
                      Slot #{s.slot_index}
                    </option>
                  ))}
                </select>
              </label>
              <div className="form-field" style={{ justifyContent: 'flex-end' }}>
                <button
                  className="btn"
                  disabled={busy || !archive}
                  onClick={() => void handleImportSlot()}
                >
                  {t.start.importAction}
                </button>
              </div>
            </div>
          </section>
        )}

        {mode === 'new_career' && (
          <section className="card">
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 14 }}>
              <h3 className="card-title" style={{ margin: 0 }}>
                {t.start.newCareer} · Slot #{selectedSlot}
              </h3>
              <button className="btn" onClick={() => setMode('slots')}>
                ← {t.start.back}
              </button>
            </div>
            <div className="form-grid">
              <label className="form-field">
                {t.start.managerName}
                <input
                  className="input"
                  value={managerName}
                  onChange={(e) => setManagerName(e.target.value)}
                />
              </label>
              <label className="form-field">
                {t.start.selectCountry}
                <select
                  className="select"
                  value={countryId}
                  onChange={(e) => setCountryId(Number(e.target.value))}
                >
                  {countries.map((c) => (
                    <option key={c.country_id} value={c.country_id}>
                      {c.name} ({c.team_count} clubs)
                    </option>
                  ))}
                </select>
              </label>
              <label className="form-field">
                {t.start.selectLeague}
                <select
                  className="select"
                  value={leagueId}
                  onChange={(e) => setLeagueId(Number(e.target.value))}
                >
                  {leagues.map((l) => (
                    <option key={l.tournament_id} value={l.tournament_id}>
                      Div {l.division_id} · {l.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="form-field">
                {t.start.selectClub}
                <select
                  className="select"
                  value={teamId}
                  onChange={(e) => setTeamId(Number(e.target.value))}
                >
                  {teams.map((tm) => (
                    <option key={tm.team_id} value={tm.team_id}>
                      {tm.name} (OVR {tm.power} · {formatMoney(tm.budget)})
                    </option>
                  ))}
                </select>
              </label>
              <label className="form-field">
                {t.start.ffp}
                <select
                  className="select"
                  value={ffp}
                  onChange={(e) => setFfp(e.target.value as FfpMode)}
                >
                  <option value="off">{t.start.ffpOff}</option>
                  <option value="relaxed">{t.start.ffpRelaxed}</option>
                  <option value="standard">{t.start.ffpStandard}</option>
                  <option value="strict">{t.start.ffpStrict}</option>
                </select>
              </label>
            </div>
            <div style={{ marginTop: 16, display: 'flex', justifyContent: 'flex-end' }}>
              <button className="btn btn-primary" disabled={busy || !teamId} onClick={() => void handleStartNew()}>
                ▶ {t.start.startNow}
              </button>
            </div>
          </section>
        )}

        {mode === 'custom_club' && (
          <section className="card">
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 14 }}>
              <h3 className="card-title" style={{ margin: 0 }}>
                {t.start.customClub} · Slot #{selectedSlot}
              </h3>
              <button className="btn" onClick={() => setMode('slots')}>
                ← {t.start.back}
              </button>
            </div>
            <div className="form-grid">
              <label className="form-field">
                {t.start.managerName}
                <input className="input" value={managerName} onChange={(e) => setManagerName(e.target.value)} />
              </label>
              <label className="form-field">
                {t.start.selectCountry}
                <select className="select" value={countryId} onChange={(e) => setCountryId(Number(e.target.value))}>
                  {countries.map((c) => (
                    <option key={c.country_id} value={c.country_id}>
                      {c.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="form-field">
                {t.start.clubName}
                <input className="input" value={clubName} onChange={(e) => setClubName(e.target.value)} />
              </label>
              <label className="form-field">
                {t.start.abbr}
                <input className="input" maxLength={3} value={abbr} onChange={(e) => setAbbr(e.target.value)} />
              </label>
              <label className="form-field">
                {t.start.stadiumName}
                <input className="input" value={stadiumName} onChange={(e) => setStadiumName(e.target.value)} />
              </label>
              <label className="form-field">
                {t.start.nickname}
                <input className="input" value={nickname} onChange={(e) => setNickname(e.target.value)} />
              </label>
              <label className="form-field">
                {t.start.badgeShape}
                <select className="select" value={badgeShape} onChange={(e) => setBadgeShape(e.target.value)}>
                  <option value="shield">{t.start.shield}</option>
                  <option value="circle">{t.start.circle}</option>
                  <option value="diamond">{t.start.diamond}</option>
                  <option value="classic">{t.start.classic}</option>
                </select>
              </label>
              <label className="form-field">
                {t.start.badgeSymbol}
                <select className="select" value={badgeSymbol} onChange={(e) => setBadgeSymbol(e.target.value)}>
                  <option value="soccer">{t.start.soccer}</option>
                  <option value="star">{t.start.star}</option>
                  <option value="crown">{t.start.crown}</option>
                  <option value="eagle">{t.start.eagle}</option>
                  <option value="flame">{t.start.flame}</option>
                </select>
              </label>
              <label className="form-field">
                {t.start.jerseyPattern}
                <select className="select" value={jerseyPattern} onChange={(e) => setJerseyPattern(e.target.value)}>
                  <option value="plain">{t.start.plain}</option>
                  <option value="stripes_vertical">{t.start.stripesVertical}</option>
                  <option value="stripes_horizontal">{t.start.stripesHorizontal}</option>
                  <option value="diagonal">{t.start.diagonal}</option>
                </select>
              </label>
              <label className="form-field">
                {t.start.primaryColor}
                <input className="input" type="color" value={primaryColor} onChange={(e) => setPrimaryColor(e.target.value)} />
              </label>
              <label className="form-field">
                {t.start.secondaryColor}
                <input className="input" type="color" value={secondaryColor} onChange={(e) => setSecondaryColor(e.target.value)} />
              </label>
              <label className="form-field">
                {t.start.preset}
                <select
                  className="select"
                  value={preset}
                  onChange={(e) => setPreset(e.target.value as 'youth' | 'competitive' | 'veteran')}
                >
                  <option value="youth">Youth Project</option>
                  <option value="competitive">Competitive</option>
                  <option value="veteran">Veteran Core</option>
                </select>
              </label>
              <label className="form-field">
                {t.start.budget}
                <input
                  className="input tabular"
                  type="number"
                  min={1_000_000}
                  step={1_000_000}
                  value={budget}
                  onChange={(e) => setBudget(Number(e.target.value))}
                />
              </label>
              <label className="form-field">
                {t.start.ffp}
                <select
                  className="select"
                  value={ffp}
                  onChange={(e) => setFfp(e.target.value as FfpMode)}
                >
                  <option value="off">{t.start.ffpOff}</option>
                  <option value="relaxed">{t.start.ffpRelaxed}</option>
                  <option value="standard">{t.start.ffpStandard}</option>
                  <option value="strict">{t.start.ffpStrict}</option>
                </select>
              </label>
            </div>
            <div style={{ display: 'flex', gap: 12, alignItems: 'center', marginTop: 14 }}>
              {crest && (
                <img
                  src={crest}
                  alt={abbr}
                  style={{ width: 84, height: 84, borderRadius: 10, background: '#0b1424' }}
                />
              )}
              <div style={{ fontSize: 12.5, color: 'var(--text-dim)' }}>
                <div style={{ fontWeight: 700, color: 'var(--text)' }}>
                  {clubName} {nickname ? `· ${nickname}` : ''}
                </div>
                <div>
                  {abbr} · {t.start.preview}
                </div>
              </div>
            </div>
            <div style={{ marginTop: 16, display: 'flex', justifyContent: 'flex-end' }}>
              <button className="btn btn-primary" disabled={busy || !countryId} onClick={() => void handleStartCustom()}>
                ▶ {t.start.startNow}
              </button>
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

