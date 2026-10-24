import { i18n } from '../../i18n/index.ts';
import { formatTime } from '../../lib/format.ts';
import { displayName, importer, type EntryCandidate, type ImportEntry } from '../../lib/import.ts';
import type { LibraryTrack } from '../../lib/library.ts';
import type { MenuEntry } from '../../lib/menu.ts';
import { report } from '../library/actions.ts';

/**
 * A row for an imported entry still without a track: the list's own names in its place.
 * Its id is negative so it never matches a library track.
 */
export function placeholderTrack(entry: ImportEntry): LibraryTrack {
  const { source } = entry;
  return {
    id: -entry.id,
    path: '',
    title: source.title || source.url || '…',
    artist: source.artists.join(', ') || null,
    artistId: null,
    artists: [],
    album: source.album,
    albumId: null,
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
    discNumber: null,
    duration: source.duration,
    format: null,
    sampleRate: null,
    bitDepth: null,
    bitrate: null,
    favorite: false,
    playCount: 0,
    lastPlayedAt: null,
    addedAt: 0,
    cover: null,
    folderId: null,
    sourceUrl: null,
  };
}

/** "Artist — Title · 3:45 (in the library)" of a candidate. */
export function candidateLabel(candidate: EntryCandidate): string {
  const parts = [displayName(candidate)];
  if (candidate.duration !== null) {
    parts.push(formatTime(candidate.duration));
  }
  const label = parts.join(' · ');
  return candidate.trackId !== null ? `${label} (${i18n.t('import.inLibrary')})` : label;
}

/**
 * The "?" menu of a doubtful (or picked) entry: what the list names, the alternatives with
 * the chosen one checked, and Find….
 */
export function alternativesMenu(entry: ImportEntry, find: () => void): MenuEntry[] {
  const chosen = entry.trackId !== null ? entry.candidates[0] : undefined;
  return [
    {
      label: i18n.t('import.wanted', { name: displayName(entry.source) }),
      enabled: false,
      action: () => {},
    },
    'separator',
    ...entry.candidates.map((candidate) => ({
      label: candidateLabel(candidate),
      checked: candidate === chosen,
      action: () => {
        if (candidate !== chosen) {
          importer.pick(entry.id, candidate).catch(report('Choosing a track'));
        }
      },
    })),
    ...(entry.candidates.length > 0 ? ['separator' as const] : []),
    { label: i18n.t('import.findAnother'), action: find },
  ];
}
