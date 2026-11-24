import id from './id.json';
import en from './en.json';

export type Locale = 'id' | 'en';
export type Dictionary = typeof id;

const DICTS: Record<Locale, Dictionary> = { id, en };

export function getDictionary(locale: Locale): Dictionary {
  return DICTS[locale];
}

