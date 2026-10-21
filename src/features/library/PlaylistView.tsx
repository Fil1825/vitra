import { ask } from '@tauri-apps/plugin-dialog';
import { ListMusic, MoreHorizontal } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { library, playlists } from '../../lib/library.ts';
import { showContextMenu } from '../../lib/menu.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { useUi } from '../../stores/ui.ts';
import { report } from './actions.ts';
import { EmptyState, LoadError, PlayButtons } from './common.tsx';
import { Cover } from './Cover.tsx';
import styles from './Library.module.css';
import { useListSummary } from './summary.ts';
import { TrackTable } from './TrackTable.tsx';

const CHANGES = ['playlists', 'tracks'] as const;

export function PlaylistView({ id }: { id: number }) {
  const { t } = useTranslation();
  const summary = useListSummary();
  const renaming = useUi((s) => s.renamingPlaylist === id);
  const renamePlaylist = useUi((s) => s.renamePlaylist);
  const playlist = useLibraryData(`playlist:${id}`, () => playlists.get(id), CHANGES);
  const tracks = useLibraryData(
    `playlist-tracks:${id}`,
    () => library.tracks({ kind: 'playlist', id }),
    CHANGES,
  );

  if (playlist.data === null) {
    return <EmptyState title={t('library.nothingHere')} />;
  }
  const data = playlist.data;
  const list = tracks.data ?? [];
  const cover = list.find((track) => track.cover !== null)?.cover ?? null;

  const remove = async () => {
    if (!data) {
      return;
    }
    const confirmed = await ask(t('library.deletePlaylistConfirm', { name: data.name }), {
      kind: 'warning',
    });
    if (confirmed) {
      await playlists.remove(id);
      useNavigation.getState().goBack();
    }
  };

  return (
    <div className={styles.view}>
      {playlist.error !== undefined && <LoadError error={playlist.error} />}
      <header className={styles.hero}>
        <Cover id={cover} className={styles.heroCover} />
        <div className={styles.heroText}>
          <span className={styles.kind}>{t('library.playlist')}</span>
          {renaming && data ? (
            <NameInput
              initial={data.name}
              onDone={(name) => {
                renamePlaylist(null);
                if (name && name !== data.name) {
                  playlists.rename(id, name).catch(report('Renaming a playlist'));
                }
              }}
            />
          ) : (
            <h2 className={styles.heroTitle} onDoubleClick={() => renamePlaylist(id)}>
              {data?.name}
            </h2>
          )}
          {data && <p className={styles.heroMeta}>{summary(data.trackCount, data.duration)}</p>}
          <div className={styles.actions}>
            <PlayButtons ids={list.map((track) => track.id)} />
            <IconButton
              label={t('library.playlist')}
              onClick={() =>
                showContextMenu([
                  { label: t('library.renamePlaylist'), action: () => renamePlaylist(id) },
                  { label: t('library.deletePlaylist'), action: () => void remove() },
                ]).catch(report('Showing a menu'))
              }
            >
              <MoreHorizontal size={18} />
            </IconButton>
          </div>
        </div>
      </header>
      {tracks.data?.length === 0 ? (
        <EmptyState
          icon={<ListMusic size={36} aria-hidden="true" />}
          title={t('library.emptyPlaylist')}
          body={t('library.emptyPlaylistBody')}
        />
      ) : (
        <TrackTable
          tracks={list}
          columns={['number', 'title', 'artist', 'album', 'time', 'favorite']}
          numbering="index"
          covers
          onMove={(from, to) => playlists.move(id, from, to).catch(report('Reordering'))}
          extraMenu={(rows) => [
            {
              label: t('library.removeFromPlaylist'),
              action: () => playlists.removeItems(id, rows).catch(report('Removing')),
            },
          ]}
        />
      )}
    </div>
  );
}

function NameInput({ initial, onDone }: { initial: string; onDone(name: string): void }) {
  const { t } = useTranslation();
  const [name, setName] = useState(initial);
  return (
    <input
      className={styles.heroTitleInput}
      aria-label={t('nav.playlistName')}
      value={name}
      // The user just asked to rename.
      autoFocus
      onFocus={(event) => event.currentTarget.select()}
      onChange={(event) => setName(event.target.value)}
      onBlur={() => onDone(name.trim())}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          onDone(name.trim());
        } else if (event.key === 'Escape') {
          onDone(initial);
        }
      }}
    />
  );
}
