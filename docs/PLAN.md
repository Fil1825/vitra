# Vitra development plan

Requirements: [`SPEC.md`](../SPEC.md). Architecture decisions and commands: [`CLAUDE.md`](../CLAUDE.md).

Every stage ends with: `npm run check:all` green (cargo fmt/clippy/test, eslint, prettier, tsc,
vitest, vite build), a pull request describing what changed and **how to verify it on macOS**,
and an explicit list of checks that can only be done on a Mac.

## Stage 0 — Preparation

- Environment check: Rust 1.97, Node 22, Tauri CLI 2.12, Linux WebKitGTK deps (recorded in
  `CLAUDE.md`).
- Project skeleton: Cargo workspace + Tauri 2 app crate, React 19 + TS 6 + Vite 8 frontend,
  ESLint, Prettier, Vitest, original app icon, strict production CSP.
- `scripts/fetch-sidecars.mjs`: downloads mpv, yt-dlp, FFmpeg/ffprobe and Deno for the current
  (or a given) target with checksum verification; writes `manifest.json`.
- `src-tauri/src/sidecar.rs`: finds those binaries at runtime (app data → bundled → dev → PATH).
  The stage-0 window lists what it found.
- CI workflow running the same checks on every pull request (Linux).
- This plan and `CLAUDE.md`.

## Stage 1 — Shell

- Main window: transparent, custom title bar area (`titleBarStyle: Overlay` on macOS),
  `window-vibrancy` (macOS `NSVisualEffectView`, Windows 11 Mica/Acrylic, Linux: no vibrancy,
  translucent background fallback). Needs `macOSPrivateApi` for transparency on macOS.
- Theme token system: `--vitra-*` CSS variables for colors, glass, typography, radii, spacing,
  motion; Light, Dark, Auto (follows the OS) and **Aurora Night** built-in themes, applied at
  runtime. Aurora Night is the brand theme (palette in `assets/brand/README.md`): dark navy
  background, ice-blue accent, and the aurora gradient as the dynamic background when no
  artwork is playing.
- `<Glass>` component: `blur`, `saturation`, `tint`, `opacity`, `radius`, `border`,
  `highlight`, `refraction`; defaults from theme variables. Refraction through an SVG
  `feDisplacementMap` used in `backdrop-filter`, enabled only when feature detection proves it
  works (Chromium/WebView2); WebKit falls back to blur + specular highlight.
- Accessibility & performance switches: system `prefers-reduced-motion` and
  `prefers-reduced-transparency` plus in-app toggles; a budget for simultaneously active
  `backdrop-filter` layers; "power saving" mode disables heavy effects.
- Typed event bus (`src/lib/bus.ts`) and i18n setup (i18next, en + ru).
- Deps: `window-vibrancy`, `zustand`, `motion`, `lucide-react`, `i18next`, `react-i18next`.
- Tests: theme token resolution, bus, feature detection fallbacks.
- **Mac-only checks**: vibrancy, transparency, traffic-light placement.

## Stage 2 — Engine

- `crates/mpv-ipc`: async (tokio) JSON IPC client — commands with request ids, property
  observation, events; Unix socket / Windows named pipe.
- Player service in the app crate: spawns mpv (flags in `CLAUDE.md`), restarts it if it dies,
  exposes commands (open, play/pause, seek, volume, mute, speed) and emits
  `player://state` / `player://position` / `player://track-changed`.
- Open files via dialog and drag & drop (`tauri-plugin-dialog`).
- Bottom control bar (transport, seek bar, volume, speed), large artwork, track info.
- Artwork & metadata via `lofty`; dominant colors extracted in Rust; background = blurred
  artwork + slowly flowing gradient; accent color from artwork (can be disabled/fixed by theme).
- Tests: IPC protocol against a mock socket server; color extraction; an opt-in integration
  test with the real mpv (`VITRA_MPV_TESTS=1`).
- **Mac**: playback of FLAC/ALAC/MP3/Opus files, artwork background.

## Stage 3 — Library & queue

- SQLite schema with migrations (`rusqlite`, bundled, FTS5): tracks, albums, artists,
  playlists, playlist items, lyrics, play history, favorites, session.
- Folder scanning (`lofty` tags, parallel, incremental by mtime/size) and watching (`notify`).
- Views: albums grid, artists, all tracks table (virtualized), playlists, recently added.
- Instant search (FTS5) over title, artist, album and lyrics.
- Queue in Rust: play next, add to end, reorder (drag & drop), shuffle, repeat one/all,
  history. Gapless by mirroring the next item into mpv's internal playlist.
- Session restore: queue, position, volume.
- Basic tag editor (title, artist, album, year, cover).
- Tests: queue logic (shuffle/repeat/reorder invariants), scanner on fixture files,
  FTS queries, migrations.
- Done as planned, plus: favorites view, play history panel, moved/renamed files keep their
  history, native context menus, album artist in the tag editor. Not done here: drag and drop
  from the library onto sidebar playlists (context menu instead), multi-disc headers in
  albums, a column chooser for the songs table.
- **Mac**: native context menus and `ask`/`message` dialogs, FSEvents folder watching, cover
  grid scrolling performance on a large library, gapless transitions on real albums.

## Stage 4 — Links

- Paste/drop a URL: YouTube, YouTube Music, SoundCloud, direct audio, internet radio
  (m3u/pls/stream). Audio only (`--ytdl-format=bestaudio/best`, `--no-video`).
- Metadata via `yt-dlp -J` (title, uploader, thumbnail, duration); playlist URL → imported
  playlist (`--flat-playlist`).
- Downloads to the library: original stream (Opus/M4A, default) or MP3 (bitrate) / FLAC;
  `-x --embed-thumbnail --embed-metadata` + FFmpeg; lyrics from the DB written into tags;
  configurable folder and filename template (`Artist - Title`); queue with progress, cancel,
  retry; streamed track record replaced by the local file (playlists, lyrics, play counts kept).
- "Update yt-dlp" button: copies yt-dlp into the app data dir and runs `yt-dlp -U` there.
- Tests: `yt-dlp -J` parsing on recorded fixtures, filename templating, download state machine.
- Done as planned, plus: links are library tracks (path = page URL) so favorites, playlists and
  history work for them; a download takes over the stream's row; imported playlists fill in
  their entries' metadata and thumbnails in the background; radio shows the stream's
  `icy-title`; opt-in tests against the real yt-dlp (CI job `yt-dlp (real)`). Fixtures for
  YouTube and SoundCloud are hand-written from yt-dlp's documented fields (the cloud can't reach
  those sites); `scripts/record-ytdlp-fixtures.sh` records real ones. Not done here: reading
  `.webloc`/`.url` shortcut files, resuming downloads after a restart (the queue lives in
  memory).
- After the Mac review: macOS uses yt-dlp's **folder build** (`yt-dlp_macos.zip`), because
  the single-file build took ~25 s per call there (macOS re-checks the Python libraries it
  unpacks on every run). It can't update itself, so "Update" downloads the latest zip (curl +
  checksum + `ditto`) into the app data folder. yt-dlp is run once at start to get macOS's
  one-time check of a new copy over with. `npm run bundle` ships the fetched sidecars as
  resources. While a link opens the player shows a spinner and seeking waits for the file.
- **Mac**: real YouTube/SoundCloud playback and downloads (needs Deno for YouTube), the paste
  shortcut outside text fields (WebKit's `beforepaste`), dropping a link from Safari, the
  yt-dlp update (zip into the app data folder), `npm run bundle` producing an app whose
  `Contents/Resources/sidecars/` works (signatures of mpv.app and the yt-dlp folder).

## Stage 5 — Lyrics

- `crates/lyrics`: LRC + enhanced LRC (word timings) parser and serializer, offsets,
  translation track; thorough unit tests.
- Sources in priority order: synced tags (ID3 `SYLT`, Vorbis `LYRICS`/`SYNCEDLYRICS`, MP4
  `©lyr`) → sibling `.lrc` → unsynced tags (`USLT`/`LYRICS`) → app DB → LRCLIB (off by default,
  cached).
- Apple-Music-like view: smooth auto-scroll, highlighted current line, click to seek, karaoke
  word highlight, unsynced fallback, optional translation line, font size and sync offset
  adjustable live.
- Editor: paste/write, tap-to-sync with Space (undo line, shift all), save to tags (confirm +
  backup) / `.lrc` / DB only, import/export `.lrc`.

## Stage 6 — Mini player

- Separate window (`mini.html`), opened by button, tray or hotkey.
- Modes: compact (artwork + controls + progress), slim bar, lyrics-only (karaoke line).
- Settings: always on top / normal, opacity, size, snap to screen edges, remember position,
  optional click-through. Uses the current theme and `<Glass>`.
- **Mac-only checks**: always-on-top over full-screen apps, click-through.

## Stage 7 — Customization

- Theme editor with live preview: palette, glass sliders (blur, opacity, highlight,
  refraction), fonts, radii, background.
- `.vitratheme` = zip of `theme.json` + optional `custom.css` + assets; JSON Schema
  (`docs/theme.schema.json`) generated from zod; import by drag & drop or button, export with
  one click; no JavaScript allowed; CSS sanitized.
- Example themes in `/themes`.
- Modular layout with `@dnd-kit`: toggle, drag, resize modules (sidebar, library, queue,
  lyrics panel, big artwork, control bar, volume meter); presets, optionally part of a theme.
- Rebindable in-app hotkeys.
- Docs: `docs/THEMES.md`.

## Stage 8 — Sound & system

- 10-band EQ with presets and custom presets (`lavfi` `equalizer` chain, updated live).
- ReplayGain (track/album), volume normalization (`dynaudnorm`/`loudnorm`), crossfade.
- Media keys and Now Playing via `souvlaki` (macOS, Windows SMTC, Linux MPRIS).
- Tray / menu-bar icon (`assets/brand/mono.svg`, a template image on macOS) with basic controls; global hotkeys (`tauri-plugin-global-shortcut`).
- **Mac-only checks**: media keys, Now Playing widget, menu-bar icon.

## Stage 9 — Release

- Discord Rich Presence, Last.fm scrobbling (both optional).
- Complete en/ru translations.
- CI: builds for macOS (arm64 + x64), Windows x64, Linux on tag via `tauri-action`; sidecars
  fetched in CI; signing/notarization notes.
- `README.md` (+ screenshots/GIF), `README.ru.md`, `CONTRIBUTING.md`, issue/PR templates,
  disclaimer that users are responsible for complying with the terms of sites they play from.
- First release.

## Post-v1 roadmap

- Plugin API on top of the event bus.
- Smart playlists.

## Risks and how we handle them

| Risk                                                                                       | Plan                                                                                                                                          |
| ------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| mpv has no native crossfade                                                                | Crossfade with a second mpv instance and volume ramps (stage 8); gapless stays native. Crossfade and gapless are alternatives per transition. |
| Shipping `mpv.app` (dylibs) inside the bundle complicates signing/notarization             | Sign nested binaries in the release workflow (stage 9). Unsigned local builds work.                                                           |
| YouTube changes break yt-dlp                                                               | Updatable yt-dlp in app data + "Update yt-dlp" button; Deno for EJS challenges.                                                               |
| Installer size: static FFmpeg+ffprobe ≈ 330 MB (Win/Linux), Deno ≈ 100 MB, mpv ≈ 60–100 MB | See open question 1.                                                                                                                          |
| `backdrop-filter` with SVG filters only works in Chromium                                  | Feature detection + blur/highlight fallback (WebKit on macOS/Linux).                                                                          |
| Transparent windows on Linux need a compositor                                             | Detect and fall back to an opaque theme background.                                                                                           |
| A hard crash on Windows can orphan mpv (no fd-passing IPC there)                           | Assign mpv to a Job Object with kill-on-close (stage 9); macOS/Linux already tie mpv's life to the IPC socket.                                |
| A huge queue (tens of thousands of items) is sent whole on every change                    | Fine for normal libraries (≈ 2 MB at 10 000 items); send diffs or pages if it shows up in profiles.                                           |
| Linux inotify watch limits on very large libraries                                         | Watching degrades to manual rescans (logged); raise `fs.inotify.max_user_watches` if needed.                                                  |
| No GUI in the cloud environment                                                            | Logic in testable crates/modules; UI verified by the owner on macOS following PR instructions.                                                |

## Open questions

1. **Sidecar distribution.** Bundle everything in the installer (simple, works offline, but
   +500 MB on Windows/Linux), or bundle only mpv and download yt-dlp/FFmpeg/Deno into the app
   data dir the first time a link feature is used (with consent and progress)? The second
   option keeps the installer small and matches the "update yt-dlp" flow.
2. **macOS support range.** Official mpv builds need macOS 14+ (Apple Silicon) / 15+ (Intel).
   Is that acceptable, and is Intel support needed at all?
3. **Bundle identifier** `io.github.fil1825.vitra` — fine? It must not change after the first
   release (it determines the app data location).
4. **Workflow**: start each stage from `main` after the previous PR is merged, or stack stage
   branches on top of each other without waiting?
