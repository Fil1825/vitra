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

| #   | Stage                                                  | Status            |
| --- | ------------------------------------------------------ | ----------------- |
| 0   | Preparation: environment, structure, sidecars, plan    | in review (PR #1) |
| 1   | Shell: transparent window, vibrancy, `<Glass>`, themes | in review (PR #2) |
| 2   | Engine: mpv IPC, transport controls, artwork backdrop  | in review (PR #3) |
| 3   | Library & queue: scan, SQLite, views, search, session  | in review (PR #4) |
| 4   | Links: yt-dlp playback, playlists, downloads           | in review (PR #5) |
| 5   | Lyrics: LRC parser, tags, display, tap-sync editor     | not started       |
| 6   | Mini player                                            | not started       |
| 7   | Customization: theme editor, `.vitratheme`, layout     | not started       |
| 8   | Sound & system: EQ, crossfade, media keys, tray        | not started       |
| 9   | Integrations, i18n, CI builds, README, first release   | not started       |

## Commands

```sh
npm ci                     # install frontend deps and the Tauri CLI
npm run fetch-sidecars     # download mpv / yt-dlp / FFmpeg / Deno for this machine
npm run tauri dev          # run the app with hot reload
npm run tauri build        # release build + installers, without the sidecars
npm run bundle             # the same with the fetched sidecars inside (see key decision 6)

npm run check              # eslint, prettier, tsc, vitest, vite build
npm run check:rust         # cargo fmt --check, clippy -D warnings, cargo test
npm run check:all          # both of the above — must pass before every PR
npm run check:cross        # clippy for aarch64-apple-darwin + x86_64-pc-windows-msvc from Linux
VITRA_MPV_TESTS=1 cargo test -p mpv-ipc   # also run the tests against a real mpv (on PATH)
VITRA_YTDLP=src-tauri/sidecars/<triple>/yt-dlp cargo test -p links --test real_ytdlp
                                          # real yt-dlp against a local HTTP server (FFmpeg on PATH)
scripts/record-ytdlp-fixtures.sh          # re-record yt-dlp fixtures (needs the sites reachable)
```

Debugging playback: `VITRA_MPV_ARGS="--ao=null --log-file=/tmp/mpv.log" npm run tauri dev` passes
extra flags to mpv; files given on the command line (`target/debug/vitra song.flac`) play at launch.

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
# llvm-rc for `npm run check:cross` (Windows icon resource):
sudo apt-get install -y llvm
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
  main.tsx              boot: backend bridge → settings, environment, player, queue, library
                        stores → i18n → render
  app/                  shell layout (TitleBar, Sidebar, NowPlaying, ControlBar) and
                        useAppearanceController (theme vars, effect flags, window theme, language)
  components/           shared UI: Glass/ (glass surface, layer budget, refraction), Backdrop/,
                        ui/ (IconButton, SegmentedControl, Switch, Slider)
  features/<feature>/   feature UI: library/ (views, TrackTable, AlbumGrid, actions + menus),
                        queue/ (QueuePanel), tags/ (TagEditor), links/ (LinkDialog,
                        DownloadsView), settings/
  stores/               zustand stores: settings (mirror of Rust), environment, appearance,
                        player, queue, library (scan status), downloads, navigation (routes),
                        ui (panels, dialogs)
  theme/                zod schema, built-in themes (builtin/*.json), tokens → CSS vars, effects
  i18n/                 i18next setup, locales/en.json + ru.json (typed keys)
  lib/                  IPC wrappers, event bus + backend bridge, hooks, pure helpers
  styles/global.css     base document styles
  (planned) windows/mini/
src-tauri/              Tauri app crate (`vitra`, lib `vitra_lib`)
  src/lib.rs            builder, plugins, setup, command registration
  src/settings.rs       Settings struct, JSON persistence, get/set commands
  src/window.rs         window creation from config, vibrancy / Mica / Acrylic
  src/error.rs          AppError (serializes to a message for the frontend)
  src/sidecar.rs        locating mpv / yt-dlp / ffmpeg / ffprobe / deno
  src/player/           mpv supervisor, queue mirroring and commands (mod.rs, commands.rs),
                        PlayerState reducer (state.rs), play queue (queue.rs, pure),
                        session save/restore (session.rs), artwork:// scheme (artwork.rs)
  src/library/          LibraryService: DB connection, scanner thread, folder watcher
                        (watch.rs), library/playlist/tag-editor commands (commands.rs)
  src/links/            LinkService (resolve, play, import + background enrich, yt-dlp
                        update), Downloads (queue, 2 at a time), commands
  tauri.conf.json, capabilities/, icons/ (generated: `npm run tauri icon -- assets/brand/icon-1024.png`)
  sidecars/<triple>/    fetched binaries (git-ignored)
crates/                 pure-Rust crates testable without WebKit
  mpv-ipc/              async mpv JSON IPC client + spawning mpv (in-memory fake + opt-in real-mpv tests)
  media/                lofty tags (read + write), embedded/folder artwork, thumbnails, color
                        palette (fixtures in tests/)
  library/              SQLite library: schema + migrations, tracks/albums/artists, FTS5
                        search, playlists, history, session, incremental folder scanner,
                        links as tracks (remote.rs)
  links/                URL classification, yt-dlp -J parsing, file name templates, progress
                        lines, download state machine, YtDlp runner (fixtures + opt-in real
                        tests in tests/)
scripts/
  fetch-sidecars.mjs    downloader (I/O)
  sidecars/plan.mjs     what to download per target (pure, unit-tested)
assets/brand/           logo, app icon source, mono tray icon, palette (see its README)
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
5. **mpv** runs as a child process controlled over JSON IPC (`crates/mpv-ipc`).
   - macOS/Linux: one end of a socket pair is mpv's stdin (`--input-ipc-client=fd://0`); mpv
     quits when that connection closes, so it can't outlive the app even after a crash or
     `kill -9`. Windows: a named pipe (`--input-ipc-server`); a hard crash there can still leave
     an orphan until a Job Object is added (stage 9).
   - Flags (`player::mpv_args`): `--no-config` (ignores the user's mpv.conf and the bundle's
     pseudo-GUI config), `--load-scripts=no`, `--idle`, `--no-video`, `--no-terminal`,
     `--input-media-keys=no` (media keys/Now Playing are ours), `--ytdl-format=bestaudio/best`,
     plus `--media-controls=no` and `--macos-app-activation-policy=prohibited` (no Dock icon) for
     mpv 0.39+; if mpv exits at startup with those, it is restarted without them.
   - The supervisor restarts mpv after crashes (≤5 per minute), restoring volume, speed and the
     queue's current item with its position. Every spawn starts with an `mpv-version` handshake.
   - `PlayerState::apply` (pure, unit-tested) turns mpv events into UI state; the app emits
     `player://state`, throttled `player://position` (4/s, the UI extrapolates) and
     `player://track-changed`. Commands: `player_open/play/pause/toggle/seek/set_volume/...`.
   - Metadata comes from lofty off the async runtime; formats lofty can't read fall back to
     mpv's `metadata` property. Artwork (embedded, else `cover.jpg` & co.) is decoded once in
     `crates/media`: display image (≤1200 px), 96 px backdrop thumbnail, palette + accent.
   - Artwork reaches the WebView through the `artwork://` URI scheme (in-memory, content-
     addressed ids, CSP `img-src artwork: http://artwork.localhost`), never via file access.
     Library covers are `artwork://localhost/cover/<16 hex>` from `<app data>/covers/`.
6. **Sidecars** (helper binaries):
   - mpv: official builds pinned to `v0.41.0` with SHA-256 (`macos-14-arm`, `macos-15-intel`,
     `x86_64-pc-windows-msvc`). On macOS it is a whole `mpv.app` with dylibs in
     `Contents/MacOS/lib`, so it ships as a directory resource, not a Tauri `externalBin`.
     Linux uses the system mpv (no official binaries).
   - yt-dlp: latest release, checked against `SHA2-256SUMS`. Windows and Linux get the
     single-file build (`yt-dlp[.exe]`); **macOS gets the folder build** `yt-dlp_macos.zip`
     (`yt-dlp/yt-dlp_macos` + `_internal/`): the single-file build unpacks Python into a new
     temporary folder on every run and macOS checks those libraries each time (~25 s per
     call), while the folder build is checked once (~25 s on its very first run, then 0.2 s;
     the app runs `--version` at start to get that over with). The folder build can't
     `-U` itself.
   - FFmpeg + ffprobe (yt-dlp needs both for `-x`/embedding): macOS from
     ffmpeg.martin-riedl.de (static, signed), Windows/Linux from BtbN's GitHub releases (newest
     stable GPL build picked from `checksums.sha256`). Homebrew FFmpeg works as a fallback.
   - Deno: yt-dlp needs a JS runtime for YouTube since 2025.11 (EJS); Deno is its default.
     Passed explicitly with `--js-runtimes deno:<path>`.
   - Lookup order (`src-tauri/src/sidecar.rs`): app data `sidecars/` (updatable copies, e.g.
     yt-dlp self-update) → bundled (exe dir, `resources/sidecars/`) → dev
     `src-tauri/sidecars/<triple>/` (debug builds only) → `PATH` + Homebrew/`/usr/local/bin`
     (macOS GUI apps don't inherit the shell `PATH`). On macOS only `yt-dlp/yt-dlp_macos`
     counts in those folders; a single-file `yt-dlp` is used from `PATH` only.
   - Bundling: `npm run bundle` adds `src-tauri/tauri.bundle.conf.json`, whose
     `beforeBuildCommand` runs `scripts/stage-sidecars.mjs` (copies
     `sidecars/<TAURI_ENV_TARGET_TRIPLE>/` to `sidecars/bundle/`, with `ditto` on macOS) and
     whose `bundle.resources` ships that as `<resources>/sidecars/`. It isn't in
     `tauri.conf.json` because tauri-build requires resource folders to exist at compile time
     and copies them into `target/` on every build.
7. **Accent & backdrop from artwork**: themes opt in with `background.useArtwork` (blurred
   thumbnail + palette-colored gradient) and `accentFromArtwork`; the user can turn the accent
   off (`appearance.artworkAccent`). `theme/accent.ts` adjusts the color per light/dark.
8. **Frontend**: React 19, TypeScript 6.0 (strict), Vite 8, CSS Modules. Every visual value is
   a CSS custom property (`--vitra-*`), so a theme is just a set of variables. Zustand for
   stores, Motion for animation, Lucide icons — each added in the stage that first needs it.
9. **Multiple windows** = multiple Vite HTML entries (`index.html`, `mini.html`).
10. **Security**: strict CSP in production (`script-src 'self'`), no remote scripts. Themes are
    data (JSON + CSS + assets), never JavaScript; `custom.css` is sanitized on import.
    `devCsp` is `null` because Vite's dev server injects inline scripts — check CSP-sensitive
    work with `npm run tauri build`.
11. **Persistence**: SQLite (`rusqlite`, bundled, FTS5) in the app data dir (`library.db`, WAL)
    for library, playlists, lyrics, history and session; settings in a JSON file in the app
    config dir, owned by Rust. Migrations are numbered SQL batches (`PRAGMA user_version`);
    never edit a released one. A database that can't be opened is set aside, not fatal.
12. **Theme schema**: zod schema is the single source; `docs/theme.schema.json` is generated
    from it (`z.toJSONSchema`) and a test fails if the committed file is stale.
13. **LRC / enhanced LRC parsing and serialization in Rust** (one implementation shared by tag
    reading, `.lrc` files, FTS indexing and saving from the editor).
14. **i18n from stage 1** (i18next, en + ru) so strings are never hard-coded; stage 9 completes
    and reviews translations. (The spec lists i18n in stage 9; doing it early avoids a rewrite.)
15. **Windows are created in Rust** from their `tauri.conf.json` entry (`"create": false`) so
    transparency can differ per platform: Linux windows are opaque (no vibrancy to show, and a
    transparent window renders black without a compositor).
16. **Settings flow**: UI calls `useSettings().update` → `set_settings` command → Rust saves
    `settings.json` and emits `settings://changed` → `lib/bridge.ts` → bus → every window's store.
17. **Effects** (`theme/effects.ts`) combine OS preferences (`prefers-reduced-motion`,
    `prefers-reduced-transparency`), user overrides, power saving and engine support, and are
    exposed as `data-motion`, `data-transparency`, `data-backdrop-animation`, `data-vibrancy`,
    `data-platform` on `<html>` for CSS. At most 8 glass layers blur at once (priority-ranked).
18. **Refraction** needs SVG filters in `backdrop-filter`, which only Blink renders (Windows /
    WebView2). Detection: `CSS.supports` plus the Blink-only `navigator.userAgentData`; WebKit
    (macOS, Linux) gets blur + specular highlight. Settings can force it on/off.
19. **Window material** (macOS): `appearance.windowMaterial` = `vibrancy` (NSVisualEffectView) or
    `liquidGlass` (NSGlassEffectView, macOS 26+, falls back to vibrancy). Changing it re-applies
    the backdrop on the main thread and emits `window://effects-changed` to that window.
20. **Library** (`crates/library`, `src-tauri/src/library`):
    - Albums are keyed by title + album artist (falling back to the track artist); untitled
      tracks show their file name. Plays, favorites and playlist entries hang off track ids.
    - Scans are incremental (size + mtime), read tags on up to 8 threads, write in batches of
      250, follow moved/renamed files (same size and title → same row), never delete tracks of
      an unreadable folder or a cancelled scan, and date a folder's first scan by file mtime.
    - Album covers: 512 px JPEGs named by the FNV-1a hash of the source image, cached in
      `<app data>/covers`, garbage-collected after scans. Albums sharing a cover share the id,
      so each write goes through its own temporary file (`<id>.<pid>-<n>.part`).
    - Single tracks: files opened or dropped outside library folders join the library with
      `folder_id = NULL` (setting `library.addOpenedFiles`, on by default; done in the
      background after playback starts, then the queue entries get their track ids). Adding a
      folder that contains them takes them in (same rows, so favorites and history stay).
      Only single tracks can be removed from the library (`library_remove_tracks`); tracks in
      folders would come back with the next scan.
    - One scanner thread with its own connection; commands share another (`LibraryService`).
      `notify` watches folders; changes rescan the folder 3 s after things settle.
    - Search: FTS5 `unicode61 remove_diacritics 2`, every word a prefix (`"word"*`), ranked
      title > artist > album > album artist > lyrics.
    - Events: `library://changed` (`tracks` | `playlists` | `folders`) and `library://scan`;
      views reload through `useLibraryData`, which coalesces bursts.
21. **Queue** (`player/queue.rs`, pure): items in play order with unique `uid`s, `current`,
    shuffle (current item first, the original order kept to unshuffle), repeat off/all/one.
    Gapless: mpv's playlist holds the current item and `peek_next()` (`loadfile … append`,
    `--prefetch-playlist`); `start-file` of that entry advances the queue. A play counts at
    half the track (at most 4 min). The queue and playback values (position, volume, mute,
    speed) are saved to the `session` table every few seconds and on exit; the current item
    comes back paused. `queue://changed` carries the whole queue.
22. **Links** (`crates/links`, `src-tauri/src/links`, `features/links`):
    - Only `http(s)` links are accepted, and the URL always goes after `--` in yt-dlp's
      arguments. yt-dlp runs with `--ignore-config --no-warnings` and
      `--js-runtimes deno:<path>` when Deno is found.
    - Pages are resolved with `yt-dlp -J --flat-playlist --no-playlist`, writing the thumbnail
      in the same run (`--no-simulate --skip-download --write-thumbnail`); direct audio files
      and radio (`.m3u`/`.pls`/HLS) skip yt-dlp. No HTTP client of our own.
    - A link is a library track whose path is its page URL (`tracks.source_url`, migration 2);
      its thumbnail is the track's own cover (`tracks.cover`, shown before the album's).
      Listings (`complete: false`) never overwrite what a full description stored. Links
      join the library when played if `library.addOpenedFiles` is on; always when imported
      or downloaded.
    - Imported playlists keep `playlists.source_url`; importing again replaces their entries
      and keeps the name. Entries are filled in afterwards by `yt-dlp -j` (one line each).
    - mpv plays links through its own yt-dlp hook: `--script-opts-append=ytdl_hook-ytdl_path=`
      and `--ytdl-raw-options-append=js-runtimes=deno:` point it at our binaries. Radio
      playlists end in `end-file reason=redirect`, after which the next `start-file` is the
      same queue item; `metadata` is observed for `icy-title`.
    - Downloads: `-f bestaudio/best -x` (+ `--audio-format mp3|flac`), `--embed-metadata`,
      `--embed-thumbnail`, `--convert-thumbnails jpg`, `--no-mtime`; progress via
      `--progress-template` (download progress on stdout, post-processing on **stderr**) and
      `--print after_move:`; partial files in `<cache>/downloads/<job>`. Then Vitra writes its
      own title/artist/album/year and saved lyrics into the file and
      `Library::attach_download` swaps the stream's path for the file (same row; a row the
      watcher added first is merged in).
    - "Update yt-dlp": on macOS `links::release::install_zip_release` downloads the latest
      `yt-dlp_macos.zip` and `SHA2-256SUMS` with `curl`, checks the hash (`sha2`), unpacks
      with `ditto` and swaps the folder into `<app data>/sidecars/yt-dlp/` (a
      `.release-sha256` marker skips the same release). Elsewhere it copies a bundled
      yt-dlp to `<app data>/sidecars/` (found first from then on) and runs `-U` there; a
      system yt-dlp is updated in place (pip/Homebrew ones refuse).
    - While mpv opens a link (its hook waits for yt-dlp) the status is `loading`: the UI
      shows a spinner and disables seeking, and `Player::seek` defers to `file-loaded`.
23. **Context menus** are native (`@tauri-apps/api/menu`, `lib/menu.ts`); the WebView's own
    menu is suppressed outside text fields. Reordering uses a small pointer-based hook
    (`lib/reorder.ts`) because HTML5 drag and drop clashes with the window's file drop.
24. License **GPL-3.0-or-later** (compatible with mpv). Bundle identifier
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
- Never animate `opacity`, `filter` or `mask` on an ancestor of a `<Glass>`: it becomes a
  "backdrop root" and the glass stops seeing what's behind it. Animate `transform` instead.
- Headless UI checks: `npm run build && npx vite preview`, then drive it with `playwright-core`
  and the preinstalled Chromium (`/opt/pw-browsers/chromium-*/chrome-linux/chrome`). For the real
  app (WebKitGTK): `npx tauri build --debug --no-bundle`, run `target/debug/vitra` under
  `xvfb-run` and click with `xdotool`; settings land in `~/.config/io.github.fil1825.vitra/`.
- Platform-specific Rust is type-checked from Linux with `npm run check:cross` (also a CI job):
  `scripts/cross-check/fake-cc` stands in for the Apple and MSVC C compiler and archiver
  (objc2's build script and bundled SQLite need one) and writes empty files, since nothing is
  linked during clippy. Run it after touching `#[cfg(target_os = ...)]` code.
- `convertFileSrc` percent-encodes its whole argument, `/` included: build library cover URLs
  as `convertFileSrc('cover', 'artwork') + '/' + id` (see `coverUrl`).
- Lists that virtualize (`useVirtualRange`) measure against the closest
  `[data-scroll-container]` ancestor; the main area and the queue panel are such containers.
- The library database and covers live in `~/.local/share/io.github.fil1825.vitra/` on Linux
  (app data dir); to test a library under Xvfb, insert a row into `folders` or use the
  settings panel.
- macOS traffic lights (`trafficLightPosition` in `tauri.conf.json`): `x` is the close button's
  left edge; on Tahoe the buttons are 14 pt wide, 23 pt apart, and their center sits at `y − 2.5`
  (measured from the owner's screenshot). Current `{x: 23, y: 33}` centers the close button on
  the sidebar's corner arc and aligns the row with the title (y ≈ 30).
- The cloud can't reach YouTube or SoundCloud: test links with a local page
  (`python3 -m http.server` serving an HTML file with `<audio src=…>`; yt-dlp's generic
  extractor handles it, naming entries "Title (1)") and keep 127.0.0.1 in `NO_PROXY`.
- lofty can't write `ItemKey::Lyrics` to ID3v2; use `ItemKey::UnsyncLyrics` (USLT) there.
  lofty also warns about every odd MP3 frame; the app logs `lofty` at error level only.
- The macOS mpv tarball carries code-signature xattrs; `fetch-sidecars` moves extracted files
  with `rename` (never copies) to keep them. Linux GNU tar prints harmless warnings about them.
