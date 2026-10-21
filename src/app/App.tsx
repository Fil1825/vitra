import { AnimatePresence, MotionConfig } from 'motion/react';
import { Music } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Backdrop } from '../components/Backdrop/Backdrop.tsx';
import { Glass } from '../components/Glass/index.ts';
import { SettingsPanel } from '../features/settings/SettingsPanel.tsx';
import { playPaths } from '../lib/openFiles.ts';
import { artworkUrl } from '../lib/player.ts';
import { useAppearance } from '../stores/appearance.ts';
import { usePlayer } from '../stores/player.ts';
import styles from './App.module.css';
import { ControlBar } from './ControlBar.tsx';
import { NowPlaying } from './NowPlaying.tsx';
import { Sidebar } from './Sidebar.tsx';
import { TitleBar } from './TitleBar.tsx';
import { useAppearanceController } from './useAppearanceController.ts';
import { useFileDrop } from './useFileDrop.ts';
import { usePlayerShortcuts } from './usePlayerShortcuts.ts';

export function App() {
  useAppearanceController();
  usePlayerShortcuts();
  const { t } = useTranslation();
  const motionEnabled = useAppearance((s) => s.effects.motion);
  const useArtwork = useAppearance((s) => s.theme.background.useArtwork);
  const artwork = usePlayer((s) => s.state.track?.artwork ?? null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const dropping = useFileDrop(
    useCallback((paths: string[]) => {
      playPaths(paths).catch((err: unknown) => console.error('Playing dropped files failed', err));
    }, []),
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
      <div className={styles.shell}>
        <Sidebar className={styles.sidebar} />
        <TitleBar className={styles.header} onOpenSettings={() => setSettingsOpen(true)} />
        <main className={styles.main}>
          <NowPlaying />
        </main>
        <ControlBar className={styles.controls} />
      </div>
      <AnimatePresence>
        {settingsOpen && <SettingsPanel onClose={() => setSettingsOpen(false)} />}
      </AnimatePresence>
      {dropping && (
        <div className={styles.dropZone}>
          <Glass className={styles.dropCard} priority={6}>
            <Music size={32} aria-hidden="true" />
            {t('player.dropToPlay')}
          </Glass>
        </div>
      )}
    </MotionConfig>
  );
}
