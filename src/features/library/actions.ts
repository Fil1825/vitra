import { message } from '@tauri-apps/plugin-dialog';

import { i18n } from '../../i18n/index.ts';
import { confirm } from '../../lib/confirm.ts';
import {
  library,
  playlists,
  type Album,
  type LibraryTrack,
  type Playlist,
} from '../../lib/library.ts';
import { downloads, isRemotePath } from '../../lib/links.ts';
import { coverMenu } from './covers.ts';
import type { MenuEntry } from '../../lib/menu.ts';
import { pickFolder } from '../../lib/openFiles.ts';
import { queue } from '../../lib/queue.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { useUi } from '../../stores/ui.ts';

export const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

/** Replaces the queue with the tracks and plays from `start`. */
export function playTracks(ids: number[], start = 0): void {
  if (ids.length > 0) {
    queue.play(ids, start).catch(report('Playing'));
  }
}

/** Plays the tracks shuffled, starting with a random one. */
export function shuffleTracks(ids: number[]): void {
  if (ids.length === 0) {
    return;
  }
  queue
    .setShuffle(true)
    .then(() => queue.play(ids, Math.floor(Math.random() * ids.length)))
    .catch(report('Shuffling'));
}

/** The library folder that already contains `path` (not `path` itself), if any. */
export function containingFolder(path: string, folders: readonly string[]): string | undefined {
  const normalized = path.replace(/[\\/]+$/, '');
  return folders.find((folder) => {
    const base = folder.replace(/[\\/]+$/, '');
    return (
      normalized !== base &&
      normalized.startsWith(base) &&
      ['/', '\\'].includes(normalized.charAt(base.length))
    );
  });
}

/** Asks for a folder and adds it to the library (its scan starts right away). */
export async function addMusicFolder(): Promise<void> {
  const path = await pickFolder();
  if (!path) {
    return;
  }
  try {
    const folders = (await library.folders()).map((folder) => folder.path);
    const parent = containingFolder(path, folders);
    if (parent) {
      await message(i18n.t('settings.library.insideFolder', { folder: parent }), {
        kind: 'info',
      });
      return;
    }
    await library.addFolder(path);
  } catch (err) {
    await message(String(err), { kind: 'warning' });
  }
}

export function setFavorite(ids: number[], favorite: boolean): void {
  Promise.all(ids.map((id) => library.setFavorite(id, favorite))).catch(report('Favoriting'));
}

/** Asks, then removes single tracks from the library (their files stay on disk). */
async function removeFromLibrary(tracks: LibraryTrack[]): Promise<void> {
  const [first] = tracks;
  // Links have no file to stay on disk.
  const links = tracks.every((track) => isRemotePath(track.path));
  const question =
    tracks.length === 1 && first
      ? i18n.t(links ? 'library.removeLinkConfirm' : 'library.removeOneConfirm', {
          title: first.title,
        })
      : i18n.t(links ? 'library.removeLinksConfirm' : 'library.removeManyConfirm', {
          count: tracks.length,
        });
  if (await confirm(question, 'remove')) {
    await library.removeTracks(tracks.map((track) => track.id));
  }
}

/** Queues link tracks for download (the others are skipped by the backend). */
export function downloadTracks(tracks: LibraryTrack[]): void {
  const ids = tracks.filter((track) => isRemotePath(track.path)).map((track) => track.id);
  if (ids.length > 0) {
    downloads.add(ids).catch(report('Downloading'));
  }
}

/** Downloads an album's linked tracks as that album. */
export function downloadAlbum(id: number): void {
  downloads.addAlbum(id).catch(report('Downloading an album'));
}

export function copyText(text: string): void {
  navigator.clipboard.writeText(text).catch(report('Copying'));
}

/** Creates a playlist with the tracks, opens it and starts renaming it. */
export function createPlaylist(ids: number[] = []): void {
  playlists
    .create(i18n.t('library.newPlaylist'), ids)
    .then((playlist) => {
      useNavigation.getState().go({ view: 'playlist', id: playlist.id });
      useUi.getState().renamePlaylist(playlist.id);
    })
    .catch(report('Creating a playlist'));
}

function addToPlaylistMenu(ids: () => Promise<number[]>, all: Playlist[]): MenuEntry {
  const add = (id: number) => () => {
    ids()
      .then((trackIds) => playlists.add(id, trackIds))
      .catch(report('Adding to a playlist'));
  };
  return {
    label: i18n.t('library.addToPlaylist'),
    items: [
      {
        label: i18n.t('library.newPlaylist'),
        action: () => {
          ids().then(createPlaylist).catch(report('Creating a playlist'));
        },
      },
      ...(all.length > 0 ? ['separator' as const] : []),
      ...all.map((playlist) => ({ label: playlist.name, action: add(playlist.id) })),
    ],
  };
}

/** Context menu for one or more tracks. */
export function trackMenu(
  tracks: LibraryTrack[],
  allPlaylists: Playlist[],
  extra: MenuEntry[] = [],
): MenuEntry[] {
  const ids = tracks.map((track) => track.id);
  const single = tracks.length === 1 ? tracks[0] : undefined;
  const allFavorite = tracks.length > 0 && tracks.every((track) => track.favorite);
  const go = useNavigation.getState().go;
  const entries: MenuEntry[] = [
    {
      label: i18n.t('library.playNext'),
      action: () => queue.add(ids, true).catch(report('Queueing')),
    },
    {
      label: i18n.t('library.addToQueue'),
      action: () => queue.add(ids, false).catch(report('Queueing')),
    },
    'separator',
    addToPlaylistMenu(() => Promise.resolve(ids), allPlaylists),
    {
      label: i18n.t(allFavorite ? 'player.unfavorite' : 'player.favorite'),
      action: () => setFavorite(ids, !allFavorite),
    },
  ];
  const streamed = tracks.filter((track) => isRemotePath(track.path));
  if (streamed.length > 0) {
    entries.push('separator', {
      label: i18n.t('links.download'),
      action: () => downloadTracks(streamed),
    });
  }
  if (single) {
    entries.push('separator');
    // A link has no file to tag; its download gets tags.
    if (!isRemotePath(single.path)) {
      entries.push({
        label: i18n.t('library.editTags'),
        action: () => useUi.getState().editTags(single.path),
      });
    }
    entries.push({
      label: i18n.t('library.editLyrics'),
      action: () =>
        useUi.getState().editLyrics({
          path: single.path,
          libraryId: single.id,
          title: single.title,
          artist: single.artist,
        }),
    });
    const { sourceUrl } = single;
    if (sourceUrl !== null) {
      entries.push({ label: i18n.t('links.copyLink'), action: () => copyText(sourceUrl) });
    }
    const { albumId, artistId } = single;
    if (albumId !== null) {
      entries.push({
        label: i18n.t('library.goToAlbum'),
        action: () => go({ view: 'album', id: albumId }),
      });
    }
    const { artists } = single;
    if (artists.length > 1) {
      entries.push({
        label: i18n.t('library.goToArtist'),
        items: artists.map((artist) => ({
          label: artist.name,
          action: () => go({ view: 'artist', id: artist.id }),
        })),
      });
    } else if (artistId !== null) {
      entries.push({
        label: i18n.t('library.goToArtist'),
        action: () => go({ view: 'artist', id: artistId }),
      });
    }
  }
  const tail = [...extra];
  // Tracks in library folders would come back with the next scan; only single files can go.
  if (tracks.length > 0 && tracks.every((track) => track.folderId === null)) {
    tail.push({
      label: i18n.t('library.removeFromLibrary'),
      action: () => void removeFromLibrary(tracks).catch(report('Removing from the library')),
    });
  }
  if (tail.length > 0) {
    entries.push('separator', ...tail);
  }
  return entries;
}

const albumTrackIds = (id: number) =>
  library.tracks({ kind: 'album', id }).then((tracks) => tracks.map((track) => track.id));

/** Context menu for an album tile. */
export function albumMenu(album: Album, allPlaylists: Playlist[]): MenuEntry[] {
  const withTracks = (what: string, run: (ids: number[]) => Promise<void> | void) => () => {
    albumTrackIds(album.id).then(run).catch(report(what));
  };
  const entries: MenuEntry[] = [
    { label: i18n.t('library.play'), action: withTracks('Playing', (ids) => playTracks(ids)) },
    {
      label: i18n.t('library.shuffle'),
      action: withTracks('Shuffling', (ids) => shuffleTracks(ids)),
    },
    'separator',
    {
      label: i18n.t('library.playNext'),
      action: withTracks('Queueing', (ids) => queue.add(ids, true)),
    },
    {
      label: i18n.t('library.addToQueue'),
      action: withTracks('Queueing', (ids) => queue.add(ids, false)),
    },
    'separator',
    addToPlaylistMenu(() => albumTrackIds(album.id), allPlaylists),
    'separator',
    ...coverMenu({ kind: 'album', id: album.id }, album.customCover),
  ];
  if (album.linkCount > 0) {
    entries.push('separator', {
      label: i18n.t('links.downloadAlbum'),
      action: () => downloadAlbum(album.id),
    });
  }
  const { artistId } = album;
  if (artistId !== null) {
    entries.push('separator', {
      label: i18n.t('library.goToArtist'),
      action: () => useNavigation.getState().go({ view: 'artist', id: artistId }),
    });
  }
  return entries;
}
