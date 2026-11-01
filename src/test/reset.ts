import { useGlassBudget } from '../components/Glass/budget.ts';
import { i18n } from '../i18n/index.ts';
import { clearLibraryDataCache } from '../lib/useLibraryData.ts';
import { INITIAL_PLAYER_STATE } from '../lib/player.ts';
import { EMPTY_QUEUE } from '../lib/queue.ts';
import { DEFAULT_SETTINGS } from '../lib/settings.ts';
import { useNavigation } from '../stores/navigation.ts';
import { usePlayer } from '../stores/player.ts';
import { useQueue } from '../stores/queue.ts';
import { useSettings } from '../stores/settings.ts';
import { useThemeEditor } from '../stores/themeEditor.ts';
import { useThemes } from '../stores/themes.ts';
import { useUi } from '../stores/ui.ts';
import { resetApp } from './mocks/app.ts';
import { resetCore } from './mocks/core.ts';
import { resetDialog } from './mocks/dialog.ts';
import { resetEvents } from './mocks/event.ts';
import { resetMenu } from './mocks/menu.ts';
import { resetWebview } from './mocks/webview.ts';
import { resetWindow } from './mocks/window.ts';

/** Puts the mirrored stores and the Tauri fakes back to where a fresh window starts. */
export async function resetUi(): Promise<void> {
  clearLibraryDataCache();
  usePlayer.setState({ state: { ...INITIAL_PLAYER_STATE }, positionAt: 0 });
  useQueue.setState({ queue: structuredClone(EMPTY_QUEUE) });
  useSettings.setState({ settings: structuredClone(DEFAULT_SETTINGS) });
  useNavigation.setState({ route: { view: 'albums' }, back: [], forward: [] });
  useUi.setState({
    searchRequested: false,
    tagEditorPath: null,
    lyricsEditor: null,
    artworkView: null,
    coverDrop: null,
    renamingPlaylist: null,
    linkInput: null,
    importInput: null,
    settingsOpen: false,
    settingsSection: 'general',
  });
  useThemeEditor.setState({ draft: null });
  useThemes.setState({ user: [], broken: [] });
  useGlassBudget.setState({ slots: new Map(), nextOrder: 0 });
  resetCore();
  resetEvents();
  resetWindow();
  resetWebview();
  resetMenu();
  resetDialog();
  resetApp();
  if (i18n.language !== 'en') await i18n.changeLanguage('en');
}
