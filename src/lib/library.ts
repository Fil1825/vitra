import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors crates/library/src/model.rs and src-tauri/src/library.

/** One of a track's artists. */
export interface ArtistRef {
  id: number;
  name: string;
}

export interface LibraryTrack {
  id: number;
  path: string;
  title: string;
  /** The artists' names joined for display ("A, B"). */
  artist: string | null;
  /** The main (first) artist. */
  artistId: number | null;
  /** Every artist in credit order, the main one first. */
  artists: ArtistRef[];
  album: string | null;
  albumId: number | null;
  albumArtist: string | null;
  genre: string | null;
  year: number | null;
  trackNumber: number | null;
  discNumber: number | null;
  /** Seconds. */
  duration: number | null;
  format: string | null;
  sampleRate: number | null;
  bitDepth: number | null;
  bitrate: number | null;
  favorite: boolean;
  playCount: number;
  lastPlayedAt: number | null;
  /** Unix seconds. */
  addedAt: number;
  /** Cover id: the track's own (a link's thumbnail), else its album's. */
  cover: string | null;
  /** The library folder the file is in; `null` for a single file or a link. */
  folderId: number | null;
  /** The web page the track comes from: while it streams, `path` is this URL too. */
  sourceUrl: string | null;
}

export interface Album {
  id: number;
  title: string;
  artist: string | null;
  artistId: number | null;
  year: number | null;
  cover: string | null;
  trackCount: number;
  duration: number;
  addedAt: number;
}

export interface Artist {
  id: number;
  name: string;
  albumCount: number;
  trackCount: number;
  cover: string | null;
}

export interface Playlist {
  id: number;
  name: string;
  trackCount: number;
  duration: number;
  createdAt: number;
  updatedAt: number;
  /** The web playlist it was imported from. */
  sourceUrl: string | null;
}

export interface Folder {
  id: number;
  path: string;
  addedAt: number;
  trackCount: number;
}

export interface HistoryEntry {
  track: LibraryTrack;
  playedAt: number;
}

export interface SearchResults {
  tracks: LibraryTrack[];
  albums: Album[];
  artists: Artist[];
}

export type ScanPhase = 'listing' | 'reading' | 'covers';

export interface ScanStatus {
  scanning: boolean;
  folderId: number | null;
  phase: ScanPhase | null;
  done: number;
  total: number;
  queued: number;
}

/** What a `library://changed` event says changed. */
export type LibraryChange = 'tracks' | 'playlists' | 'folders';

export type TrackQuery =
  | { kind: 'all' }
  | { kind: 'album'; id: number }
  | { kind: 'artist'; id: number }
  /** Tracks the artist only features on. */
  | { kind: 'artistAppearances'; id: number }
  | { kind: 'favorites' }
  | { kind: 'recent'; limit: number }
  | { kind: 'playlist'; id: number }
  | { kind: 'search'; query: string };

export type AlbumOrder = 'artist' | 'recent';

export interface TagForm {
  title: string;
  artist: string;
  album: string;
  albumArtist: string;
  year: number | null;
  /** Artwork id of the current cover. */
  cover: string | null;
  writable: boolean;
}

export interface TagValues {
  title: string;
  artist: string;
  album: string;
  albumArtist: string;
  year: number | null;
}

export type CoverEdit = { kind: 'keep' } | { kind: 'remove' } | { kind: 'replace'; path: string };

export const LIBRARY_CHANGED_EVENT = 'library://changed';
export const LIBRARY_SCAN_EVENT = 'library://scan';

export const IDLE_SCAN: ScanStatus = {
  scanning: false,
  folderId: null,
  phase: null,
  done: 0,
  total: 0,
  queued: 0,
};

/** URL of a cached album cover (`artwork://localhost/cover/<id>`). */
export function coverUrl(id: string): string {
  // The id goes after the encoded base: convertFileSrc would encode a `/` inside it.
  return `${convertFileSrc('cover', 'artwork')}/${id}`;
}

const call = <T = void>(command: string, args?: Record<string, unknown>) =>
  isTauri()
    ? invoke<T>(command, args)
    : Promise.reject(new Error('The library needs the desktop app'));

export const library = {
  folders: () => call<Folder[]>('library_folders'),
  addFolder: (path: string) => call<Folder>('library_add_folder', { path }),
  removeFolder: (id: number) => call('library_remove_folder', { id }),
  rescan: (id?: number) => call('library_rescan', { id: id ?? null }),
  scanStatus: () => call<ScanStatus>('library_scan_status'),
  tracks: (query: TrackQuery) => call<LibraryTrack[]>('library_tracks', { query }),
  track: (id: number) => call<LibraryTrack | null>('library_track', { id }),
  albums: (order: AlbumOrder) => call<Album[]>('library_albums', { order }),
  album: (id: number) => call<Album | null>('library_album', { id }),
  artists: () => call<Artist[]>('library_artists'),
  artist: (id: number) => call<Artist | null>('library_artist', { id }),
  artistAlbums: (id: number) => call<Album[]>('library_artist_albums', { id }),
  search: (query: string, limit: number) => call<SearchResults>('library_search', { query, limit }),
  setFavorite: (trackId: number, favorite: boolean) =>
    call('library_set_favorite', { trackId, favorite }),
  /** Removes single tracks (not in a library folder); returns how many went. */
  removeTracks: (trackIds: number[]) => call<number>('library_remove_tracks', { trackIds }),
  history: (limit: number) => call<HistoryEntry[]>('library_history', { limit }),
};

export const playlists = {
  list: () => call<Playlist[]>('playlists_list'),
  get: (id: number) => call<Playlist | null>('playlist_get', { id }),
  create: (name: string, trackIds?: number[]) =>
    call<Playlist>('playlist_create', { name, trackIds: trackIds ?? null }),
  rename: (id: number, name: string) => call('playlist_rename', { id, name }),
  remove: (id: number) => call('playlist_delete', { id }),
  add: (id: number, trackIds: number[], index?: number) =>
    call('playlist_add', { id, trackIds, index: index ?? null }),
  removeItems: (id: number, indices: number[]) => call('playlist_remove', { id, indices }),
  move: (id: number, from: number, to: number) => call('playlist_move', { id, from, to }),
};

/** Whose artwork the full-screen viewer shows. */
export type ArtworkSource = { kind: 'track'; path: string } | { kind: 'album'; id: number };

export const artwork = {
  /** Artwork in its original size; resolves with its artwork id, or `null` if there is none. */
  original: (source: ArtworkSource) => call<string | null>('artwork_original', { source }),
};

export const tags = {
  load: (path: string) => call<TagForm>('tags_load', { path }),
  save: (path: string, values: TagValues, cover: CoverEdit) =>
    call('tags_save', { path, values, cover }),
  /** Prepares a picked image for preview; returns its artwork id. */
  preview: (path: string) => call<string>('artwork_preview', { path }),
};
