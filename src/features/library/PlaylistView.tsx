import { ListMusic, MoreHorizontal } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { confirm } from '../../lib/confirm.ts';
import { library, playlists } from '../../lib/library.ts';
import { isRemotePath, links } from '../../lib/links.ts';
import { showContextMenu, type MenuEntry } from '../../lib/menu.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { useUi } from '../../stores/ui.ts';
import { copyText, downloadTracks, report } from './actions.ts';
import { EmptyState, LoadError, PlayButtons } from './common.tsx';
import { coverMenu } from './covers.ts';
import { HeroCover } from './HeroCover.tsx';
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

  const remove = async () => {
    if (!data) {
      return;
    }
    const confirmed = await confirm(
      t('library.deletePlaylistConfirm', { name: data.name }),
      'delete',
    );
    if (confirmed) {
      await playlists.remove(id);
      useNavigation.getState().goBack();
    }
  };

  return (
    <div className={styles.view}>
      {playlist.error !== undefined && <LoadError error={playlist.error} />}
      <header className={styles.hero}>
        <HeroCover
          target={{ kind: 'playlist', id }}
          cover={data?.cover ?? null}
          custom={data?.customCover ?? false}
        />
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
              onClick={() => {
                const sourceUrl = data?.sourceUrl ?? null;
                const streamed = list.filter((track) => isRemotePath(track.path));
                const entries: MenuEntry[] = [
                  { label: t('library.renamePlaylist'), action: () => renamePlaylist(id) },
                  ...coverMenu({ kind: 'playlist', id }, data?.customCover ?? false),
                  { label: t('library.deletePlaylist'), action: () => void remove() },
                ];
                if (streamed.length > 0 || sourceUrl !== null) {
                  entries.push('separator');
                }
                if (streamed.length > 0) {
                  entries.push({
                    label: t('links.downloadAll'),
                    action: () => downloadTracks(streamed),
                  });
                }
                if (sourceUrl !== null) {
                  entries.push(
                    {
                      label: t('links.refreshPlaylist'),
                      action: () => void links.import(sourceUrl).catch(report('Refreshing')),
                    },
                    { label: t('links.copyLink'), action: () => copyText(sourceUrl) },
                  );
                }
                showContextMenu(entries).catch(report('Showing a menu'));
              }}
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
