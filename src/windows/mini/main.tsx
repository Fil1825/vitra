import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { initI18n, resolveLocale } from '../../i18n/index.ts';
import { startBackendBridge } from '../../lib/bridge.ts';
import { suppressWebViewMenu } from '../../lib/webview.ts';
import { initEnvironment } from '../../stores/environment.ts';
import { initLyrics } from '../../stores/lyrics.ts';
import { initPlayer } from '../../stores/player.ts';
import { initQueue } from '../../stores/queue.ts';
import { initSettings, useSettings } from '../../stores/settings.ts';
import { initThemes } from '../../stores/themes.ts';
import '../../styles/global.css';
import './mini.css';
import { MiniApp } from './MiniApp.tsx';

/** The mini player window: the same stores as the main window, minus the library's. */
async function boot(root: HTMLElement): Promise<void> {
  suppressWebViewMenu();
  await startBackendBridge();
  await Promise.all([
    initSettings(),
    initEnvironment(),
    initPlayer(),
    initQueue(),
    initLyrics(),
    initThemes(),
  ]);
  await initI18n(resolveLocale(useSettings.getState().settings.language, navigator.languages));

  createRoot(root).render(
    <StrictMode>
      <MiniApp />
    </StrictMode>,
  );
}

const root = document.getElementById('root');
if (!root) {
  throw new Error('Root element #root not found');
}
boot(root).catch((err: unknown) => {
  console.error('The mini player failed to start', err);
  root.textContent = `The mini player failed to start: ${String(err)}`;
});
