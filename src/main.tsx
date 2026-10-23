import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './app/App.tsx';
import { initI18n, resolveLocale } from './i18n/index.ts';
import { adoptOldPanels } from './layout/state.ts';
import { startBackendBridge } from './lib/bridge.ts';
import { suppressWebViewMenu } from './lib/webview.ts';
import { initDownloads } from './stores/downloads.ts';
import { initEnvironment } from './stores/environment.ts';
import { initLibrary } from './stores/library.ts';
import { initLyrics } from './stores/lyrics.ts';
import { initPlayer } from './stores/player.ts';
import { initQueue } from './stores/queue.ts';
import { initSettings, useSettings } from './stores/settings.ts';
import { initThemes } from './stores/themes.ts';
import './styles/global.css';

async function boot(root: HTMLElement): Promise<void> {
  suppressWebViewMenu();
  // Start listening before loading so a change made meanwhile in another window isn't missed.
  await startBackendBridge();
  await Promise.all([
    initSettings(),
    initEnvironment(),
    initPlayer(),
    initQueue(),
    initLibrary(),
    initDownloads(),
    initLyrics(),
    initThemes(),
  ]);
  adoptOldPanels();
  await initI18n(resolveLocale(useSettings.getState().settings.language, navigator.languages));

  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

const root = document.getElementById('root');
if (!root) {
  throw new Error('Root element #root not found');
}
boot(root).catch((err: unknown) => {
  console.error('Vitra failed to start', err);
  root.textContent = `Vitra failed to start: ${String(err)}`;
});
