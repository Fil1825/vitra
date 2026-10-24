import { AnimatePresence, MotionConfig } from 'motion/react';
import { Link2, ListMusic, Music, Palette } from 'lucide-react';
import { useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { ArtworkViewer } from '../components/ArtworkViewer/ArtworkViewer.tsx';
import { Backdrop } from '../components/Backdrop/Backdrop.tsx';
import { Glass } from '../components/Glass/index.ts';
import { ImportDialog } from '../features/import/ImportDialog.tsx';
import { report } from '../features/library/actions.ts';
import { setCover } from '../features/library/covers.ts';
import { LinkDialog } from '../features/links/LinkDialog.tsx';
import { LyricsEditor } from '../features/lyrics/LyricsEditor.tsx';
import { importThemeFile } from '../features/themes/actions.ts';
import { ThemeEditor } from '../features/themes/ThemeEditor.tsx';
import { SettingsPanel } from '../features/settings/SettingsPanel.tsx';
import { TagEditor } from '../features/tags/TagEditor.tsx';
import { Workspace } from '../layout/Workspace.tsx';
import { importer, isPlaylistFile, isServiceLink } from '../lib/import.ts';
import type { CoverTarget } from '../lib/library.ts';
import { playPaths } from '../lib/openFiles.ts';
import { artworkUrl } from '../lib/player.ts';
import { isThemeFile, themeAssetUrl } from '../lib/themes.ts';
import { useShortcuts } from '../shortcuts/useShortcuts.ts';
import { useAppearance } from '../stores/appearance.ts';
import { useNavigation } from '../stores/navigation.ts';
import { usePlayer } from '../stores/player.ts';
import { useThemeEditor } from '../stores/themeEditor.ts';
import { useUi } from '../stores/ui.ts';
import styles from './App.module.css';
import { useAppearanceController } from './useAppearanceController.ts';
import { useTrayLabels } from './useTrayLabels.ts';
import { useFileDrop } from './useFileDrop.ts';
import { useLevelMeter } from './useLevelMeter.ts';
import { useLinkInput } from './useLinkInput.ts';

export function App() {
  useAppearanceController();
  useShortcuts('main');
  useTrayLabels();
  useLinkInput();
  useLevelMeter();
  const { t } = useTranslation();
  const motionEnabled = useAppearance((s) => s.effects.motion);
  const useArtwork = useAppearance((s) => s.theme.background.useArtwork);
  const themeId = useAppearance((s) => s.theme.id);
  const themePicture = useAppearance((s) => s.theme.background.image);
  const artwork = usePlayer((s) => s.state.track?.artwork ?? null);
  const settingsOpen = useUi((s) => s.settingsOpen);
  const showSettings = useUi((s) => s.showSettings);
  const themeDraft = useThemeEditor((s) => s.draft);
  const tagEditorPath = useUi((s) => s.tagEditorPath);
  const editTags = useUi((s) => s.editTags);
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
  const importInput = useUi((s) => s.importInput);
  const openImport = useUi((s) => s.openImport);
  const closeImport = useCallback(() => openImport(null), [openImport]);
  const dropping = useFileDrop(
    useCallback((paths: string[]) => {
      const themes = paths.filter(isThemeFile);
      if (themes.length > 0) {
        void (async () => {
          for (const path of themes) await importThemeFile(path);
        })().catch((err: unknown) => console.error('Importing dropped themes failed', err));
        return;
      }
      const lists = paths.filter(isPlaylistFile);
      if (lists.length > 0) {
        importer
          .files(lists)
          .then(([first]) => {
            if (first) useNavigation.getState().go({ view: 'playlist', id: first.playlist.id });
          })
          .catch(report('Importing dropped playlists'));
        return;
      }
      playPaths(paths).catch((err: unknown) => console.error('Playing dropped files failed', err));
    }, []),
    useCallback((url: string) => {
      const ui = useUi.getState();
      if (isServiceLink(url)) ui.openImport(url);
      else ui.openLink(url);
    }, []),
    useCallback((target: CoverTarget, path: string) => setCover(target, path), []),
  );
  const shownArtwork = useArtwork ? artwork : null;

  useEffect(() => {
    // Escape closes Settings; a component that used it (a menu, a field) has said so.
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !event.defaultPrevented) showSettings(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [showSettings]);

  return (
    <MotionConfig reducedMotion={motionEnabled ? 'never' : 'always'}>
      <Backdrop
        colors={shownArtwork?.palette?.colors}
        artworkUrl={shownArtwork ? artworkUrl(shownArtwork.id, 'thumb') : undefined}
        picture={
          themePicture && {
            url: themeAssetUrl(themeId, themePicture.src.slice('assets/'.length)),
            opacity: themePicture.opacity,
            fit: themePicture.fit,
          }
        }
      />
      <Workspace onOpenSettings={() => showSettings(true)} />
      {/* The theme editor takes Settings' place; Settings come back when it closes. */}
      <AnimatePresence>
        {settingsOpen && !themeDraft && <SettingsPanel onClose={() => showSettings(false)} />}
      </AnimatePresence>
      <AnimatePresence>{themeDraft && <ThemeEditor draft={themeDraft} />}</AnimatePresence>
      {tagEditorPath && (
        <TagEditor key={tagEditorPath} path={tagEditorPath} onClose={closeTagEditor} />
      )}
      {lyricsEditor && (
        <LyricsEditor key={lyricsEditor.path} target={lyricsEditor} onClose={closeLyricsEditor} />
      )}
      {linkInput !== null && <LinkDialog key={linkInput} initial={linkInput} onClose={closeLink} />}
      {importInput !== null && (
        <ImportDialog key={importInput} initial={importInput} onClose={closeImport} />
      )}
      {artworkView && <ArtworkViewer view={artworkView} onClose={closeArtwork} />}
      {/* A cover being dropped on shows it itself. */}
      {(dropping === 'files' ||
        dropping === 'link' ||
        dropping === 'theme' ||
        dropping === 'playlist') && (
        <div className={styles.dropZone}>
          <Glass className={styles.dropCard} priority={6}>
            {dropping === 'link' ? (
              <Link2 size={32} aria-hidden="true" />
            ) : dropping === 'theme' ? (
              <Palette size={32} aria-hidden="true" />
            ) : dropping === 'playlist' ? (
              <ListMusic size={32} aria-hidden="true" />
            ) : (
              <Music size={32} aria-hidden="true" />
            )}
            {dropping === 'link'
              ? t('player.dropToOpenLink')
              : dropping === 'theme'
                ? t('themes.dropToImport')
                : dropping === 'playlist'
                  ? t('import.dropToImport')
                  : t('player.dropToPlay')}
          </Glass>
        </div>
      )}
    </MotionConfig>
  );
}
