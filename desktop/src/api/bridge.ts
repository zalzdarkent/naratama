export interface TeamDto {
  team_id: number;
  name: string;
  abbreviation: string;
  country_id: number;
  power: number;
  budget: number;
  division: number;
}

export interface PlayerDto {
  player_id: number;
  name: string;
  age: number;
  position_id: number;
  power: number;
  potential: number;
  fitness: number;
  value: number;
  salary: number;
  is_captain: boolean;
}

export interface StandingRowDto {
  team_id: number;
  team_name: string;
  played: number;
  wins: number;
  draws: number;
  losses: number;
  goals_for: number;
  goals_against: number;
  points: number;
}

export interface FixtureMatchDto {
  match_id: number;
  tournament_id: number;
  home_team_id: number;
  away_team_id: number;
  date: string | null;
  home_goals: number | null;
  away_goals: number | null;
}

export interface NewsItemDto {
  news_id: number;
  date: string;
  scope: string;
  category: string;
  headline: string;
  body: string;
  importance: number;
}

export interface InboxMessageDto {
  message_id: number;
  title: string;
  body: string;
  date: string;
  sender: string;
  is_read: boolean;
}

export interface CountryRankDto {
  country_id: number;
  name: string;
  iso2: string;
  power: number;
  points: number;
  previous_rank: number | null;
}

export interface DashboardSnapshotDto {
  season: { season_id: number; name: string; start_year: number; end_year: number };
  club: TeamDto;
  current_date: string;
  transfer_window_open: boolean;
  transfer_window_label: string;
  fan_mood: number;
  unread_inbox: number;
  next_match: FixtureMatchDto | null;
  standings_preview: StandingRowDto[];
  recent_news: NewsItemDto[];
  recent_inbox: InboxMessageDto[];
  weekly_payroll: number;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__?.invoke) {
    return window.__TAURI_INTERNALS__.invoke<T>(cmd, args);
  }
  return null;
}

const PREVIEW_SQUAD: PlayerDto[] = [
  { player_id: 1, name: 'A. Pratama', age: 25, position_id: 1, power: 77, potential: 81, fitness: 96, value: 8_500_000, salary: 28_000, is_captain: false },
  { player_id: 2, name: 'R. Kusuma', age: 27, position_id: 2, power: 76, potential: 78, fitness: 91, value: 7_200_000, salary: 26_000, is_captain: true },
  { player_id: 3, name: 'M. Santoso', age: 24, position_id: 3, power: 75, potential: 80, fitness: 94, value: 6_800_000, salary: 22_000, is_captain: false },
  { player_id: 4, name: 'D. Wijaya', age: 23, position_id: 3, power: 74, potential: 82, fitness: 88, value: 6_400_000, salary: 20_000, is_captain: false },
  { player_id: 5, name: 'B. Nugroho', age: 26, position_id: 4, power: 75, potential: 77, fitness: 92, value: 6_100_000, salary: 21_000, is_captain: false },
  { player_id: 6, name: 'F. Hidayat', age: 22, position_id: 6, power: 78, potential: 85, fitness: 95, value: 12_500_000, salary: 34_000, is_captain: false },
  { player_id: 7, name: 'H. Mahendra', age: 25, position_id: 7, power: 77, potential: 81, fitness: 90, value: 10_200_000, salary: 30_000, is_captain: false },
  { player_id: 8, name: 'I. Saputra', age: 21, position_id: 8, power: 76, potential: 86, fitness: 93, value: 11_000_000, salary: 25_000, is_captain: false },
  { player_id: 9, name: 'K. Wibowo', age: 28, position_id: 7, power: 74, potential: 75, fitness: 87, value: 5_400_000, salary: 24_000, is_captain: false },
  { player_id: 10, name: 'Y. Gunawan', age: 24, position_id: 10, power: 80, potential: 86, fitness: 97, value: 18_400_000, salary: 48_000, is_captain: false },
  { player_id: 11, name: 'E. Setiawan', age: 20, position_id: 11, power: 75, potential: 88, fitness: 95, value: 13_200_000, salary: 23_000, is_captain: false },
];

const PREVIEW_SNAPSHOT: DashboardSnapshotDto = {
  season: { season_id: 1, name: 'Season 2025/26', start_year: 2025, end_year: 2026 },
  club: {
    team_id: 101,
    name: 'Naratama United FC',
    abbreviation: 'NRU',
    country_id: 94,
    power: 76,
    budget: 24_800_000,
    division: 1,
  },
  current_date: '2025-08-23',
  transfer_window_open: true,
  transfer_window_label: 'OPEN (9d)',
  fan_mood: 74,
  unread_inbox: 2,
  next_match: {
    match_id: 1001,
    tournament_id: 1,
    home_team_id: 101,
    away_team_id: 102,
    date: '2025-08-23',
    home_goals: null,
    away_goals: null,
  },
  standings_preview: [
    { team_id: 101, team_name: 'Naratama United FC', played: 2, wins: 2, draws: 0, losses: 0, goals_for: 5, goals_against: 1, points: 6 },
    { team_id: 102, team_name: 'Garuda Nusantara', played: 2, wins: 1, draws: 1, losses: 0, goals_for: 4, goals_against: 2, points: 4 },
    { team_id: 103, team_name: 'Persib Bandung', played: 2, wins: 1, draws: 1, losses: 0, goals_for: 3, goals_against: 2, points: 4 },
    { team_id: 104, team_name: 'Persija Jakarta', played: 2, wins: 1, draws: 0, losses: 1, goals_for: 3, goals_against: 3, points: 3 },
    { team_id: 105, team_name: 'Bali United', played: 2, wins: 0, draws: 1, losses: 1, goals_for: 1, goals_against: 3, points: 1 },
  ],
  recent_news: [
    {
      news_id: 1,
      date: '2025-08-20',
      scope: 'league',
      category: 'preview',
      headline: 'Naratama United FC lead early title pace after flying start',
      body: 'Analysts praise the balanced 4-4-2 structure and clinical transition play.',
      importance: 4,
    },
    {
      news_id: 2,
      date: '2025-08-18',
      scope: 'world',
      category: 'transfer',
      headline: 'DONE DEAL: Summer deadline week heats up across top leagues',
      body: 'Clubs across Europe and Asia finalize key reinforcements before the registration window shuts.',
      importance: 4,
    },
  ],
  recent_inbox: [
    {
      message_id: 1,
      title: 'Broadcast installment received',
      body: 'The league distributed the kickoff broadcast pool share.',
      date: '2025-08-15',
      sender: 'League Office',
      is_read: false,
    },
    {
      message_id: 2,
      title: 'Youth Academy Intake: 2 new prospects',
      body: 'Two U18 talents are ready for review in your academy.',
      date: '2025-08-12',
      sender: 'Youth Academy',
      is_read: false,
    },
  ],
  weekly_payroll: 301_000,
};

const PREVIEW_FIFA: CountryRankDto[] = [
  { country_id: 11, name: 'Argentina', iso2: 'AR', power: 86, points: 1889.02, previous_rank: 1 },
  { country_id: 71, name: 'France', iso2: 'FR', power: 86, points: 1851.92, previous_rank: 2 },
  { country_id: 64, name: 'Spain', iso2: 'ES', power: 85, points: 1836.42, previous_rank: 3 },
  { country_id: 44, name: 'England', iso2: 'GB-ENG', power: 85, points: 1738.72, previous_rank: 4 },
  { country_id: 94, name: 'Indonesia', iso2: 'ID', power: 68, points: 1124.17, previous_rank: 130 },
];

export const desktopApi = {
  async getDashboard(): Promise<DashboardSnapshotDto> {
    return (await tauriInvoke<DashboardSnapshotDto>('get_dashboard')) ?? PREVIEW_SNAPSHOT;
  },
  async getSquad(): Promise<PlayerDto[]> {
    return (await tauriInvoke<PlayerDto[]>('get_squad')) ?? PREVIEW_SQUAD;
  },
  async getFifaRankings(): Promise<CountryRankDto[]> {
    return (await tauriInvoke<CountryRankDto[]>('get_fifa_rankings')) ?? PREVIEW_FIFA;
  },
  async simulateNextMatchday(): Promise<void> {
    await tauriInvoke('simulate_next_matchday');
  },
  async trainFocus(focus: string): Promise<void> {
    await tauriInvoke('train_focus', { focus });
  },
  async saveCareerSlot(): Promise<void> {
    await tauriInvoke('save_career_slot');
  },
};

export function formatMoney(amount: number): string {
  const abs = Math.abs(amount);
  const sign = amount < 0 ? '-' : '';
  if (abs >= 1_000_000_000) return `${sign}$${(abs / 1_000_000_000).toFixed(2)}B`;
  if (abs >= 1_000_000) return `${sign}$${(abs / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${sign}$${Math.round(abs / 1_000)}K`;
  return `${sign}$${abs}`;
}

export function positionMeta(posId: number): { label: string; cls: string } {
  if (posId === 1) return { label: 'GK', cls: 'pos-gk' };
  if (posId <= 4) return { label: 'DEF', cls: 'pos-def' };
  if (posId <= 8) return { label: 'MID', cls: 'pos-mid' };
  return { label: 'ATT', cls: 'pos-att' };
}

