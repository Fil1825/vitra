import {
  AudioLines,
  Clock3,
  Disc3,
  Download,
  Heart,
  ListMusic,
  Music2,
  Plus,
  Search,
  Users,
  X,
  type LucideIcon,
} from 'lucide-react';
import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../components/Glass/index.ts';
import { IconButton } from '../components/ui/IconButton.tsx';
import { createPlaylist, report } from '../features/library/actions.ts';
import { usePlaylists } from '../features/library/usePlaylists.ts';
import { confirm } from '../lib/confirm.ts';
import { playlists } from '../lib/library.ts';
import { isActive } from '../lib/links.ts';
import { showContextMenu } from '../lib/menu.ts';
import { useDownloads } from '../stores/downloads.ts';
import { useNavigation, type Route } from '../stores/navigation.ts';
import { useUi } from '../stores/ui.ts';
import styles from './Sidebar.module.css';

/** Typing pauses this long before the search runs. */
const SEARCH_DELAY_MS = 150;

function SearchField() {
  const { t } = useTranslation();
  const route = useNavigation((s) => s.route);
  const [text, setText] = useState('');
  const input = useRef<HTMLInputElement>(null);
  const timer = useRef<number | undefined>(undefined);

  // Leaving the results view (e.g. via the sidebar) clears the field.
  const [lastView, setLastView] = useState(route.view);
  if (route.view !== lastView) {
    setLastView(route.view);
    if (route.view !== 'search') {
      setText('');
    }
  }

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'f') {
        event.preventDefault();
        input.current?.focus();
        input.current?.select();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.clearTimeout(timer.current);
    };
  }, []);

  const search = (query: string) => {
    setText(query);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      const { route: current, go, replace } = useNavigation.getState();
      if (current.view === 'search') {
        replace({ view: 'search', query });
      } else if (query.trim()) {
        go({ view: 'search', query });
      }
    }, SEARCH_DELAY_MS);
  };

  return (
    <div className={styles.search}>
      <Search size={14} className={styles.searchIcon} aria-hidden="true" />
      <input
        ref={input}
        type="search"
        className={styles.searchInput}
        placeholder={t('nav.search')}
        aria-label={t('nav.search')}
        value={text}
        onChange={(event) => search(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            search('');
            input.current?.blur();
          }
        }}
      />
      {text && (
        <button
          type="button"
          className={styles.clearSearch}
          aria-label={t('nav.clearSearch')}
          onClick={() => search('')}
        >
          <X size={12} />
        </button>
      )}
    </div>
  );
}
function NavItem({
  route,
  label,
  icon: Icon,
  badge,
  onContextMenu,
}: {
  route: Route;
  label: string;
  icon: LucideIcon;
  /** A count shown at the end (e.g. running downloads). */
  badge?: number;
  onContextMenu?(event: MouseEvent): void;
}) {
  const current = useNavigation((s) => s.route);
  const go = useNavigation((s) => s.go);
  const active =
    current.view === route.view &&
    (!('id' in route) || ('id' in current && current.id === route.id));
  return (
    <li>
      <button
        type="button"
        className={styles.item}
        aria-current={active ? 'page' : undefined}
        onClick={() => go(route)}
        onContextMenu={onContextMenu}
      >
        <Icon size={16} className={styles.icon} aria-hidden="true" />
        <span className={styles.label}>{label}</span>
        {badge !== undefined && badge > 0 && <span className={styles.badge}>{badge}</span>}
      </button>
    </li>
  );
}

function DownloadsItem({ visible, badge }: { visible: boolean; badge: number }) {
  const { t } = useTranslation();
  const open = useNavigation((s) => s.route.view === 'downloads');
  if (!visible && !open) {
    return null;
  }
  return (
    <NavItem
      route={{ view: 'downloads' }}
      label={t('nav.downloads')}
      icon={Download}
      badge={badge}
    />
  );
}

export function Sidebar({ className }: { className?: string }) {
  const { t } = useTranslation();
  const all = usePlaylists();
  const renamePlaylist = useUi((s) => s.renamePlaylist);
  const activeDownloads = useDownloads((s) => s.jobs.filter((job) => isActive(job.status)).length);
  const hasDownloads = useDownloads((s) => s.jobs.length > 0);
  const items: { route: Route; label: string; icon: LucideIcon }[] = [
    { route: { view: 'nowPlaying' }, label: t('nav.nowPlaying'), icon: AudioLines },
    { route: { view: 'recent' }, label: t('nav.recent'), icon: Clock3 },
    { route: { view: 'albums' }, label: t('nav.albums'), icon: Disc3 },
    { route: { view: 'artists' }, label: t('nav.artists'), icon: Users },
    { route: { view: 'songs' }, label: t('nav.songs'), icon: Music2 },
    { route: { view: 'favorites' }, label: t('nav.favorites'), icon: Heart },
  ];

  const playlistMenu = (id: number, name: string) => (event: MouseEvent) => {
    event.preventDefault();
    showContextMenu([
      {
        label: t('library.renamePlaylist'),
        action: () => {
          useNavigation.getState().go({ view: 'playlist', id });
          renamePlaylist(id);
        },
      },
      {
        label: t('library.deletePlaylist'),
        action: () => {
          confirm(t('library.deletePlaylistConfirm', { name }), 'delete')
            .then(async (confirmed) => {
              if (confirmed) {
                await playlists.remove(id);
                const { route, replace } = useNavigation.getState();
                if (route.view === 'playlist' && route.id === id) {
                  replace({ view: 'albums' });
                }
              }
            })
            .catch(report('Deleting a playlist'));
        },
      },
    ]).catch(report('Showing a menu'));
  };

  return (
    <Glass
      as="nav"
      className={`${styles.sidebar} ${className ?? ''}`}
      aria-label={t('nav.label')}
      priority={2}
    >
      {/* Room for the macOS traffic lights; also lets the user drag the window from here. */}
      <div className={styles.dragArea} data-tauri-drag-region />
      <SearchField />
      <div className={styles.scroll}>
        <p className={styles.section}>{t('nav.library')}</p>
        <ul className={styles.list}>
          {items.map((item) => (
            <NavItem key={item.route.view} {...item} />
          ))}
          {/* Shown once there is something to show, or while it is open. */}
          <DownloadsItem visible={hasDownloads} badge={activeDownloads} />
        </ul>
        <div className={styles.sectionRow}>
          <p className={styles.section}>{t('nav.playlists')}</p>
          <IconButton
            label={t('nav.newPlaylist')}
            className={styles.add}
            onClick={() => createPlaylist()}
          >
            <Plus size={14} />
          </IconButton>
        </div>
        <ul className={styles.list}>
          {all.map((playlist) => (
            <NavItem
              key={playlist.id}
              route={{ view: 'playlist', id: playlist.id }}
              label={playlist.name}
              icon={ListMusic}
              onContextMenu={playlistMenu(playlist.id, playlist.name)}
            />
          ))}
        </ul>
      </div>
    </Glass>
  );
}
