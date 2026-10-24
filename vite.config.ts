/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

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
    include: ['src/**/*.test.{ts,tsx}', 'scripts/**/*.test.mjs'],
    environment: 'node',
    // Stylesheets reach tests empty, except the icons' motions (a test checks them).
    css: { include: [/styles\/icons\.css/] },
  },
});
