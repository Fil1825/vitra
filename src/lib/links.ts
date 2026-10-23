import { invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors src-tauri/src/links and crates/links.

/** A link track as the link dialog shows it. */
export interface PreviewTrack {
  url: string;
  title: string;
  artist: string | null;
  /** The artists when the site lists several. */
  artists: string[];
  album: string | null;
  /** The track's number on its album. */
  trackNumber: number | null;
  /** Seconds; `null` for live streams and unknown lengths. */
  duration: number | null;
  live: boolean;
  /** Artwork id (`artwork://`) of the thumbnail. */
  artwork: string | null;
  /** The library track, once the link is in the library. */
  trackId: number | null;
  /** The library has it as a downloaded file. */
  downloaded: boolean;
}

export type LinkPreview =
  | { kind: 'track'; track: PreviewTrack }
  | {
      kind: 'playlist';
      url: string;
      title: string;
      uploader: string | null;
      artwork: string | null;
      count: number;
      /** The first entries. */
      tracks: PreviewTrack[];
      /** The playlist this link was imported as before. */
      playlistId: number | null;
      /** Set when the playlist is an album. */
      album: AlbumPreview | null;
    };

/** An album link (YouTube / YouTube Music album playlists and pages). */
export interface AlbumPreview {
  title: string;
  artist: string | null;
  year: number | null;
  /** Tracks on the album, unavailable ones included. */
  count: number | null;
  /** The library album, once its tracks are in the library. */
  albumId: number | null;
}

/** How a link joins the queue. */
export type PlayMode = 'play' | 'next' | 'append';

export type DownloadStatus =
  | { state: 'queued' }
  /** `progress` is 0–1 when the size is known. */
  | { state: 'downloading'; progress: number | null }
  | { state: 'processing' }
  | { state: 'done'; path: string }
  | { state: 'failed'; error: string }
  | { state: 'cancelled' };

/** An album or playlist downloaded together. */
export interface DownloadGroup {
  id: number;
  kind: 'album' | 'playlist';
  url: string;
  title: string;
  artist: string | null;
  /** Library cover id. */
  cover: string | null;
}

export interface DownloadJob {
  id: number;
  trackId: number;
  url: string;
  title: string;
  artist: string | null;
  album: string | null;
  status: DownloadStatus;
  /** The album or playlist it is downloaded with. */
  group: DownloadGroup | null;
}

export type AudioFormat = 'original' | 'mp3' | 'flac';

export const MP3_BITRATES = [128, 192, 256, 320] as const;

export interface YtDlpInfo {
  version: string | null;
  path: string | null;
  /** The copy in the app data folder (or a bundled one) can update itself. */
  updatable: boolean;
  /** FFmpeg, which downloads need, was found. */
  ffmpeg: boolean;
}

export const DOWNLOADS_CHANGED_EVENT = 'downloads://changed';

/** Whether a library path or queue entry is a link rather than a file. */
export function isRemotePath(path: string): boolean {
  return /^https?:\/\//i.test(path);
}

/**
 * Whether pasted text looks like a link worth opening: one word that starts with a scheme
 * or `www.`, or a host with a dot followed by a path. The backend decides for real.
 */
export function looksLikeLink(text: string): boolean {
  const value = text.trim();
  if (value === '' || /\s/.test(value) || value.length > 4096) {
    return false;
  }
  return (
    /^https?:\/\/[^/]+\.[^/]/i.test(value) ||
    /^www\.[^/]+\.[^/]+/i.test(value) ||
    /^[a-z0-9-]+(\.[a-z0-9-]+)+\/\S/i.test(value)
  );
}

/** Whether a download is still to run or running. */
export function isActive(status: DownloadStatus): boolean {
  return (
    status.state === 'queued' || status.state === 'downloading' || status.state === 'processing'
  );
}

const call = <T = void>(command: string, args?: Record<string, unknown>) =>
  isTauri() ? invoke<T>(command, args) : Promise.reject(new Error('Links need the desktop app'));

export const links = {
  resolve: (input: string) => call<LinkPreview>('link_resolve', { input }),
  play: (url: string, mode: PlayMode) => call('link_play', { url, mode }),
  import: (url: string) => call<{ id: number; name: string }>('link_import', { url }),
  /** Downloads a link (each entry, for a playlist); resolves with how many were queued. */
  download: (url: string) => call<number>('link_download', { url }),
  /** Whether Vitra can read the browser's cookies (only Safari's can be checked). */
  cookiesAccess: (browser: string) =>
    call<'ok' | 'noAccess' | 'unknown'>('links_cookies_access', { browser }),
};

export const downloads = {
  list: () => call<DownloadJob[]>('downloads_list'),
  /** Downloads library tracks that are links; resolves with how many were queued. */
  add: (trackIds: number[]) => call<number>('downloads_add', { trackIds }),
  /** Downloads a library album's linked tracks as that album (folders, tags, one cover). */
  addAlbum: (albumId: number) => call<number>('downloads_add_album', { albumId }),
  cancel: (id: number) => call('downloads_cancel', { id }),
  retry: (id: number) => call('downloads_retry', { id }),
  /** Cancels every unfinished download of an album or playlist. */
  cancelGroup: (group: number) => call('downloads_cancel_group', { group }),
  /** Retries every failed or cancelled download of an album or playlist. */
  retryGroup: (group: number) => call('downloads_retry_group', { group }),
  clear: () => call('downloads_clear'),
  /** Where downloads go with the current settings. */
  folder: () => call<string>('downloads_folder'),
};

/**
 * The link being dragged (or just dropped) onto the window, read by the backend from the drag
 * pasteboard; `null` when it isn't a link or the system doesn't tell (only macOS does).
 */
export function dragLink(): Promise<string | null> {
  return isTauri() ? invoke<string | null>('drag_link') : Promise.resolve(null);
}

export const ytdlp = {
  info: () => call<YtDlpInfo>('ytdlp_info'),
  /** Updates yt-dlp; resolves with the new version. */
  update: () => call<string>('ytdlp_update'),
};
