# Vitra — working notes for Claude

Vitra is a cross-platform open-source desktop music player with a Liquid Glass look
(Tauri 2 + React). Requirements live in [`SPEC.md`](SPEC.md) (Russian, the source of truth);
the stage-by-stage plan is in [`docs/PLAN.md`](docs/PLAN.md).

- Talk to the owner in **Russian**. Code, comments, commit messages, docs and README in **English**.
- Ask instead of guessing when the spec is ambiguous.
- [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `chore:`, `docs:` ...).
- One branch + pull request per stage. Every PR description has a **"How to verify on macOS"**
  section (commands, what to click, expected result) and lists checks that only work on a Mac.
- The owner develops on macOS (Apple Silicon); Windows x64 and Linux must build and work too.
- Don't add heavy dependencies without need; justify new ones in the PR.

## Stage status

| #   | Stage                                                  | Status      |
| --- | ------------------------------------------------------ | ----------- |
| 0   | Preparation: environment, structure, sidecars, plan    | in review   |
| 1   | Shell: transparent window, vibrancy, `<Glass>`, themes | not started |
| 2   | Engine: mpv IPC, transport controls, artwork backdrop  | not started |
| 3   | Library & queue: scan, SQLite, views, search, session  | not started |
| 4   | Links: yt-dlp playback, playlists, downloads           | not started |
| 5   | Lyrics: LRC parser, tags, display, tap-sync editor     | not started |
| 6   | Mini player                                            | not started |
| 7   | Customization: theme editor, `.vitratheme`, layout     | not started |
| 8   | Sound & system: EQ, crossfade, media keys, tray        | not started |
| 9   | Integrations, i18n, CI builds, README, first release   | not started |

## Commands

```sh
npm ci                     # install frontend deps and the Tauri CLI
npm run fetch-sidecars     # download mpv / yt-dlp / FFmpeg / Deno for this machine
npm run tauri dev          # run the app with hot reload
npm run tauri build        # release build + installers

npm run check              # eslint, prettier, tsc, vitest, vite build
npm run check:rust         # cargo fmt --check, clippy -D warnings, cargo test
npm run check:all          # both of the above — must pass before every PR
```

`fetch-sidecars` options: `--target <triple>`, `--only mpv,yt-dlp`, `--skip deno`, `--force`,
`--out <dir>`, `--require-checksums` (see `--help`).

### Linux system dependencies (cloud environment, Ubuntu 24.04)

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev \
  libjavascriptcoregtk-4.1-dev libgtk-3-dev libdbus-1-dev
# Runtime for engine tests (no official Linux mpv binaries exist):
sudo apt-get install -y mpv
```

`libdbus-1-dev` is for MPRIS (media keys) via `souvlaki` in stage 8.

### Cloud environment notes

- No GUI: verify with builds and tests; the owner checks the UI on a Mac.
- Outbound traffic goes through a proxy. npm, crates.io, Ubuntu archives and GitHub release
  downloads work; `api.github.com`, `mpv.io` and `ffmpeg.martin-riedl.de` (macOS FFmpeg) do not.
- Node's `fetch` ignores `HTTPS_PROXY` unless `NODE_USE_ENV_PROXY=1`; `fetch-sidecars`
  re-launches itself with it when a proxy is configured.
- Toolchain at stage 0: Rust 1.97, Node 22.22, npm 10.9, Tauri CLI 2.12 (npm devDependency,
  use `npx tauri`, not `cargo tauri`).

## Repository layout

```
Cargo.toml              Cargo workspace (members: src-tauri, later crates/*)
package.json            frontend + scripts; Tauri CLI is a devDependency
index.html              main window entry (mini player gets mini.html in stage 6)
src/                    React + TypeScript frontend
  main.tsx, App.tsx
  lib/                  IPC wrappers, event bus, pure helpers
  styles/               global CSS, theme tokens
  (planned) components/ shared UI (Glass, Slider, ...), features/<feature>/, stores/,
            theme/, i18n/, windows/mini/
src-tauri/              Tauri app crate (`vitra`, lib `vitra_lib`)
  src/lib.rs            builder, plugins, command registration
  src/sidecar.rs        locating mpv / yt-dlp / ffmpeg / ffprobe / deno
  tauri.conf.json, capabilities/, icons/ (generated from assets/icon.svg via `npx tauri icon`)
  sidecars/<triple>/    fetched binaries (git-ignored)
crates/                 (planned) pure-Rust crates testable without WebKit, e.g. mpv-ipc, lyrics
scripts/
  fetch-sidecars.mjs    downloader (I/O)
  sidecars/plan.mjs     what to download per target (pure, unit-tested)
assets/icon.svg         app icon source (original artwork, no Apple assets)
docs/                   PLAN.md; later THEMES.md, theme.schema.json
themes/                 (stage 7) example themes
.github/workflows/      CI
```

## Key decisions

1. **Tauri 2 stable** (2.12). crates.io already has Tauri 3 alphas — do not upgrade.
2. **Cargo workspace** at the repo root. Logic that doesn't need Tauri (mpv IPC client, LRC
   parser, library DB) goes into `crates/*` so it can be tested fast and without a WebView.
3. **Rust owns the state** of playback, queue, library and settings; windows are views that
   mirror it through events. Reason: the mini player, tray, media keys and global hotkeys must
   work regardless of which window is open.
4. **Event bus.** Backend emits typed Tauri events (`player://state`, `player://track-changed`,
   `lyrics://line`, ...). The frontend has a small typed bus (`src/lib/bus.ts`, stage 1) that
   re-publishes them; UI modules subscribe to the bus, never to Tauri directly. This is the
   future plugin API surface (plugins are not implemented in v1).
5. **mpv** runs as a child process controlled over JSON IPC (Unix socket on macOS/Linux, named
   pipe on Windows). Always start it with `--no-config` (ignore the user's mpv.conf and the
   bundle's `pseudo-gui` config), `--idle=yes`, `--no-video`, `--no-terminal`,
   `--input-ipc-server=...`, `--ytdl-format=bestaudio/best`,
   `--script-opts=ytdl_hook-ytdl_path=<yt-dlp>`. Engine code sits behind a trait so tests can
   mock it.
6. **Sidecars** (helper binaries):
   - mpv: official builds pinned to `v0.41.0` with SHA-256 (`macos-14-arm`, `macos-15-intel`,
     `x86_64-pc-windows-msvc`). On macOS it is a whole `mpv.app` with dylibs in
     `Contents/MacOS/lib`, so it ships as a directory resource, not a Tauri `externalBin`.
     Linux uses the system mpv (no official binaries).
   - yt-dlp: latest standalone release, checked against `SHA2-256SUMS`.
   - FFmpeg + ffprobe (yt-dlp needs both for `-x`/embedding): macOS from
     ffmpeg.martin-riedl.de (static, signed), Windows/Linux from BtbN's GitHub releases (newest
     stable GPL build picked from `checksums.sha256`). Homebrew FFmpeg works as a fallback.
   - Deno: yt-dlp needs a JS runtime for YouTube since 2025.11 (EJS); Deno is its default.
     Passed explicitly with `--js-runtimes deno:<path>`.
   - Lookup order (`src-tauri/src/sidecar.rs`): app data `sidecars/` (updatable copies, e.g.
     yt-dlp self-update) → bundled (exe dir, `resources/sidecars/`) → dev
     `src-tauri/sidecars/<triple>/` (debug builds only) → `PATH` + Homebrew/`/usr/local/bin`
     (macOS GUI apps don't inherit the shell `PATH`).
7. **Frontend**: React 19, TypeScript 6.0 (strict), Vite 8, CSS Modules. Every visual value is
   a CSS custom property (`--vitra-*`), so a theme is just a set of variables. Zustand for
   stores, Motion for animation, Lucide icons — each added in the stage that first needs it.
8. **Multiple windows** = multiple Vite HTML entries (`index.html`, `mini.html`).
9. **Security**: strict CSP in production (`script-src 'self'`), no remote scripts. Themes are
   data (JSON + CSS + assets), never JavaScript; `custom.css` is sanitized on import.
   `devCsp` is `null` because Vite's dev server injects inline scripts — check CSP-sensitive
   work with `npm run tauri build`.
10. **Persistence**: SQLite (`rusqlite`, bundled, FTS5) in the app data dir for library,
    playlists, lyrics, history and session; settings in a JSON file in the app config dir,
    owned by Rust.
11. **Theme schema**: zod schema is the single source; `docs/theme.schema.json` is generated
    from it (`z.toJSONSchema`) and a test fails if the committed file is stale.
12. **LRC / enhanced LRC parsing and serialization in Rust** (one implementation shared by tag
    reading, `.lrc` files, FTS indexing and saving from the editor).
13. **i18n from stage 1** (i18next, en + ru) so strings are never hard-coded; stage 9 completes
    and reviews translations. (The spec lists i18n in stage 9; doing it early avoids a rewrite.)
14. License **GPL-3.0-or-later** (compatible with mpv). Bundle identifier
    `io.github.fil1825.vitra`. macOS minimum **14.0** (official mpv builds require it).

## Conventions

- Rust: edition 2024, `cargo fmt`, `clippy -D warnings`, `unsafe_code = "deny"` (allow locally
  with a comment if a platform API needs it). Commands return `Result<T, AppError>` with a
  serializable error (stage 2). No `unwrap()` outside tests and proven invariants.
- TypeScript: no `any`; IPC goes through typed wrappers in `src/lib/`; components never hard-code
  colors, radii, blur — use theme variables.
- Tests: pure logic gets unit tests (LRC parser, theme validation, queue logic, sidecar plan);
  mpv is mocked in unit tests, real-mpv integration tests are opt-in.
- Comments explain _why_, not _what_. Keep files focused; prefer small modules.

## Gotchas

- TypeScript is pinned to `~6.0`: `typescript-eslint` 8.x does not support TypeScript 7 yet.
- `tauri::generate_context!` does not need `dist/` in debug builds, so `cargo check/clippy/test`
  work without building the frontend; `tauri build` needs it (it runs `npm run build` itself).
- Static FFmpeg + ffprobe are ~330 MB on Windows/Linux and Deno ~100 MB: installer size is an
  open question (see docs/PLAN.md).
- The macOS mpv tarball carries code-signature xattrs; `fetch-sidecars` moves extracted files
  with `rename` (never copies) to keep them. Linux GNU tar prints harmless warnings about them.
