import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './app/App.tsx';
import { initI18n, resolveLocale } from './i18n/index.ts';
import { startBackendBridge } from './lib/bridge.ts';
import { initEnvironment } from './stores/environment.ts';
import { initPlayer } from './stores/player.ts';
import { initSettings, useSettings } from './stores/settings.ts';
import './styles/global.css';

async function boot(root: HTMLElement): Promise<void> {
  // Start listening before loading so a change made meanwhile in another window isn't missed.
  await startBackendBridge();
  await Promise.all([initSettings(), initEnvironment(), initPlayer()]);
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
