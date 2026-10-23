import {
  ChevronLeft,
  ChevronRight,
  FolderOpen,
  LayoutDashboard,
  Link2,
  PanelLeftClose,
  PanelLeftOpen,
  Settings2,
} from 'lucide-react';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../components/ui/IconButton.tsx';
import {
  focusSearch,
  nextSidebarMode,
  setCustomizing,
  setSidebarMode,
  sidebarMode,
  useCustomizing,
  useLayout,
} from '../layout/state.ts';
import { pickAndPlay } from '../lib/openFiles.ts';
import { useEnvironment } from '../stores/environment.ts';
import { useNavigation } from '../stores/navigation.ts';
import { useSettings } from '../stores/settings.ts';
import { useUi } from '../stores/ui.ts';
import styles from './TitleBar.module.css';

export interface TitleBarProps {
  className?: string;
  onOpenSettings(): void;
}

/** Top strip of the content area: history navigation and actions; doubles as a drag region. */
export function TitleBar({ className, onOpenSettings }: TitleBarProps) {
  const { t } = useTranslation();
  const canGoBack = useNavigation((s) => s.back.length > 0);
  const canGoForward = useNavigation((s) => s.forward.length > 0);
  const goBack = useNavigation((s) => s.goBack);
  const goForward = useNavigation((s) => s.goForward);
  const sidebar = sidebarMode(useLayout());
  const customizing = useCustomizing((s) => s.on);
  const mac = useEnvironment((s) => s.platform === 'macos');
  const nextSidebar = nextSidebarMode(sidebar);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey;
      // Cmd+Option+S / Ctrl+Alt+S. Option changes `key` on macOS (ß), so match the key itself.
      if (command && event.altKey && !event.shiftKey && event.code === 'KeyS') {
        event.preventDefault();
        setSidebarMode(nextSidebarMode(sidebarMode(useSettings.getState().settings.layout)));
        return;
      }
      if (command && !event.altKey && event.key.toLowerCase() === 'f') {
        event.preventDefault();
        focusSearch();
        return;
      }
      // Cmd+[ / Cmd+] (macOS) and Alt+←/→ (elsewhere), like a browser.
      const back =
        ((event.metaKey || event.ctrlKey) && event.key === '[') ||
        (event.altKey && event.key === 'ArrowLeft');
      const forward =
        ((event.metaKey || event.ctrlKey) && event.key === ']') ||
        (event.altKey && event.key === 'ArrowRight');
      const typing = event.target instanceof HTMLInputElement;
      if ((back || forward) && !typing) {
        event.preventDefault();
        if (back) {
          useNavigation.getState().goBack();
        } else {
          useNavigation.getState().goForward();
        }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <header className={`${styles.titleBar} ${className ?? ''}`} data-tauri-drag-region>
      <div className={styles.history}>
        <IconButton
          label={`${t(`nav.sidebarNext.${nextSidebar}`)} (${mac ? '⌥⌘S' : 'Ctrl+Alt+S'})`}
          onClick={() => setSidebarMode(nextSidebar)}
        >
          {sidebar === 'hidden' ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
        </IconButton>
        <IconButton label={t('nav.back')} disabled={!canGoBack} onClick={goBack}>
          <ChevronLeft size={20} />
        </IconButton>
        <IconButton label={t('nav.forward')} disabled={!canGoForward} onClick={goForward}>
          <ChevronRight size={20} />
        </IconButton>
      </div>
      <div className={styles.actions}>
        <IconButton
          label={t('player.open')}
          onClick={() =>
            pickAndPlay().catch((err: unknown) => console.error('Opening files failed', err))
          }
        >
          <FolderOpen size={18} />
        </IconButton>
        <IconButton label={t('links.openLink')} onClick={() => useUi.getState().openLink('')}>
          <Link2 size={18} />
        </IconButton>
        <IconButton
          label={t('layout.customize')}
          aria-pressed={customizing}
          onClick={() => setCustomizing(!customizing)}
        >
          <LayoutDashboard size={18} />
        </IconButton>
        <IconButton label={t('settings.open')} onClick={onOpenSettings}>
          <Settings2 size={18} />
        </IconButton>
      </div>
    </header>
  );
}
