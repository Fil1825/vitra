/// <reference types="vitest/config" />
import { fileURLToPath } from 'node:url';

import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Set by the Tauri CLI when developing against a remote device.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  // Tauri expects a fixed port and prints its own errors.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: {
      ignored: ['**/src-tauri/**', '**/target/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    // WebView2 (Chromium) on Windows, WebKit on macOS (14+) and Linux.
    target: ['es2022', 'chrome120', 'safari17'],
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // Assets load from disk inside the app, so the web-oriented 500 kB warning doesn't apply.
    chunkSizeWarningLimit: 1500,
    // One HTML entry per window; shared code goes into common chunks.
    rollupOptions: {
      input: {
        main: 'index.html',
        mini: 'mini.html',
      },
    },
  },
  test: {
    // Stylesheets reach unit tests empty, except the icons' motions (a test checks them).
    // Component tests turn CSS on so CSS modules keep their class names.
    css: { include: [/styles\/icons\.css/] },
    // Kept transforms are keyed by the file's own text, so a test's `import.meta.glob`
    // (the icons' test reads every source) kept missing files added since: local runs
    // passed where CI failed.
    fsModuleCache: false,
    // Two projects so the existing node tests stay in node (some stub `document`
    // themselves) while components render in jsdom. A `.test.tsx` file is a component
    // test; everything else stays a unit test.
    projects: [
      {
        extends: true,
        test: {
          name: 'unit',
          environment: 'node',
          include: ['src/**/*.test.ts', 'scripts/**/*.test.mjs'],
        },
      },
      {
        extends: true,
        test: {
          name: 'components',
          environment: 'jsdom',
          include: ['src/**/*.test.tsx'],
          setupFiles: ['src/test/setup.ts'],
          css: true,
          // Desktop-only modules resolve to in-repo fakes. The unit project does not
          // inherit this, so its own `vi.mock` calls keep seeing the real packages.
          alias: [
            {
              find: '@tauri-apps/api/core',
              replacement: fileURLToPath(new URL('./src/test/mocks/core.ts', import.meta.url)),
            },
            {
              find: '@tauri-apps/api/event',
              replacement: fileURLToPath(new URL('./src/test/mocks/event.ts', import.meta.url)),
            },
            {
              find: '@tauri-apps/api/window',
              replacement: fileURLToPath(new URL('./src/test/mocks/window.ts', import.meta.url)),
            },
            {
              find: '@tauri-apps/api/webview',
              replacement: fileURLToPath(new URL('./src/test/mocks/webview.ts', import.meta.url)),
            },
            {
              find: '@tauri-apps/api/menu',
              replacement: fileURLToPath(new URL('./src/test/mocks/menu.ts', import.meta.url)),
            },
            {
              find: '@tauri-apps/api/app',
              replacement: fileURLToPath(new URL('./src/test/mocks/app.ts', import.meta.url)),
            },
            {
              find: '@tauri-apps/plugin-dialog',
              replacement: fileURLToPath(new URL('./src/test/mocks/dialog.ts', import.meta.url)),
            },
          ],
        },
      },
    ],
  },
});
