import { EQ_BANDS, EQ_GAIN_MAX, type EqPreset, type Equalizer } from '../../lib/settings.ts';

/**
 * Built-in equalizer curves (dB at 32 Hz … 16 kHz), after the classic music player presets.
 * Their names are translated (`sound.presetNames.<id>`).
 */
export const BUILTIN_PRESETS = {
  flat: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  acoustic: [5, 4.5, 4, 1, 1.5, 1.5, 3.5, 4, 3.5, 2],
  bassBooster: [5.5, 4.5, 3.5, 2.5, 1, 0, 0, 0, 0, 0],
  bassReducer: [-5.5, -4.5, -3.5, -2.5, -1, 0, 0, 0, 0, 0],
  classical: [4.5, 3.5, 3, 2.5, -1.5, -1.5, 0, 2, 3, 3.5],
  dance: [3.5, 6.5, 5, 0, 2, 3.5, 5, 4.5, 3.5, 0],
  deep: [5, 3.5, 1.5, 1, 3, 2.5, 1.5, -2, -3.5, -4.5],
  electronic: [4.5, 4, 1.5, 0, -2, 2, 1, 1.5, 4, 5],
  hipHop: [5, 4.5, 1.5, 3, -1, -1, 1.5, -0.5, 2, 3],
  jazz: [4, 3, 1.5, 2, -1.5, -1.5, 0, 1.5, 3, 3.5],
  latin: [4.5, 3, 0, 0, -1.5, -1.5, -1.5, 0, 3, 4.5],
  loudness: [6, 4, 0, 0, -2, 0, -1, -5, 5, 1],
  lounge: [-3, -1.5, -0.5, 1.5, 4, 2.5, 0, -1.5, 2, 1],
  piano: [3, 2, 0, 2.5, 3, 1.5, 3.5, 4.5, 3, 3.5],
  pop: [-1.5, -1, 0, 2, 4, 4, 2, 0, -1, -1.5],
  rnb: [2.5, 7, 5.5, 1.5, -2.5, -1.5, 2, 2.5, 3, 3.5],
  rock: [5, 4, 3, 1.5, -0.5, -1, 0.5, 2.5, 3.5, 4.5],
  smallSpeakers: [5.5, 4, 3.5, 2.5, 1, 0, -1, -2, -3.5, -4],
  spokenWord: [-3.5, -0.5, 0, 0.5, 3.5, 4.5, 4.5, 4, 2, 0],
  trebleBooster: [0, 0, 0, 0, 0, 1, 2.5, 3.5, 4.5, 5.5],
  trebleReducer: [0, 0, 0, 0, 0, -1, -2.5, -3.5, -4.5, -5.5],
  vocalBooster: [-1.5, -3, -3, 1.5, 4, 4, 3, 1.5, 0, -1.5],
} as const satisfies Record<string, readonly number[]>;

export type BuiltinPreset = keyof typeof BUILTIN_PRESETS;

export const BUILTIN_IDS = Object.keys(BUILTIN_PRESETS) as BuiltinPreset[];

const USER_PREFIX = 'user:';

/** The preset id of a curve the user saved. */
export const userPresetId = (name: string) => `${USER_PREFIX}${name}`;

export function isBuiltin(id: string): id is BuiltinPreset {
  return Object.hasOwn(BUILTIN_PRESETS, id);
}

/** Room the boosts need so they don't clip: the highest gain, in half decibels. */
export function headroom(gains: readonly number[]): number {
  const highest = Math.max(0, ...gains);
  return highest === 0 ? 0 : -Math.ceil(highest * 2) / 2;
}

/** The curve a preset id names: a built-in one, or one of the user's; `null` if it's gone. */
export function presetCurve(
  id: string,
  saved: readonly EqPreset[],
): { gains: number[]; preamp: number } | null {
  if (isBuiltin(id)) {
    const gains = [...BUILTIN_PRESETS[id]];
    return { gains, preamp: headroom(gains) };
  }
  if (id.startsWith(USER_PREFIX)) {
    const preset = saved.find((p) => p.name === id.slice(USER_PREFIX.length));
    return preset ? { gains: [...preset.gains], preamp: preset.preamp } : null;
  }
  return null;
}

/** A gain as the equalizer keeps it: within range, in tenths of a decibel. */
export function clampGain(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.round(Math.min(EQ_GAIN_MAX, Math.max(-EQ_GAIN_MAX, value)) * 10) / 10;
}

/** The equalizer with one band (or the preamp, `band` -1) moved; the preset no longer applies. */
export function withGain(equalizer: Equalizer, band: number, value: number): Equalizer {
  const gain = clampGain(value);
  if (band < 0) return { ...equalizer, preamp: gain, preset: '' };
  const gains = EQ_BANDS.map((_, i) => (i === band ? gain : (equalizer.gains[i] ?? 0)));
  return { ...equalizer, gains, preset: '' };
}

/** "32", "1k", "16k": a band's centre as its label shows it. */
export function bandLabel(hz: number): string {
  return hz >= 1000 ? `${hz / 1000}k` : String(hz);
}

/** "+3.5", "0", "−2": a gain as its label shows it. */
export function gainLabel(db: number): string {
  if (db === 0) return '0';
  const text = Math.abs(db).toFixed(Number.isInteger(db) ? 0 : 1);
  return db > 0 ? `+${text}` : `−${text}`;
}
