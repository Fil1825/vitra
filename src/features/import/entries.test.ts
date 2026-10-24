import { beforeAll, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-dialog', () => ({ message: vi.fn(), open: vi.fn() }));

import { i18n, initI18n } from '../../i18n/index.ts';
import type { EntryCandidate, ImportEntry } from '../../lib/import.ts';
import { alternativesMenu, candidateLabel, placeholderTrack } from './entries.ts';

const candidate = (overrides: Partial<EntryCandidate>): EntryCandidate => ({
  trackId: null,
  url: 'https://music.youtube.com/watch?v=a',
  title: 'One More Time',
  artists: ['Daft Punk'],
  album: null,
  duration: 320,
  score: 0.7,
  ...overrides,
});

const entry = (overrides: Partial<ImportEntry>): ImportEntry => ({
  id: 7,
  playlistId: 1,
  source: {
    title: 'One More Time',
    artists: ['Daft Punk'],
    album: 'Discovery',
    duration: 320,
    isrc: null,
    url: null,
    path: null,
    swappable: false,
  },
  state: 'doubtful',
  candidates: [],
  trackId: 42,
  ...overrides,
});

beforeAll(async () => {
  await initI18n('en');
});

describe('import entries', () => {
  it('stand in for their track with the list’s names', () => {
    const row = placeholderTrack(entry({ state: 'pending', trackId: null }));
    expect(row.id).toBe(-7);
    expect(row.title).toBe('One More Time');
    expect(row.artist).toBe('Daft Punk');
    expect(row.album).toBe('Discovery');
    expect(row.duration).toBe(320);
  });

  it('show a bare link until its page is read', () => {
    const bare = entry({ trackId: null });
    bare.source = {
      ...bare.source,
      title: '',
      artists: [],
      url: 'https://open.spotify.com/track/x',
    };
    expect(placeholderTrack(bare).title).toBe('https://open.spotify.com/track/x');
    expect(placeholderTrack(bare).artist).toBeNull();
  });

  it('label candidates with length and the library', () => {
    expect(candidateLabel(candidate({}))).toBe('Daft Punk — One More Time · 5:20');
    expect(candidateLabel(candidate({ trackId: 3, duration: null }))).toBe(
      `Daft Punk — One More Time (${i18n.t('import.inLibrary')})`,
    );
  });

  it('check the chosen alternative in the "?" menu', () => {
    const chosen = candidate({});
    const other = candidate({ url: 'https://music.youtube.com/watch?v=b', title: 'Live' });
    const menu = alternativesMenu(entry({ candidates: [chosen, other] }), () => {});
    const items = menu.filter((item) => item !== 'separator');
    expect(items[0]).toMatchObject({ enabled: false });
    expect(items[1]).toMatchObject({ checked: true });
    expect(items[2]).toMatchObject({ checked: false });
    expect(items.at(-1)).toMatchObject({ label: i18n.t('import.findAnother') });
  });
});
