import { AnimatePresence, MotionConfig } from 'motion/react';
import { useEffect, useState } from 'react';

import { Backdrop } from '../components/Backdrop/Backdrop.tsx';
import { SettingsPanel } from '../features/settings/SettingsPanel.tsx';
import { useAppearance } from '../stores/appearance.ts';
import styles from './App.module.css';
import { ControlBar } from './ControlBar.tsx';
import { NowPlaying } from './NowPlaying.tsx';
import { Sidebar } from './Sidebar.tsx';
import { TitleBar } from './TitleBar.tsx';
import { useAppearanceController } from './useAppearanceController.ts';

export function App() {
  useAppearanceController();
  const motionEnabled = useAppearance((s) => s.effects.motion);
  const [settingsOpen, setSettingsOpen] = useState(false);

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
      <Backdrop />
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
    </MotionConfig>
  );
}
