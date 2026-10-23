import { AnimatePresence, MotionConfig } from 'motion/react';
import { Link2, Music, Palette } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { ArtworkViewer } from '../components/ArtworkViewer/ArtworkViewer.tsx';
import { Backdrop } from '../components/Backdrop/Backdrop.tsx';
import { Glass } from '../components/Glass/index.ts';
import { setCover } from '../features/library/covers.ts';
import { LinkDialog } from '../features/links/LinkDialog.tsx';
import { LyricsEditor } from '../features/lyrics/LyricsEditor.tsx';
import { importThemeFile } from '../features/themes/actions.ts';
import { ThemeEditor } from '../features/themes/ThemeEditor.tsx';
import { SettingsPanel } from '../features/settings/SettingsPanel.tsx';
import { TagEditor } from '../features/tags/TagEditor.tsx';
import { Workspace } from '../layout/Workspace.tsx';
import type { CoverTarget } from '../lib/library.ts';
import { playPaths } from '../lib/openFiles.ts';
import { artworkUrl } from '../lib/player.ts';
import { isThemeFile, themeAssetUrl } from '../lib/themes.ts';
import { useAppearance } from '../stores/appearance.ts';
import { usePlayer } from '../stores/player.ts';
import { useThemeEditor } from '../stores/themeEditor.ts';
import { useUi } from '../stores/ui.ts';
import styles from './App.module.css';
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
  const themeId = useAppearance((s) => s.theme.id);
  const themePicture = useAppearance((s) => s.theme.background.image);
  const artwork = usePlayer((s) => s.state.track?.artwork ?? null);
  const [settingsOpen, setSettingsOpen] = useState(false);
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
  const dropping = useFileDrop(
    useCallback((paths: string[]) => {
      const themes = paths.filter(isThemeFile);
      if (themes.length > 0) {
        void (async () => {
          for (const path of themes) await importThemeFile(path);
        })().catch((err: unknown) => console.error('Importing dropped themes failed', err));
        return;
      }
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
        picture={
          themePicture && {
            url: themeAssetUrl(themeId, themePicture.src.slice('assets/'.length)),
            opacity: themePicture.opacity,
            fit: themePicture.fit,
          }
        }
      />
      <Workspace onOpenSettings={() => setSettingsOpen(true)} />
      {/* The theme editor takes Settings' place; Settings come back when it closes. */}
      <AnimatePresence>
        {settingsOpen && !themeDraft && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      </AnimatePresence>
      <AnimatePresence>{themeDraft && <ThemeEditor draft={themeDraft} />}</AnimatePresence>
      {tagEditorPath && (
        <TagEditor key={tagEditorPath} path={tagEditorPath} onClose={closeTagEditor} />
      )}
      {lyricsEditor && (
        <LyricsEditor key={lyricsEditor.path} target={lyricsEditor} onClose={closeLyricsEditor} />
      )}
      {linkInput !== null && <LinkDialog key={linkInput} initial={linkInput} onClose={closeLink} />}
      {artworkView && <ArtworkViewer view={artworkView} onClose={closeArtwork} />}
      {/* A cover being dropped on shows it itself. */}
      {(dropping === 'files' || dropping === 'link' || dropping === 'theme') && (
        <div className={styles.dropZone}>
          <Glass className={styles.dropCard} priority={6}>
            {dropping === 'link' ? (
              <Link2 size={32} aria-hidden="true" />
            ) : dropping === 'theme' ? (
              <Palette size={32} aria-hidden="true" />
            ) : (
              <Music size={32} aria-hidden="true" />
            )}
            {dropping === 'link'
              ? t('player.dropToOpenLink')
              : dropping === 'theme'
                ? t('themes.dropToImport')
                : t('player.dropToPlay')}
          </Glass>
        </div>
      )}
    </MotionConfig>
  );
}
