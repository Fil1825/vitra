import { AnimatePresence, MotionConfig } from 'motion/react';
import { Link2, Music } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { ArtworkViewer } from '../components/ArtworkViewer/ArtworkViewer.tsx';
import { Backdrop } from '../components/Backdrop/Backdrop.tsx';
import { Glass } from '../components/Glass/index.ts';
import { setCover } from '../features/library/covers.ts';
import { LibraryView } from '../features/library/LibraryView.tsx';
import { LinkDialog } from '../features/links/LinkDialog.tsx';
import { LyricsEditor } from '../features/lyrics/LyricsEditor.tsx';
import { LyricsPanel } from '../features/lyrics/LyricsPanel.tsx';
import { QueuePanel } from '../features/queue/QueuePanel.tsx';
import { SettingsPanel } from '../features/settings/SettingsPanel.tsx';
import { TagEditor } from '../features/tags/TagEditor.tsx';
import type { CoverTarget } from '../lib/library.ts';
import { playPaths } from '../lib/openFiles.ts';
import { artworkUrl } from '../lib/player.ts';
import { useAppearance } from '../stores/appearance.ts';
import { useNavigation } from '../stores/navigation.ts';
import { usePlayer } from '../stores/player.ts';
import { useUi } from '../stores/ui.ts';
import styles from './App.module.css';
import { ControlBar } from './ControlBar.tsx';
import { Sidebar } from './Sidebar.tsx';
import { TitleBar } from './TitleBar.tsx';
import { useAppearanceController } from './useAppearanceController.ts';
import { useFileDrop } from './useFileDrop.ts';
import { useLevelMeter } from './useLevelMeter.ts';
import { useLinkInput } from './useLinkInput.ts';
import { useMiniPlayerShortcut } from './useMiniPlayerShortcut.ts';
import { usePlayerShortcuts } from './usePlayerShortcuts.ts';

export function App() {
  useAppearanceController();
  usePlayerShortcuts();
  useLinkInput();
  useLevelMeter();
  useMiniPlayerShortcut();
  const { t } = useTranslation();
  const motionEnabled = useAppearance((s) => s.effects.motion);
  const useArtwork = useAppearance((s) => s.theme.background.useArtwork);
  const artwork = usePlayer((s) => s.state.track?.artwork ?? null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const sidePanel = useUi((s) => s.sidePanel);
  const sidebar = useUi((s) => s.sidebar);
  const tagEditorPath = useUi((s) => s.tagEditorPath);
  const editTags = useUi((s) => s.editTags);
  const route = useNavigation((s) => s.route);
  const closeTagEditor = useCallback(() => editTags(null), [editTags]);
  const lyricsEditor = useUi((s) => s.lyricsEditor);
  const editLyrics = useUi((s) => s.editLyrics);
  const closeLyricsEditor = useCallback(() => editLyrics(null), [editLyrics]);
  const artworkView = useUi((s) => s.artworkView);
  const viewArtwork = useUi((s) => s.viewArtwork);
  const closeArtwork = useCallback(() => viewArtwork(null), [viewArtwork]);
  const linkInput = useUi((s) => s.linkInput);
  const openLink = useUi((s) => s.openLink);
  const closeLink = useCallback(() => openLink(null), [openLink]);
  const dropping = useFileDrop(
    useCallback((paths: string[]) => {
      playPaths(paths).catch((err: unknown) => console.error('Playing dropped files failed', err));
    }, []),
    useCallback((url: string) => useUi.getState().openLink(url), []),
    useCallback((target: CoverTarget, path: string) => setCover(target, path), []),
  );
  const shownArtwork = useArtwork ? artwork : null;

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      // Cmd+, / Ctrl+, is the conventional settings shortcut; rebinding comes in stage 7.
      if (event.key === ',' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        setSettingsOpen((open) => !open);
      } else if (event.key === 'Escape') {
        setSettingsOpen(false);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <MotionConfig reducedMotion={motionEnabled ? 'never' : 'always'}>
      <Backdrop
        colors={shownArtwork?.palette?.colors}
        artworkUrl={shownArtwork ? artworkUrl(shownArtwork.id, 'thumb') : undefined}
      />
      <div className={styles.shell} data-panel={sidePanel ?? undefined} data-sidebar={sidebar}>
        {sidebar !== 'hidden' && <Sidebar className={styles.sidebar} mode={sidebar} />}
        <TitleBar className={styles.header} onOpenSettings={() => setSettingsOpen(true)} />
        {/* Keyed by route so each view starts at the top. */}
        <main key={JSON.stringify(route)} className={styles.main} data-scroll-container>
          <LibraryView />
        </main>
        {sidePanel === 'queue' && <QueuePanel className={styles.panel} />}
        {sidePanel === 'lyrics' && <LyricsPanel className={styles.panel} />}
        <ControlBar className={styles.controls} />
      </div>
      <AnimatePresence>
        {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      </AnimatePresence>
      {tagEditorPath && (
        <TagEditor key={tagEditorPath} path={tagEditorPath} onClose={closeTagEditor} />
      )}
      {lyricsEditor && (
        <LyricsEditor key={lyricsEditor.path} target={lyricsEditor} onClose={closeLyricsEditor} />
      )}
      {linkInput !== null && <LinkDialog key={linkInput} initial={linkInput} onClose={closeLink} />}
      {artworkView && <ArtworkViewer view={artworkView} onClose={closeArtwork} />}
      {/* A cover being dropped on shows it itself. */}
      {(dropping === 'files' || dropping === 'link') && (
        <div className={styles.dropZone}>
          <Glass className={styles.dropCard} priority={6}>
            {dropping === 'link' ? (
              <Link2 size={32} aria-hidden="true" />
            ) : (
              <Music size={32} aria-hidden="true" />
            )}
            {t(dropping === 'link' ? 'player.dropToOpenLink' : 'player.dropToPlay')}
          </Glass>
        </div>
      )}
    </MotionConfig>
  );
}
