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
import { useTranslation } from 'react-i18next';

import { IconButton } from '../components/ui/IconButton.tsx';
import {
  nextSidebarMode,
  setCustomizing,
  setSidebarMode,
  sidebarMode,
  useCustomizing,
  useLayout,
} from '../layout/state.ts';
import { pickAndPlay } from '../lib/openFiles.ts';
import { useShortcutLabel, withShortcut } from '../shortcuts/useShortcuts.ts';
import { useNavigation } from '../stores/navigation.ts';
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
  const nextSidebar = nextSidebarMode(sidebar);
  const shortcut = {
    sidebar: useShortcutLabel('sidebar'),
    back: useShortcutLabel('back'),
    forward: useShortcutLabel('forward'),
    open: useShortcutLabel('open-files'),
    link: useShortcutLabel('open-link'),
    customize: useShortcutLabel('customize-layout'),
    settings: useShortcutLabel('settings'),
  };

  return (
    <header className={`${styles.titleBar} ${className ?? ''}`} data-tauri-drag-region>
      <div className={styles.history}>
        <IconButton
          label={withShortcut(t(`nav.sidebarNext.${nextSidebar}`), shortcut.sidebar)}
          onClick={() => setSidebarMode(nextSidebar)}
        >
          {sidebar === 'hidden' ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}
        </IconButton>
        <IconButton
          label={withShortcut(t('nav.back'), shortcut.back)}
          disabled={!canGoBack}
          onClick={goBack}
        >
          <ChevronLeft size={20} />
        </IconButton>
        <IconButton
          label={withShortcut(t('nav.forward'), shortcut.forward)}
          disabled={!canGoForward}
          onClick={goForward}
        >
          <ChevronRight size={20} />
        </IconButton>
      </div>
      <div className={styles.actions}>
        <IconButton
          label={withShortcut(t('player.open'), shortcut.open)}
          onClick={() =>
            pickAndPlay().catch((err: unknown) => console.error('Opening files failed', err))
          }
        >
          <FolderOpen size={18} />
        </IconButton>
        <IconButton
          label={withShortcut(t('links.openLink'), shortcut.link)}
          onClick={() => useUi.getState().openLink('')}
        >
          <Link2 size={18} />
        </IconButton>
        <IconButton
          label={withShortcut(t('layout.customize'), shortcut.customize)}
          aria-pressed={customizing}
          onClick={() => setCustomizing(!customizing)}
        >
          <LayoutDashboard size={18} />
        </IconButton>
        <IconButton
          label={withShortcut(t('settings.open'), shortcut.settings)}
          onClick={onOpenSettings}
        >
          <Settings2 size={18} />
        </IconButton>
      </div>
    </header>
  );
}
