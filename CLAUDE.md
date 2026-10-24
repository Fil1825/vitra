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

| #   | Stage                                                  | Status             |
| --- | ------------------------------------------------------ | ------------------ |
| 0   | Preparation: environment, structure, sidecars, plan    | in review (PR #1)  |
| 1   | Shell: transparent window, vibrancy, `<Glass>`, themes | in review (PR #2)  |
| 2   | Engine: mpv IPC, transport controls, artwork backdrop  | in review (PR #3)  |
| 3   | Library & queue: scan, SQLite, views, search, session  | in review (PR #4)  |
| 4   | Links: yt-dlp playback, playlists, downloads           | in review (PR #5)  |
| 5   | Lyrics: LRC parser, tags, display, tap-sync editor     | accepted (PR #6)   |
| 6   | Mini player                                            | accepted (PR #7)   |
| 7   | Customization: theme editor, `.vitratheme`, layout     | accepted (PR #8)   |
| 8   | Sound & system: EQ, crossfade, media keys, tray        | accepted (PR #9)   |
| 9   | Playlist import: Spotify, Yandex, VK, Apple Music…     | in review (PR #10) |
| 10  | My Wave: mood analysis, endless mood-based queue       | not started        |
| 11  | Integrations, i18n, CI builds, README, first release   | not started        |

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
npm run theme-schema       # regenerate docs/theme.schema.json after changing src/theme/schema.ts
cargo run -p themes --example pack -- themes/dunes   # pack a theme folder → target/themes/*.vitratheme
VITRA_MPV_TESTS=1 cargo test -p mpv-ipc   # also run the tests against a real mpv (on PATH)
VITRA_YTDLP=src-tauri/sidecars/<triple>/yt-dlp cargo test -p links --test real_ytdlp
                                          # real yt-dlp against a local HTTP server (FFmpeg on PATH;
                                          # relative paths are from the repo root; macOS:
                                          # .../<triple>/yt-dlp/yt-dlp_macos)
scripts/record-ytdlp-fixtures.sh          # record real pages as *-recorded.json fixtures
scripts/record-import-fixtures.sh         # record Spotify / Apple / Deezer / Yandex pages for
                                          # crates/import (needs those sites; not the cloud)
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
  use `npx tauri`, not `cargo tauri`). CI uses the latest stable Rust (1.99 at stage 5), whose
  clippy may add lints: `rustup update stable` before `check:rust`.
- The session's disk allowance fills up with `target/` (debug, incremental, cross-check
  targets): `cargo clean` when "no space left on device" appears.

## Repository layout

```
Cargo.toml              Cargo workspace (members: src-tauri, later crates/*)
package.json            frontend + scripts; Tauri CLI is a devDependency
index.html, mini.html   window entries: main window, mini player
src/                    React + TypeScript frontend
  main.tsx              boot: backend bridge → settings, environment, player, queue, library,
                        downloads, lyrics, themes stores → old panel state into the layout →
                        i18n → render
  app/                  the window's parts (TitleBar, Sidebar, NowPlaying, ControlBar),
                        useAppearanceController (theme vars, user theme CSS, effect flags,
                        window theme, language), useLevelMeter (asks for the pulse and the
                        spectrum) and useTrayLabels (the tray menu's words)
  layout/               modular layout: model.ts (zones, modules, normalize; pure), state.ts
                        (in the settings, customize mode), Workspace (zones, resizing, dnd-kit),
                        Palette (customize mode), presets.ts, modules/ (ArtworkPanel,
                        LevelMeter: the spectrum on a canvas; spectrum.ts: its motion, pure)
  shortcuts/            keyboard shortcuts: keys.ts (combinations, labels, focus rules; pure),
                        actions.ts (actions, defaults, rebinding in the window and in every
                        app; pure), run.ts, useShortcuts
  components/           shared UI: Glass/ (glass surface, layer budget, refraction), Backdrop/,
                        ArtworkViewer/ (full-screen artwork), CoverArt/ (the player's covers:
                        records, their spin clock spin.ts, the no-cover placeholder), ui/ (IconButton,
                        SegmentedControl, Switch, Slider)
  features/<feature>/   feature UI: library/ (views, TrackTable, AlbumGrid, actions + menus,
                        HeroCover + covers.ts: covers the user chooses),
                        queue/ (QueuePanel), tags/ (TagEditor, files and links; album.ts:
                        which album a title + artist means), links/ (LinkDialog,
                        DownloadsView, download groups), lyrics/ (LyricsView, LyricsPanel, LyricsEditor +
                        SyncPane, tap-sync model sync.ts, Karaoke + clock.ts shared with the
                        mini player), settings/ (SettingsPanel: pages listed in sections.ts
                        with the search; a component per page, Row/Group; about.ts: the
                        owner's words and donation link), sound/ (equalizer presets.ts),
                        themes/ (ThemeEditor, fields, editor.ts: the draft; actions.ts:
                        import, export, duplicate), import/ (ImportDialog with the Music tab,
                        FindDialog, ImportStatus + useImportProgress, entries.ts: rows for
                        entries and the "?" menu, errors.ts)
  stores/               zustand stores: settings (mirror of Rust), environment, appearance,
                        player, queue, library (scan status), downloads, lyrics, navigation
                        (routes), ui (dialogs, Settings, artwork viewer), themes (installed),
                        themeEditor (the draft being edited)
  theme/                zod schema (+ jsonSchema.ts), built-in themes (builtin/*.json), tokens →
                        CSS vars, effects, user themes (user.ts), custom.css cleaning (css.ts),
                        color helpers, artwork.ts (whether covers are records)
  i18n/                 i18next setup, locales/en.json + ru.json (typed keys)
  lib/                  IPC wrappers (system.ts: tray and shortcuts in every app; browser.ts),
                        event bus + backend bridge, hooks, pure helpers
  styles/global.css     base document styles, the Vitra Sans @font-face rules
  styles/icons.css      every icon's motion under the pointer (key decision 39)
  assets/fonts/         Vitra Sans: Outfit (Latin) + Onest (Cyrillic) woff2, their OFL.txt
  windows/mini/         the mini player window: boot (main.tsx), MiniApp (three modes),
                        context menu (menu.ts)
src-tauri/              Tauri app crate (`vitra`, lib `vitra_lib`)
  src/lib.rs            builder, plugins, setup, command registration
  src/settings.rs       Settings struct, JSON persistence, get/set commands
  src/layout.rs         the layout in the settings (normalized like the frontend's), presets
  src/shortcuts.rs      the user's shortcut changes in the settings (kept tidy)
  src/global_shortcuts.rs  shortcuts in every app: registered with the system, run in Rust
  src/tray.rs           the menu-bar / notification-area icon and its menu
  src/now_playing/      the system's Now Playing and media keys: what changed (mod.rs, pure
                        Publisher), macOS MediaPlayer (macos.rs), souvlaki elsewhere
                        (controls.rs)
  src/sound/            SoundSettings + equalizer filter (mod.rs), loudness gains, R128
                        parsing and the links' integrator (loudness.rs, pure), FFmpeg
                        measurements in the background (measure.rs)
  src/browser.rs        opens web pages (About) in the default browser
  src/http.rs           GET with curl (pages for the import; cookie files)
  src/import/           ImportService: text / files / links / the Music app → playlists,
                        the background matcher (library, then YouTube Music), picks, Find…;
                        music.rs (macOS: iTunesLibrary), commands
  Info.plist            merged into the bundle's (and embedded in debug builds): the Media &
                        Apple Music usage text
  src/themes.rs         ThemeStore: installed themes, theme:// scheme, import/export commands
  src/window.rs         window creation from config, vibrancy / Mica / Acrylic
  src/error.rs          AppError (serializes to a message for the frontend)
  src/mini/             mini player window: show/hide and the main window, sizes per mode,
                        snapping, saved position (geometry.rs, pure), click-through with ⌥
                        (passthrough.rs), macOS / Windows specifics (platform.rs), commands
  src/sidecar.rs        locating mpv / yt-dlp / ffmpeg / ffprobe / deno
  src/player/           mpv supervisor, queue mirroring and commands (mod.rs, commands.rs),
                        PlayerState reducer (state.rs), play queue (queue.rs, pure),
                        session save/restore (session.rs), artwork:// scheme (artwork.rs),
                        loudness for the backdrop pulse (level.rs), the level meter's
                        spectrum (spectrum.rs), crossfade timing and curves (crossfade.rs)
  src/library/          LibraryService: DB connection, scanner thread, folder watcher
                        (watch.rs), library/playlist/tag-editor commands (commands.rs)
  src/links/            LinkService (resolve, play, import + background enrich, yt-dlp
                        update), Downloads (queue, 2 at a time), commands
  src/lyrics/           LyricsService (playing track's lyrics, offset, line events), source
                        priority (sources.rs), LRCLIB via curl (online.rs), backups before
                        tag writes (backup.rs), editor commands
  tauri.conf.json, capabilities/, icons/ (generated: `npm run tauri icon -- assets/brand/icon-1024.png`)
  sidecars/<triple>/    fetched binaries (git-ignored)
crates/                 pure-Rust crates testable without WebKit
  mpv-ipc/              async mpv JSON IPC client + spawning mpv (in-memory fake + opt-in real-mpv tests)
  media/                lofty tags (read + write, lyrics incl. SYLT / SYNCEDLYRICS, several
                        artists), embedded/folder artwork, thumbnails, square album covers,
                        color palette (fixtures in tests/)
  library/              SQLite library: schema + migrations, tracks/albums/artists, FTS5
                        search, playlists, history, session, incremental folder scanner,
                        links as tracks (remote.rs), saved lyrics + offsets (lyrics.rs),
                        several artists per track and credit splitting (artists.rs),
                        covers the user chose (covers.rs), measured loudness (loudness.rs)
  lyrics/               LRC / enhanced LRC parser and writer, SYLT conversion, .lrc files
                        and their encodings, LRCLIB URLs and result matching (pure)
  links/                URL classification, yt-dlp -J parsing (albums included), file name
                        and folder templates, progress lines, download state machine,
                        YtDlp runner and error classes (fixtures + opt-in real tests in
                        tests/)
  themes/               .vitratheme packages: safe reading (sizes, names), writing, installing;
                        examples/pack.rs packs a theme folder; tests/ packs the examples
  import/               playlist import (pure): model, normalize + matching, pasted text,
                        tables (CSV / TSV), files (M3U, XSPF, the Music app's XML), service
                        links and their pages (fixtures in tests/)
scripts/
  fetch-sidecars.mjs    downloader (I/O)
  record-ytdlp-fixtures.sh  real yt-dlp recordings for crates/links (needs YouTube)
  record-import-fixtures.sh real service pages for crates/import (needs the services)
  sidecars/plan.mjs     what to download per target (pure, unit-tested)
  theme-schema.ts       writes docs/theme.schema.json from the zod schema (`npm run theme-schema`)
assets/brand/           logo, app icon source, mono tray icon, palette (see its README)
docs/                   PLAN.md, THEMES.md (theme format), theme.schema.json (generated)
themes/                 example themes (sources: theme.json, custom.css, assets/)
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
     an orphan until a Job Object is added (stage 11).
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
   - The backdrop moves (`components/Backdrop`): the cover breathes, three copies of it drift
     and turn over each other (the third a soft round patch, `mask-image`), the theme's blobs
     drift on top. The turning copies are squares of 200vmax around the middle, so no
     corner of theirs reaches the screen at any aspect (a copy the size of the window
     showed a diagonal edge on wide screens). The 96 px thumbnail comes blurred and saturated from Rust
     (`media::artwork::backdrop_image`), so no moving layer has a CSS `filter`, and only
     `transform`/`opacity` animate. `data-backdrop-animation='off'` pauses it all.
   - Pulse (`appearance.backdropPulse`, on by default): while it is on, mpv's audio chain
     has `@vitralevel:lavfi=graph=[astats=…RMS_level]:o=[threads=1]` (`player/level.rs`;
     added when mpv starts, never touched for visibility changes, which could click). While
     a window wants it (`player_meters` from `useLevelMeter`: setting on, motion on, window
     visible) the
     player reads `af-metadata/vitralevel` every 70 ms and emits `player://level` (0..1: a
     smoothed loudness plus a kick when it rises above the recent average). `Backdrop` sets
     `--vitra-pulse` on its own element (on `<html>` it would restyle the whole window
     a dozen times a second): a slight scale and a glow from the middle.
   - WebKitGTK under Xvfb composites on the CPU: any moving backdrop under the glass (and
     the progress bar's repaints) cost a lot there; that says nothing about a Mac's GPU.
8. **Frontend**: React 19, TypeScript 6.0 (strict), Vite 8, CSS Modules. Every visual value is
   a CSS custom property (`--vitra-*`), so a theme is just a set of variables. Zustand for
   stores, Motion for animation, Lucide icons — each added in the stage that first needs it.
9. **Multiple windows** = multiple Vite HTML entries (`index.html`, `mini.html`).
10. **Security**: strict CSP in production (`script-src 'self'`), no remote scripts. Themes are
    data (JSON + CSS + assets), never JavaScript; `custom.css` is sanitized on import and
    when applied (key decision 30).
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
14. **i18n from stage 1** (i18next, en + ru) so strings are never hard-coded; the release stage
    (11) completes and reviews translations. (The spec lists i18n in its stage 9; doing it early
    avoids a rewrite.)
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
      so each write goes through its own temporary file (`<id>.<pid>-<n>.part`). They come
      from the album's own files (embedded art in the first three, else the folder's
      `cover.jpg` & co.) and only then from downloaded links, whose art is a site's thumbnail.
    - Single tracks: files opened or dropped outside library folders join the library with
      `folder_id = NULL` (setting `library.addOpenedFiles`, on by default; done in the
      background after playback starts, then the queue entries get their track ids). Adding a
      folder that contains them takes them in (same rows, so favorites and history stay).
      Only single tracks can be removed from the library (`library_remove_tracks`); tracks in
      folders would come back with the next scan.
    - One scanner thread with its own connection; commands share another (`LibraryService`).
      `notify` watches folders; changes rescan the folder 3 s after things settle.
    - Search: FTS5 `unicode61 remove_diacritics 2`, every word a prefix (`"word"*`), ranked
      title > artist > album > album artist > lyrics. `remove_diacritics` doesn't fold
      ё → е (they are separate letters to Unicode), so `search::fold` does it for the index
      and for queries (migration 3 refolded existing rows).
    - Events: `library://changed` (`tracks` | `playlists` | `folders`) and `library://scan`;
      views reload through `useLibraryData`, which coalesces bursts.
    - Several artists (`library/artists.rs`, migration 4): `track_artists(track_id,
artist_id, position)`; `tracks.artist_id` stays the first (main) one, and an album
      belongs to its album artist or the first track artist. Lists from tags (several
      `ARTIST`, `ARTISTS`, `TPE1` with `\0`) or yt-dlp (`artists`/`creators`) are used as
      they are; a single credit is split on `feat.`/`ft.`/`featuring`/`, `/`&`/`x`/`×`/`; `
      unless a whole name is an exception (`BUILTIN_EXCEPTIONS` + `library.artistSplitting`).
      The split credit is kept in `tracks.artist_credit` so changing the setting re-splits
      without rereading files (`set_artist_splitting`); migration 4 also re-reads folder
      tracks (`file_mtime = 0`) and re-splits once (`artists.resplitPending`). The scanner's
      connection reads the setting from the `session` table (`artistSplitting`). Artists are
      searched through their own FTS table (`artist_search`). An artist's page: their albums,
      then "Appears On" (`TrackFilter::ArtistAppearances`).
    - Tags are written with each artist as a separate value; MP4 has no multi-value `©ART`
      in common players, so it gets "A, B" plus `----:com.apple.iTunes:ARTISTS` items.
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
      and radio (`.m3u`/`.pls`/HLS) skip yt-dlp. No HTTP client of our own. Thumbnails go to
      a scratch folder by `-o thumbnail:` **and** `-o pl_thumbnail:` (a playlist's own
      thumbnail, which otherwise lands in the working folder), and resolve/enrich run with
      that folder as their working directory.
    - A link is a library track whose path is its page URL (`tracks.source_url`, migration 2);
      its thumbnail is the track's own cover (`tracks.cover`, shown before the album's
      unless the user corrected the link into an album).
      Listings (`complete: false`) never overwrite what a full description stored. Links
      join the library when played if `library.addOpenedFiles` is on; always when imported
      or downloaded.
    - Imported playlists keep `playlists.source_url`; importing again replaces their entries
      and keeps the name. Entries are filled in afterwards by `yt-dlp -j` (one line each).
    - Albums: a playlist whose id starts with `OLAK5uy_`, a `music.youtube.com/browse/MPREb…`
      page or a Bandcamp album (`info::album_of`). The title loses YouTube's "Album - ", the
      artist comes from `album_artists`, the uploader without " - Topic", or the tracks'
      usual artist. Entries are numbered by their place in the listing (private videos keep
      their number) and stored as one library album with one square cover
      (`media::square_cover`, `Library::set_album_artwork`); album tracks get no covers of
      their own. Enrichment keeps the placement (`Placement`), so `yt-dlp -j` can't move a
      track to another album. Tracks the library had before (played or downloaded on their
      own) are placed too (`remote::place_known`): a listing files a stream into the album
      with its number, and numbers a downloaded file whose tags name the same album; edited
      links stay as they are. The album keeps its link (`albums.source_url`, migration 7):
      opening the link (even just its preview) places the tracks the library has
      (`Library::place_album_listing`, never adds new ones), and opening the album's page
      re-reads the listing once per session when tracks lack numbers
      (`LinkService::renumber_album_later`).
    - Corrections: the tag editor edits a streamed link's title, artists, album, album artist
      and year in the library (`Library::edit_link`, `TagForm.link`); the album is found by
      title + album artist (or made), like a file's. `tracks.edited` (migration 6) makes
      `store_remote` keep them against every later description (opening the link, playlist
      enrichment, album placement). The link dialog and playback take a known link's
      library details; downloads take the track's album values before the group's. A
      downloaded link is a file: the editor writes its tags.
    - A corrected link in an album shows the album's cover, not its thumbnail (the
      `track_cover!` SQL rule in `tracks.rs`; the player's `link_thumbnail` follows it); an
      album without a cover gets the link's thumbnail cut square (the full one if
      `LinkService` still has it, else the cached copy). "Restore Site Details" in the
      editor (`TagForm.edited`, `link_restore` → `LinkService::restore`): yt-dlp describes
      the link afresh and `Library::restore_link` clears `edited` and stores that.
    - An album's listing often has no thumbnail: then the first entry thumbnail enrichment
      brings covers it (`Library::store_enriched` → `cover_bare_album`). Opening an album
      page whose album still has no cover (`library_album`) looks for one in the background
      once per session (`LinkService::cover_album_later`: a recent thumbnail, else the first
      entry's from `yt-dlp`), which also repairs albums stored before this existed.
    - Errors: `ytdlp::failure` turns yt-dlp's stderr into `AgeRestricted`, `SignInRequired`
      or `Cookies`; the UI shows translated text for them (`features/links/errors.ts`).
      `downloads.cookiesFromBrowser` (safari/chrome/firefox/…, `COOKIE_BROWSERS`) adds
      `--cookies-from-browser` to every yt-dlp run and to mpv's hook (`ytdl-raw-options`
      is set as a JSON map after each mpv start and when the setting changes).
    - mpv plays links through its own yt-dlp hook: `--script-opts-append=ytdl_hook-ytdl_path=`
      and `--ytdl-raw-options-append=js-runtimes=deno:` point it at our binaries. A failed
      link's `end-file` only says "unrecognized file format", so the player asks mpv for its
      log (`request_log_messages warn`), keeps the `ytdl_hook` lines of the current link and
      reads them with `ytdlp::failure`: cookies, age limits and sign-ins stop playback with
      that error (`PlayerError::Playback` carrying the `LinkError` text, which the UI
      translates; the control bar shows it too), since the next links would fail the same
      way; other failures are skipped as before. `Mirror::halted` keeps the preloaded file
      mpv starts meanwhile from clearing the error. Settings warn up front when Safari's
      cookie file can't be opened (`links/cookies.rs`; macOS needs Full Disk Access). Radio
      playlists end in `end-file reason=redirect`, after which the next `start-file` is the
      same queue item; `metadata` is observed for `icy-title`.
    - Downloads: `-f bestaudio/best -x` (+ `--audio-format mp3|flac`), `--embed-metadata`,
      `--embed-thumbnail`, `--convert-thumbnails jpg`, `--no-mtime`; progress via
      `--progress-template` (download progress on stdout, post-processing on **stderr**) and
      `--print after_move:`; partial files in `<cache>/downloads/<job>`. Then Vitra writes its
      own title/artist/album/year and saved lyrics into the file and
      `Library::attach_download` swaps the stream's path for the file (same row; a row the
      watcher added first is merged in).
    - Album downloads are a group (`downloads.rs`: `GroupInfo` on every job, cancel/retry
      the group): files go to `downloads.albumTemplate` (`{album_artist}/{album}/{track}
{title}`, `/` makes folders; `links::relative_path`), get album, album artist, track
      number/total and year, and all get the same cover (the album's, else the first
      track's thumbnail). Tags are re-read from the library when a job starts, so
      enrichment that finished meanwhile is used. "Download All" on a playlist is a group
      without album tags. An album page or tile of the library downloads its streamed
      tracks as such a group too (`downloads_add_album`, group url `vitra:album/<id>`;
      `Album.linkCount` says whether there is anything to download); its cover comes from
      the first track's thumbnail or the art yt-dlp embeds, never the user's own cover.
    - "Update yt-dlp": on macOS `links::release::install_zip_release` downloads the latest
      `yt-dlp_macos.zip` and `SHA2-256SUMS` with `curl`, checks the hash (`sha2`), unpacks
      with `ditto` and swaps the folder into `<app data>/sidecars/yt-dlp/` (a
      `.release-sha256` marker skips the same release). Elsewhere it copies a bundled
      yt-dlp to `<app data>/sidecars/` (found first from then on) and runs `-U` there; a
      system yt-dlp is updated in place (pip/Homebrew ones refuse).
    - While mpv opens a link (its hook waits for yt-dlp) the status is `loading`: the UI
      shows a spinner and disables seeking, and `Player::seek` defers to `file-loaded`.
23. **Lyrics** (`crates/lyrics`, `src-tauri/src/lyrics`, `features/lyrics`):
    - One model (`Lyrics { lines, synced, offset_ms, tags }`, lines with optional word
      timings and a translation) for tags, `.lrc`, search and the editor. A line repeating the
      previous line's timestamp is its translation; `[offset]` follows LRC (positive =
      sooner) and is folded into the timestamps (`normalized()`) before anything uses them.
      Timestamps are written in hundredths.
    - Sources, best first (`lyrics/sources.rs`): lyrics the user saved in the app (an
      explicit choice made after seeing what the file has) → synced tags (`SYLT`, or LRC in
      `SYNCEDLYRICS`/`LYRICS`/`USLT`/`©lyr`) → `.lrc` next to the file (UTF-8/16,
      Windows-1251 or Latin-1) → plain tags → lyrics cached from LRCLIB. The spec lists the
      app DB after the tags; user lyrics come first so "save to Vitra only" isn't hidden by
      a file. lofty's generic tag doesn't show `SYLT` or `SYNCEDLYRICS`, so they're read from
      the concrete tags and removed whenever new lyrics are written.
    - The editor saves LRC text into the format's lyrics field (`USLT`, `LYRICS`, `©lyr`),
      not `SYLT`; saving to tags or `.lrc` drops the user's DB copy so the file wins. A copy
      of the file goes to `<app data>/backups/tags|lrc/` first (newest 10 kept).
    - LRCLIB (setting `lyrics.online`, off by default): curl with a User-Agent naming Vitra,
      `/api/get` (title, artist, album, duration ±2 s) then `/api/search`, URLs percent-
      encoded to ASCII (Windows passes non-ASCII arguments in the ANSI code page). Results are
      cached in the DB (`source = 'lrclib'`), misses in `lyrics_lookups` for 30 days. 429 and
      5xx answers (and curl failures) are retried after 1.5 s and 4 s and never cached as
      misses (`online::fetch_patiently`).
    - The user's sync offset is per track (`tracks.lyrics_offset`, positive = later) or per
      session for files outside the library. `lyrics://changed` carries the whole state;
      `lyrics://line` fires when the playing line changes (for the mini player and plugins:
      the view keeps its own frame clock for karaoke).
    - The view: the playing line a third of the way down, smooth scroll and per-line blur
      only with motion on, scrolling by hand pauses following for 3 s. Tap-sync captures
      Space/Backspace/arrows before the player's shortcuts while the edited track plays.
    - `lyrics.visible` (a setting, so every window follows) hides them in Now Playing, the
      side panel (the panel stays "open" in `ui.sidePanel` and comes back with them) and the
      mini player's lyrics mode. The lyrics toolbar and the mini player hide them; the
      control bar's microphone brings them back in the panel.
24. **Context menus** are native (`@tauri-apps/api/menu`, `lib/menu.ts`); the WebView's own
    menu is suppressed outside text fields. Reordering uses a small pointer-based hook
    (`lib/reorder.ts`) because HTML5 drag and drop clashes with the window's file drop.
25. License **GPL-3.0-or-later** (compatible with mpv). Bundle identifier
    `io.github.fil1825.vitra`. macOS minimum **14.0** (official mpv builds require it).
26. **Sidebar** (`layout/state.ts`): `expanded` / `rail` (icons, labels as tooltips, playlists
    as monograms) / `hidden`, switched by the title bar button or ⌥⌘S / Ctrl+Alt+S. It is the
    layout's sidebar module (hidden = not shown, `layout.sidebar` = full or rail); the
    `localStorage` values of stage 5 move into the layout once (`adoptOldPanels`). On macOS
    the rail is 88 px so the traffic lights stay on it; whatever else sits in the window's
    top-left corner leaves 84 px for them (`Workspace`, `data-lights`). ⌘F opens the full
    sidebar with the search field focused.
27. **Full-screen artwork** (`components/ArtworkViewer`): shows the cached image at once,
    then the original from `artwork_original` (track path or album id; links use their
    thumbnail or album cover), served unscaled by `ArtworkStore::insert_original`.
28. **Covers the user chooses** (migration 5, `library/covers.rs`, `features/library/
covers.ts`): `albums.custom_cover`, `artists.cover`, `playlists.cover`, apart from the
    automatic `albums.cover` that scans and links write, so nothing automatic replaces them.
    Every query puts them first (`COALESCE(custom, automatic)`, plus a `customCover` flag
    for "Reset"); tracks of such an album show it too (rows, queue, Now Playing: the player
    reads it before the file's artwork). Images (jpg/png/webp, ≤ 50 MB) are cut square and
    kept up to 1200 px as JPEG in `covers/` under an id salted with "custom:" (never the
    512 px copy of the same image). Only the library keeps them; tags are the tag editor's.
    UI: `HeroCover` on album/artist/playlist pages (button on hover, context menu, an image
    file dropped on it: `useFileDrop` finds the `[data-cover-drop]` element under Tauri's
    drop position), plus entries in the album, artist and playlist menus. The sidebar shows a playlist's chosen cover in place of its icon.
29. **Mini player** (`src-tauri/src/mini`, `mini.html`, `src/windows/mini`, settings
    `miniPlayer`):
    - A borderless, transparent, `skipTaskbar` window labelled `mini` (`tauri.conf.json`,
      `create: false`, own capability `capabilities/mini.json`), created on first use and then
      only shown and hidden. Its page boots the settings, environment, player, queue and lyrics
      stores and the theme like the main window; it adds nothing to Rust's state.
    - Opening it hides the main window (`hideMainWindow`); expand, ⌥⌘M (`mini_toggle`),
      closing it or the Dock icon (`RunEvent::Reopen`) bring the main window back. Closing
      the main window hides it (the music goes on) while the mini player shows, always on
      macOS, and elsewhere while the tray icon is shown (`main_close_requested`); otherwise
      the hidden mini window is destroyed so the app quits. `mini-player.json` (config
      dir, not the settings: moving shouldn't notify every window) keeps `open` (reopen at
      launch), the position and each mode's size.
    - Sizes: a base size per mode × the preset (0.8 / 1 / 1.3), min/max per mode, dragged
      sizes remembered per mode; a preset change forgets them. The page sizes everything in
      `em` of a font that follows the window height (`100vh / --_rows`), or its width when
      the window is narrow for that (`100vw / --_cols`), so the text always has room. The
      compact artwork is `min(100vh − padding, 40vw)`; below an aspect of 11:5 the progress
      bar moves under it (`useMediaQuery`, re-rendered rather than a grid, see Gotchas).
      A new window reports its old size for a moment: positions are computed from the
      size just set.
    - Moves and resizes settle (250 ms, mouse button up where the platform tells) before
      snapping to the work area's edges (16 pt), saving, or pulling a grown window back on
      screen (`geometry::snap`, `fit_inside`, `reachable`).
    - Backdrop per window (`window::Backdrop`): rounded (`mini::RADIUS`, the page's
      `--glass-radius` matches; Windows 11 rounds the window itself, 8 px, through DWM's
      corner preference) and faded by `opacity`. The window's content is clipped to the same
      corners and its shadow redrawn after showing and resizing
      (`platform::round_window`: the content view's layer `cornerRadius` + `masksToBounds`,
      `invalidateShadow`): otherwise the WebView's square corners and the shadow of the
      square window showed as dark strips and corners at the edges. Fading: macOS sets the
      vibrancy view's alpha (found by window-vibrancy's view tag; Liquid Glass hosts the
      page, so a faded backdrop is always vibrancy), Windows drops Acrylic below 100% and
      the page's tint fades, Linux windows are opaque.
    - Always on top: `set_always_on_top` + all workspaces, and on macOS
      `FullScreenAuxiliary`. Over other apps' full-screen spaces only accessory apps are
      allowed, so `overFullScreen` switches the activation policy to `Accessory` while the
      mini player is the only window (`update_activation_policy`).
    - Click-through: `set_ignore_cursor_events`, except while ⌥/Alt is held over it: a loop
      (80 ms, only while it is on and shown) reads the pointer and `NSEvent.modifierFlags` /
      `GetAsyncKeyState` and emits `mini://interactive` (bus `mini:interactive`, an accent
      ring). Linux can't read the key: turned off in Settings there.
30. **User themes** (`crates/themes`, `src-tauri/src/themes.rs`, `theme/user.ts`,
    `features/themes`, `docs/THEMES.md`):
    - A `.vitratheme` is a zip of `theme.json`, `custom.css` and `assets/` (pictures without
      SVG, fonts). Rust reads it with every size capped as it is read, takes only well-named
      files (a path that could escape refuses the package), and installs it unpacked into
      `<app data>/themes/<id>/` through a staging folder. The frontend checks `theme.json`
      against the zod schema and cleans `custom.css` (`theme/css.ts`: `url()` only for the
      theme's assets, no `@import`/`image-set`/script hooks; a linear scan, strings set
      aside first) before installing and again when applying it.
    - Import is two steps (`theme_open` keeps the read package under a token, the frontend
      decides, `theme_install` writes it): a built-in id gets another, an installed one is
      replaced or kept beside; a theme with a layout asks before changing the window.
    - Assets reach the page through `theme://localhost/<id>/<name>` (`http://theme.localhost`
      on Windows), only for installed themes' assets; CSP allows it in `img-src`/`font-src`.
      Fonts become `@font-face` rules and the cleaned CSS a `<style id="vitra-theme-css">`.
    - The editor (`ThemeEditor`, a panel over the window in Settings' place) edits a draft
      (`stores/themeEditor.ts`) that `useAppearanceController` applies live; Save writes the
      installed theme, closing with changes asks. New Theme copies the theme in use.
    - Example themes are sources in `themes/` (fonts under OFL, cut to Latin + Cyrillic);
      `cargo run -p themes --example pack` makes packages; `crates/themes/tests/examples.rs`
      and `src/theme/examples.test.ts` keep them valid.
    - Stable hooks for `custom.css`: `data-part` on the modules (`sidebar`, `queue`, `lyrics`,
      `artwork`, `controls`, `level`), `center`, `header`, `main`, `lyrics-text`, `settings`,
      `mini`, `backdrop`; `--glass-radius` set on a part rounds its glass. Class names are
      hashed and never part of the theme API.
31. **Layout** (`layout/`, `src-tauri/src/layout.rs`, settings `layout` + `layoutPresets`):
    - Zones and panels (the owner's choice, not a free grid): modules in the left and right
      columns (sidebar, queue, lyrics, artwork, level) and the top and bottom bars
      (controls, level) around the main view. `FITS` says where each may go; `hidden`
      modules keep their place. Column widths 180–560 px, a module's share of its column
      0.25–4 (`weights`). Both sides normalize the same way (`normalizeLayout`,
      `Layout: From<Value>`): every module placed exactly once, misplaced ones go home.
    - `Workspace` is nested flex boxes (see Gotchas on WebKit grids). Columns resize at their
      inner edge, modules at the line between them; changes show live and save on release.
    - Customize mode (title bar button, Settings → Layout, a shortcut): dnd-kit drags a module
      by its frame (`useSortable`) or a hidden one from the palette (`useDraggable`); the drop
      target is the module under the pointer, else the zone (`dropTarget`: nearest centres
      mean nothing between zones this different in size); the half dropped on decides
      before/after. Built-in presets (`presets.ts`) and saved ones (≤ 20, by name).
    - The queue and lyrics buttons show and hide their modules wherever they are; hidden
      lyrics (`lyrics.visible`) take their module along.
    - The level meter module is a spectrum (`player/spectrum.rs`): twenty bands, 40 Hz to
      16 kHz on a log scale, measured in mpv's chain like the pulse, so links work too. lavfi
      filters that merge branches (`amerge`, `amix`, `join`) drop the side branch's metadata,
      so the analysis rides in the audio: `pan` adds one mono channel per band (named
      channels, since `bandpass … c=` picks by name), each gets its band-pass, `astats`
      measures every channel and a last `pan` keeps the two music channels bit for bit.
      `o=[threads=1]`: lavfi's threading cost ten times the filtering. The filter is in the
      chain while the meter is in the layout (`Settings::audio_meters`); readings
      (`af-metadata/vitraspectrum`, every 33 ms while playing) wait 330 ms in a `DelayLine`
      because mpv filters that far ahead of what is heard, and go out as `player://spectrum`
      (0..1 per band, high bands lifted 1.5 dB/octave; empty when nothing plays).
    - The meter draws on a canvas every frame (`LevelMeter.tsx`): bars resampled to the width
      (one per 12 px, 8–32), fast rise and slow fall with peak caps that hold and drop
      (`spectrum.ts`, `Dynamics`), one fill with the glow for all bars. Styles `bars`,
      `mirror`, `wave` (`meter.style`, its context menu); a quiet wave when no reading came
      for 900 ms; with Reduce Motion a few frames a second, no caps, still when idle. The
      canvas sits absolutely in its box and 8 px into the padding, so its pixel size never
      feeds back into the layout and the glow isn't cut off.
32. **Keyboard shortcuts** (`shortcuts/`, `src-tauri/src/shortcuts.rs`, settings `shortcuts`):
    - Actions (`actions.ts`) with a default combination per platform; the settings keep only
      the user's changes (`id → combination | null`), so new actions get their defaults.
      Combinations are `Mod+Ctrl+Alt+Shift+<KeyboardEvent.code>` (`Mod` = ⌘ on macOS, Ctrl
      elsewhere; `Ctrl` only on macOS), so they don't depend on the keyboard layout or on ⌥
      changing letters. Rebinding takes the combination from the action that had it and
      says so; the system's and text editing's combinations are refused (`isReserved`).
    - One `keydown` listener per window (`useShortcuts('main' | 'mini')`; the mini player runs
      the playback actions and its own toggle). Plain keys don't act in focused controls or
      text fields, and editing chords (arrows, ⌘A/C/V/X/Z) not in text fields (`firesIn`); a
      key a component already handled (`defaultPrevented`) is left alone; held keys repeat
      only seeking and volume. The recorder in Settings captures keys first and pauses the
      listener.
    - Shortcuts in every app: key decision 37.
33. **Covers and records** (`components/CoverArt`, `theme/artwork.ts`, theme `artwork`,
    settings `appearance.vinyl`):
    - The theme's `artwork.shape` sets `--vitra-artwork-radius` (`square` = `initial`, so
      each place keeps its own corners from `--cover-radius`; `rounded` 22%, `circle` 50%,
      shares of the cover so every size matches). Artists stay round.
    - Records (`artwork.vinyl`, or the user's `appearance.vinyl` on/off over the theme's
      `auto`; the disc button on the big cover sets it): every cover is round
      (`html[data-vinyl='on']`), and the playing track's covers (`CoverArt live`: Now
      Playing, the artwork module, the control bar, the mini player, the queue's current
      row) get grooves, a sheen and a label and turn at 33⅓ rpm while it plays.
    - One spin clock per window (`spin.ts`): a shared angle so every record shows the same
      turn, eased up (0.8 s) and down (1.4 s) when playback starts and stops, kept on
      pause; driven by `requestAnimationFrame` only while something turns and written as a
      `transform` (never `filter`/`opacity`, see Gotchas). With Reduce Motion no cover
      joins it.
    - No cover: the mono Vitra symbol (`assets/brand/mono.svg`, inlined with
      `currentColor`) in `--vitra-artwork-placeholder` (`artwork.placeholderColor`), else
      the text color faded.
34. **Vitra Sans** (`styles/global.css`, `src/assets/fonts/`): the built-in themes' font
    stack starts with it. Two `@font-face` rules under one name split by `unicode-range`:
    Outfit (variable, Latin) and Onest (variable, Cyrillic, which Outfit lacks) at
    `size-adjust: 92%`, so lowercase heights meet and mixed lines read evenly. Both SIL OFL,
    subset with their license names kept; bundled by Vite (CSP `font-src 'self'`), never
    fetched. Themes still name their own fonts first.
35. **Now Playing and media keys** (`src-tauri/src/now_playing`):
    - The player tells `NowPlaying` its state on every state and position emit; the pure
      `Publisher` passes on only what changed (the track, or the playback when it starts,
      stops, changes rate or jumps more than 1 s from where the system extrapolates it).
      MPRIS and SMTC don't move the position along, so they get it every second while
      playing. mpv's own media controls stay off (key decision 5).
    - macOS: `MPNowPlayingInfoCenter` (a dictionary kept on the main thread; title, artist,
      album, duration, elapsed time, rate, `playbackState`, artwork from the artwork store
      as an `NSImage`) and `MPRemoteCommandCenter` handlers (play, pause, toggle, stop,
      next, previous, change position). Windows / Linux: `souvlaki` (SMTC needs the main
      window's HWND; MPRIS through libdbus, bus name `vitra`), covers as files in
      `<cache>/now-playing/` for their URLs; Raise shows the main window.
36. **Sound** (`src-tauri/src/sound`, `player/crossfade.rs`, `features/settings/SoundSettings`,
    settings `sound`):
    - Equalizer: one filter `@vitraeq` (a lavfi graph of `equalizer@b0` … `@b9`, 32 Hz to
      16 kHz an octave wide, and `volume@pre`) before the meters. mpv rebuilds its lavfi
      graph for every file and seek, so `af-command` only previews a band being dragged
      (`sound_preview_equalizer`); a kept change replaces the filter in place (`af add`
      with the same label). Presets: built-in ids (`features/sound/presets.ts`) or
      `user:<name>` from `sound.presets`.
    - Normalization (`off | track | album`): mpv's `replaygain` with a +4 dB preamp (from
      ReplayGain's −18 LUFS to Vitra's −14) and `replaygain-clip`; files without tags get
      their measured gain as the per-file `replaygain-fallback` option of `loadfile` (mpv
      uses it exactly when there are no tags; set at runtime it applies from the next file,
      the UI says so). Files are measured once by the FFmpeg sidecar (`ebur128=peak=true`,
      `LoudnessMeter`, newest wishes first), links while they play by the
      `@vitraloud:lavfi=[ebur128=metadata=1]` filter (momentary readings integrated with
      R128's gates, `Integrator`); results go to the library's `loudness` table (migration
      8, keyed by path + size + mtime). Album mode averages the album's tracks' energy.
      Boosts stop at the measured peak (−1 dBTP) and +12 dB, unknown peaks at +3 dB.
    - Crossfade (`sound.crossfade`, 0–12 s, 0 = gapless as before): a second mpv, the
      _standby deck_ (`Decks`, started only while crossfade is on), loads the next track
      paused at volume 0. `crossfade_tick` starts it `C` s before the end (when the track is
      at least `2C + 1` s long); it becomes the active deck at once (queue, Now Playing,
      events follow) and both volumes cross equal-power on one wall clock (30 ms steps,
      mpv's cubic volume, paused time doesn't count). A seek or another track ends a fade
      at once; pause pauses both decks. Same-album neighbours (`same_album`: album name and
      artist, folder or both links), the same item again and radio stay gapless on one
      deck. Volume, mute, speed, EQ and normalization go to both decks; the standby deck
      prepares again whenever it (re)starts. Only the active deck's events drive the state.
37. **Menu-bar icon and shortcuts in every app** (`tray.rs`, `global_shortcuts.rs`,
    settings `system.trayIcon`, `globalShortcuts`):
    - The tray icon is built at setup on the main thread (a Linux desktop without the
      AppIndicator library panics inside tray-icon: caught, the icon is then unavailable
      and Settings says so) and only shown or hidden afterwards. macOS gets
      `icons/tray-template.png` (mono.svg, a template image; see `assets/brand/README.md`),
      the others the app icon; Windows opens the window on a left click. Its menu ids start
      with `tray:` (every menu event reaches every handler); the main window sends the
      words (`tray_labels`), Rust follows the player (`Player::emit_state`).
    - Shortcuts in every app: `globalShortcuts` (action id → combination, none by default)
      for playback and the mini player; Rust converts our combinations (`Mod` = ⌘ / Ctrl)
      to the plugin's, refuses plain keys that type (function and media keys are fine),
      registers them and runs the actions itself. A combination another app holds is
      reported (`global_shortcuts_failed`). Taking a combination in every app takes it from
      a window shortcut of another action too (the system would catch it first), and the
      other way round. On Linux they need X11 (XWayland apps on Wayland).
38. **Settings pages** (`features/settings`): a panel over the window with a list of pages
    (`sections.ts`: icon, title, the keys of what's on them) and a search that matches every
    word in a page's title or contents (lower case, ё = е); below 600 px the list keeps
    only icons. `ui.settingsSection` opens a page (`showSettings(true, 'shortcuts')`).
    About: version, the owner's note and donation link (`about.ts`, empty until written),
    source, license, credits, diagnostics.
39. **Icon motions** (`styles/icons.css`): every Lucide icon (`.lucide-<name>`) or part of it
    names a motion in the non-inheriting `--vi` / `--vi-time` / `--vi-delay` (`@property`);
    one rule plays it once when the button, link, tab or menu item around it is hovered;
    `:active` adds `scale: 0.9`; the play/pause icon (`data-morph`) turns into the other.
    Only `transform` moves; nothing with Reduce Motion; the spinner keeps its own spin.
    `styles/icons.test.ts` fails when an imported icon has no motion.
40. **Playlist import** (`crates/import`, `src-tauri/src/import`, `features/import`,
    migration 9):
    - Only the list comes over; the sound is the library's or a YouTube Music link. No
      accounts or API keys: links are read from what the services show visitors (Spotify's
      `open.spotify.com/embed/…` JSON, Apple Music's page data or JSON-LD, Deezer's open API
      page by page, Yandex Music's `handlers/*.jsx` and `api.music.yandex.net` share links)
      with curl (`http.rs`, a Safari User-Agent); YouTube / SoundCloud / Bandcamp lists go
      through `LinkService::import` as before; VK only as copied text. Readers are tested on
      hand-written fixtures in each service's shape (the cloud can't reach them) and on
      `*-recorded.*` pages when someone records them.
    - A playlist's places (`playlist_items`) are a track, an `import_entries` row (what the
      list says, `state`, `candidates` JSON) or both; positions count every place, so the
      page shows waiting entries as placeholder rows (`entries.ts`, negative ids) and moves
      and removals keep working. `PLAYLIST_SELECT` counts tracks only. Importing a source
      again (`playlists.source_url`; the Music app's is `music-app:playlist/<id>`) keeps
      entries by identity (their link, else folded artists + title) with their tracks and
      picks; a `partial` page (Spotify's first 100) removes nothing past it, and pasted
      tracks can complete such a playlist (`import_text … into`).
    - At import, entries match the library by path, then name and length (sure ones get the
      track at once). The matcher (one task) takes pending entries one at a time: bare links
      are described first (Spotify track pages, yt-dlp), then the library again, then a
      YouTube Music `#songs` search (`ytdlp::music_search_url`): flat results carry titles
      only, so the two closest are described in full (with their thumbnail, kept as the
      found track's cover). At most one request every 3 s, 60 s after failures. Scores
      (`matching.rs`): 0.5 title + 0.3 artist + 0.2 length, version mismatches −0.25 each,
      another artist −0.2, a different title gated; ≥ 0.82 sure, ≥ 0.55 doubtful ("?",
      alternatives kept), else missing (grey, Find…). Found links are stored under the
      list's names (`edited = 1`) with the link's own length; a link replaced by a pick goes
      if nothing else uses it.
    - Events: `import://progress` (bus `import:progress`) per playlist; entries changing emit
      `library://changed` (`playlists`, plus `tracks` when a track was added).
    - Yandex Music's private lists and «Мне нравится»: with `downloads.cookiesFromBrowser`
      set, yt-dlp exports that browser's cookies (`YtDlp::export_cookies`, a run against a
      refusing local address), only Yandex's are kept in a 0600 file, curl retries with
      them, the file is deleted.
    - The Music app (macOS): `objc2-itunes-library` lists playlists (not the library's
      sections or folders) for the dialog's Music tab and reads the chosen ones; macOS asks
      for Media & Apple Music once (`Info.plist`).
    - Ways in: "+" next to Playlists → Import Playlist…; pasting a service link or several
      lines outside text fields (`useLinkInput`); dropping playlist files or a service link
      (`useFileDrop`, kind `playlist`); service links typed into the link dialog.

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
- Tauri's drag-and-drop handler reports every drop as handled, so the page never gets HTML
  `drop` events, and it reads only file paths: a link dragged from a browser arrives as a
  `drop` without paths. `useFileDrop` then calls `drag_link` (`src-tauri/src/drag.rs`), which
  reads the macOS drag pasteboard (`public.url`, legacy `NSURLPboardType`, plain text).
  Windows and Linux report nothing for links; paste works there. Don't turn off
  `dragDropEnabled`: file and folder drops need its real paths.
- Under Xvfb there is no window manager: `startDragging` does nothing and keyboard focus may
  not be on a window (click before typing shortcuts). Move windows with `xdotool
windowmove` to test snapping, resize with `xdotool windowsize` to test layouts at every
  size; `xwininfo -root -tree` lists the mini window.
- WebKit layout traps (seen in the mini player): a percentage height of a flex item in a
  window-sized container didn't resolve on macOS, so an image took its own size (use `vh`
  / `vw`); a grid item spanning several rows spreads its height over them even next to
  `fr` rows, and an `auto` row whose item has `overflow: hidden` can collapse to nothing.
  Nested flex boxes are the safe choice.
- Tauri types drop positions as `PhysicalPosition`, but only Windows reports physical pixels
  (WebView2, `ScreenToClient`); macOS (`draggingLocation`, points) and Linux (GTK widget
  coordinates) report logical ones. `useFileDrop.dropPoint` divides by `devicePixelRatio`
  on Windows only.
- Typing into the real app under Xvfb with `xdotool type` lags far behind (WebKitGTK on the
  CPU): wait seconds before clicking elsewhere, or later keys land in the next field. GTK's
  file chooser completes paths as you type (`/t` → `/tmp/`): type `~/name` or short paths.
- lofty can't write `ItemKey::Lyrics` to ID3v2; use `ItemKey::UnsyncLyrics` (USLT) there.
  lofty also warns about every odd MP3 frame; the app logs `lofty` at error level only.
- The macOS mpv tarball carries code-signature xattrs; `fetch-sidecars` moves extracted files
  with `rename` (never copies) to keep them. Linux GNU tar prints harmless warnings about them.
