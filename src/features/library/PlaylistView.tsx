import { ListMusic, MoreHorizontal } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { confirm } from '../../lib/confirm.ts';
import { canRefresh, importer, type ImportEntry } from '../../lib/import.ts';
import { playlists } from '../../lib/library.ts';
import { isRemotePath } from '../../lib/links.ts';
import { showContextMenu, type MenuEntry } from '../../lib/menu.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { useUi } from '../../stores/ui.ts';
import { alternativesMenu, placeholderTrack } from '../import/entries.ts';
import { FindDialog } from '../import/FindDialog.tsx';
import { ImportStatus } from '../import/ImportStatus.tsx';
import { useImportProgress } from '../import/useImportProgress.ts';
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
  // Every place: tracks, and imported entries still being looked for.
  const places = useLibraryData(`playlist-places:${id}`, () => importer.places(id), CHANGES);
  const progress = useImportProgress(id);
  const [finding, setFinding] = useState<ImportEntry | null>(null);
  const { rows, entries } = useMemo(() => {
    const shown = (places.data ?? []).flatMap((place) => {
      const track = place.track ?? (place.entry ? placeholderTrack(place.entry) : null);
      return track ? [{ track, entry: place.entry }] : [];
    });
    return {
      rows: shown.map(({ track }) => track),
      entries: shown.some(({ entry }) => entry !== null)
        ? shown.map(({ entry }) => entry)
        : undefined,
    };
  }, [places.data]);

  if (playlist.data === null) {
    return <EmptyState title={t('library.nothingHere')} />;
  }
  const data = playlist.data;
  // Only rows with a track play.
  const list = rows.filter((_, index) => entries?.[index]?.trackId !== null);
  const onEntry = (entry: ImportEntry) => {
    if (entry.trackId === null) {
      setFinding(entry);
    } else {
      showContextMenu(alternativesMenu(entry, () => setFinding(entry))).catch(
        report('Showing a menu'),
      );
    }
  };

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
          {progress && <ImportStatus progress={progress} playlistId={id} />}
          <div className={styles.actions}>
            <PlayButtons ids={list.map((track) => track.id)} />
            <IconButton
              label={t('library.playlist')}
              onClick={() => {
                const sourceUrl = data?.sourceUrl ?? null;
                const streamed = list.filter((track) => isRemotePath(track.path));
                const menu: MenuEntry[] = [
                  { label: t('library.renamePlaylist'), action: () => renamePlaylist(id) },
                  ...coverMenu({ kind: 'playlist', id }, data?.customCover ?? false),
                  { label: t('library.deletePlaylist'), action: () => void remove() },
                ];
                const missing = (progress?.missing ?? 0) > 0;
                if (streamed.length > 0 || canRefresh(sourceUrl) || missing) {
                  menu.push('separator');
                }
                if (streamed.length > 0) {
                  menu.push({
                    label: t('links.downloadAll'),
                    action: () => downloadTracks(streamed),
                  });
                }
                if (missing) {
                  menu.push({
                    label: t('import.lookAgain'),
                    action: () => importer.retry(id).catch(report('Looking again')),
                  });
                }
                if (canRefresh(sourceUrl)) {
                  menu.push({
                    label: t('links.refreshPlaylist'),
                    action: () => void importer.link(sourceUrl).catch(report('Refreshing')),
                  });
                  if (/^https?:/i.test(sourceUrl)) {
                    menu.push({ label: t('links.copyLink'), action: () => copyText(sourceUrl) });
                  }
                }
                showContextMenu(menu).catch(report('Showing a menu'));
              }}
            >
              <MoreHorizontal size={18} />
            </IconButton>
          </div>
        </div>
      </header>
      {places.data?.length === 0 ? (
        <EmptyState
          icon={<ListMusic size={36} aria-hidden="true" />}
          title={t('library.emptyPlaylist')}
          body={t('library.emptyPlaylistBody')}
        />
      ) : (
        <TrackTable
          tracks={rows}
          columns={['number', 'title', 'artist', 'album', 'time', 'favorite']}
          numbering="index"
          covers
          entries={entries}
          onEntry={onEntry}
          onMove={(from, to) => playlists.move(id, from, to).catch(report('Reordering'))}
          extraMenu={(picked) => {
            const [single] = picked;
            const entry = picked.length === 1 && single !== undefined ? entries?.[single] : null;
            return [
              ...(entry
                ? [
                    {
                      label: t(entry.trackId === null ? 'import.find.menu' : 'import.findAnother'),
                      action: () => setFinding(entry),
                    },
                  ]
                : []),
              {
                label: t('library.removeFromPlaylist'),
                action: () => playlists.removeItems(id, picked).catch(report('Removing')),
              },
            ];
          }}
        />
      )}
      {finding && <FindDialog entry={finding} onClose={() => setFinding(null)} />}
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
