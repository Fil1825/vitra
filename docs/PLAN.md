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
- Second Mac round: links dragged from a browser are read off the macOS drag pasteboard
  (Tauri's drop events only carry file paths and never reach the page); a macOS CI job runs
  `cargo test` and the real yt-dlp tests with the folder build. Windows and Linux don't
  expose dragged links to Tauri; paste works there.
- **Mac**: real YouTube/SoundCloud playback and downloads (needs Deno for YouTube), the paste
  shortcut outside text fields (WebKit's `beforepaste`), dropping a link from Safari (drag
  pasteboard) while files and folders from Finder still drop as before, the
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
- Done as planned, with these choices: lyrics the user saved in the app come before the
  file's (otherwise "save to Vitra only" would be hidden by plain lyrics in the tags); synced
  lyrics go into tags as LRC text in the lyrics field (`USLT`/`LYRICS`/`©lyr`), and stale
  `SYLT`/`SYNCEDLYRICS` are removed so they can't outrank it; tag backups go to
  `<app data>/backups/` (newest 10). Also: `.lrc` files in UTF-16, Windows-1251 or Latin-1;
  lyrics words are searchable (ё and е find each other); a lyrics panel next to the queue
  and lyrics beside the artwork in Now Playing; LRCLIB results cached, misses re-checked
  after 30 days; "Search LRCLIB" on demand even with online lookups off. Not done here:
  writing `SYLT` frames, word-level tap-sync (lines only; word timings from files are kept and
  move with their line).
- Round before stage 6 (owner's requests after the Mac review):
  - **Several artists per track** (`track_artists`, migration 4): lists from tags (several
    `ARTIST` values, `ARTISTS`, ID3v2.4 `TPE1` with `\0`) and from yt-dlp (`artists` /
    `creators`) are taken as they are; a single credit is split on feat./ft./&/,/x/; with
    an exception list ("Tyler, The Creator", user additions) and a setting to turn it off
    (changing it re-splits the library). An album belongs to its first artist; an artist's
    page lists the albums and an "Appears On" section; each artist in a track row is a link.
    Downloads write the artists as separate values (MP4: `©ART` "A, B" plus `ARTISTS`).
  - **YouTube / YouTube Music albums** (`OLAK5uy_…` playlists, `browse/MPREb…` pages): the
    link dialog shows the album (cover, artist, year, numbered tracks) with Listen and
    Download Album; the library gets one album with one cover; downloads go to
    `{album_artist}/{album}/{track} {title}` (a setting) with album, album artist, track
    number/total, year and the album cover in every file, grouped in the downloads view
    (overall progress, cancel/retry all). Playlists keep importing, plus "Download All".
  - Full-screen artwork (Now Playing, album page, control bar) at the original resolution.
  - Sidebar: expanded / icons only / hidden (button in the title bar, ⌥⌘S / Ctrl+Alt+S),
    remembered per window.
  - Readable errors for age-restricted and sign-in-only videos, and a "cookies from a browser"
    setting (`--cookies-from-browser`, also passed to mpv's hook). LRCLIB is asked again
    (after 1.5 s and 4 s) when it answers 429/5xx; such answers are never cached as misses.
- Second round before stage 6:
  - Fix: an album added from a link kept no cover when its listing had no thumbnail. The
    first entry thumbnail that enrichment brings now covers it, and opening the album's
    page looks for a cover for albums stored without one (once per session).
  - Covers the user chooses for albums, artists and playlists (migration 5): a button on
    the page's cover, the context menus, or an image file dropped on the cover; "Reset"
    goes back to the automatic one. They come first everywhere (grids, search, sidebar,
    track rows, queue, Now Playing) and stay in Vitra's library (not written to tags).
- Third round (Mac check of the second): playlist thumbnails no longer land in the app's
  working folder (`pl_thumbnail:`); a link that fails on cookies, an age limit or a sign-in
  stops playback with a readable error (read from mpv's yt-dlp hook log) instead of being
  skipped silently, and Settings warn when Safari's cookies can't be read; dropping an image
  on a cover works on Retina (macOS drop positions are logical); "Download Album" on album
  pages and tiles; the fixture script finds the macOS folder build, writes recordings
  next to the hand-written fixtures and never truncates one when yt-dlp fails.
- **Mac** for round 3: the traffic lights over the sidebar's icon strip and next to the
  title bar with the sidebar hidden, cookies from Safari (needs Full Disk Access) and Chrome
  (Keychain prompt), how Apple Music / Finder show downloaded files with several artists,
  real album links and `scripts/record-ytdlp-fixtures.sh` (records album fixtures too).
- Fourth round:
  - Link details can be corrected ("Edit Tags" on a link: title, several artists, album,
    album artist, year). They live in the library (migration 6, `tracks.edited`), so
    opening the link again, refreshing its playlist or album never overwrite them; an
    album name moves the track into that album or makes a new one; the link dialog and
    playback show them; downloads write them into the file. A downloaded link edits its
    file's tags as before. A corrected link in an album shows the album's cover (an album
    without one takes the link's thumbnail, cut square), and "Restore Site Details" asks
    yt-dlp again and drops the corrections.
  - Fix: a downloaded link no longer replaces its album's cover with the site's thumbnail
    embedded in the file; the album's own files (and their folder's cover) come first.
  - A live backdrop everywhere: the blurred cover (blurred once in Rust, so nothing moving
    has a CSS filter) breathes and its copies drift over each other with the theme's
    blobs, and with "Background moves with the music" (on by default) it swells softly with the
    loudness and beats (mpv's `astats` filter, read ~14 times a second only while a
    window shows it). Reduce Motion and power saving hold it still.
- **Mac** for round 4: how the pulse feels on real music and what Activity Monitor shows
  for Vitra's web content process with the backdrop moving (the Linux VM renders on the
  CPU, so its numbers say nothing about a Mac's GPU), Reduce Motion stopping it.
- **Mac**: how the view looks and scrolls (blur, karaoke fill, smooth scrolling in WebKit),
  Space in the sync view not pausing playback, saving into tags of an M4A (`©lyr`) and an MP3
  (`USLT`) and reading them back in Apple Music / Finder, the native save/import/export
  dialogs, LRCLIB lookups over the real network (curl on macOS).

## Stage 6 — Mini player

- Separate window (`mini.html`), opened by button, tray or hotkey.
- Modes: compact (artwork + controls + progress), slim bar, lyrics-only (karaoke line).
- Settings: always on top / normal, opacity, size, snap to screen edges, remember position,
  optional click-through. Uses the current theme and `<Glass>`.
- **Mac-only checks**: always-on-top over full-screen apps, click-through.
- Done and accepted (PR #7), with the owner's choices: click-through is left by holding ⌥ (Alt) over the
  mini player; opening it hides the main window (a setting); size presets plus dragging, a
  size per mode; the opacity fades only the background. Also: ⌥⌘M switches windows, it
  reopens at launch if it was open, "over full-screen apps" on macOS drops the Dock icon
  while it is the only window (macOS allows nothing else). The tray entry and a global
  hotkey come with stage 8.
- **Mac** for stage 6: on top over other apps and Spaces, over full-screen apps with the
  option, click-through with ⌥, the rounded vibrancy, its shadow and fading, dragging and
  resizing the borderless window, snapping at the menu bar and Dock.
- Review round: the compact layout keeps the text its room at every size (it broke on
  macOS), lyrics can be hidden everywhere (`lyrics.visible`), and an album link numbers
  tracks the library had before; albums keep their link (migration 7), so an album's page
  repairs that too.

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
- The owner's choices: the layout is **zones and panels** (left and right columns, top and
  bottom bars around the main view; modules move between zones, columns resize), not a
  free grid; the theme editor is a **panel over the window**, so the whole app is the
  live preview.
- Done and accepted (PR #8; the owner checked it on a Mac, including the review round below):
  - Themes: the schema gained fonts, a background picture and a layout; `.vitratheme`
    import (drop or button; the CSS is cleaned and what was taken out is shown; a theme's
    layout is offered, not forced), export, duplicate, delete; `crates/themes` reads
    packages defensively and `theme://` serves installed themes' assets.
  - The editor: colors, glass, type and fonts, corners and spacing, background (gradient,
    artwork, picture), motion, `custom.css`, assets; every change shows at once.
  - Layout: modules in zones (left/right columns, top/bottom bars), resizing columns and
    the split between modules, customize mode with dnd-kit, built-in and saved presets;
    two new modules, a big artwork panel and a level meter.
  - Rebindable shortcuts for every action in Settings, by key position (any keyboard
    layout); a few new actions (queue, lyrics, shuffle, repeat, customize layout).
  - Examples in `themes/` (Dunes, Neon Grid, Paper) and `docs/THEMES.md`.
- **Mac** for stage 7: the glass, refraction fallback and backdrop picture under real
  vibrancy / Liquid Glass; theme fonts and `theme://` assets in a release build (CSP);
  the traffic lights next to the top bar or the main view when the sidebar moves; ⌘-based
  shortcuts and the macOS menu's own (⌘Q, ⌘W, ⌘H, ⌘M stay the system's); dragging modules
  with a trackpad.
- Review round (the owner's notes after checking it on a Mac):
  - The level meter is a real spectrum: twenty bands from 40 Hz to 16 kHz measured in mpv's
    audio chain (files and links alike, nothing measured while the module is hidden), drawn
    on a canvas sixty times a second with fast rise, slow fall and falling peak caps; bars,
    mirrored bars or a wave (its context menu), a quiet wave while nothing plays, still with
    Reduce Motion.
  - Covers: `artwork.shape` (square, rounded, round) and `artwork.vinyl` in the theme and the
    editor; records turn at 33⅓ rpm while music plays (the big cover, the artwork module,
    the control bar, the mini player, the queue's current track), with grooves, a sheen and
    a label; the user can force them on or off (Settings, the disc button on the big cover).
  - Vitra Sans: Outfit (Latin) with Onest (Cyrillic), bundled, the built-in themes' font.
  - The no-cover placeholder is the mono Vitra symbol in the theme's color (or its own,
    `artwork.placeholderColor`), everywhere a cover shows.
  - Fixes: the mini player's dark edges (its content is clipped to its rounded corners and
    the window shadow redrawn); the backdrop's diagonal edge on wide screens (its moving
    pieces are big squares now).
- **Mac** for the review round:
  - The spectrum with real music and a link: bars move with what you hear (no lag or lead),
    CPU stays low (Activity Monitor: mpv and Vitra), nothing measured with the module hidden.
  - Records: the spin looks smooth, starts and stops gently with play/pause, stops in place;
    Reduce Motion (System Settings → Accessibility → Display) keeps them still.
  - Vitra Sans in a release build (CSP), Latin and Cyrillic mixed on one line look even.
  - The mini player's edges and corners are clean on light and dark themes, with vibrancy
    and Liquid Glass, faded and not, and its shadow follows the rounded shape. (Windows 11:
    rounded corners.)
  - The backdrop on a 2K (2560×1440) and an ultrawide (21:9) screen in full screen, on the
    lyrics view too: no edge or diagonal line.

## Stage 8 — Sound & system

- 10-band EQ with presets and custom presets (`lavfi` `equalizer` chain, updated live).
- ReplayGain (track/album), volume normalization (`dynaudnorm`/`loudnorm`), crossfade.
- Media keys and Now Playing via `souvlaki` (macOS, Windows SMTC, Linux MPRIS).
  **Required, not optional**: on macOS the system Now Playing widget (Control Center, the
  lock screen, the menu-bar item of AirPods / headphones) must show the playing track with
  its artwork, position and play/pause/next/previous/seek, and the media keys (F7–F9, the
  Touch Bar, headphone buttons) must control Vitra. Today nothing shows there: mpv runs with
  `--media-controls=no` and `--input-media-keys=no` (key decision 5), on purpose, because
  the queue is Vitra's, not mpv's. Vitra publishes `MPNowPlayingInfoCenter` and handles
  `MPRemoteCommandCenter` itself (through `souvlaki`, or `objc2-media-player` directly)
  from Rust's player state (title, artists, album, artwork from the artwork store,
  duration, position and rate, updated on seeks and pauses rather than every tick); mpv's
  own controls stay off so two sources never fight. It must keep
  working while the mini player is the only window (accessory activation policy) and for
  streamed links.
- Tray / menu-bar icon (`assets/brand/mono.svg`, a template image on macOS) with basic
  controls and the mini player; global hotkeys (`tauri-plugin-global-shortcut`).
- **Mac-only checks**: the Now Playing widget in Control Center and on the lock screen
  (track, artwork, progress, the buttons, scrubbing), media keys and headphone buttons with
  the app in the background, both while only the mini player is open, menu-bar icon.

## Stage 9 — Playlist import from other services

The owner's request (after stage 6): bring playlists over from Spotify, Yandex Music, VK
Music, Apple Music and others, **as simply as possible for the user**: no accounts, keys
or settings to set up first. Only the **list** comes over (titles, artists, album,
duration, ISRC where given); the sound comes from the user's own files or plays as a YouTube
Music link, like everything else in Vitra. Before My Wave (stage 10), which
reuses its YouTube Music search and matching when the library runs out.

Chosen (owner, after stage 6): files and pasted text first, then public links, then the
Music app's library on macOS. Not planned: the services' official APIs with the user's
account (Spotify's development mode allows 5 users per app since February 2026, so every
user would have to register their own Client ID; VK has had no audio API for third parties
since 2016), and link-mapping services such as song.link (they get every track).

### How it works for the user

One rule: **give Vitra the playlist in whatever form you have it, and it does the rest.**

- **Ways in, all leading to the same import:**
  - paste (⌘V) or drag a playlist link into the window, as YouTube links work today;
  - drop a playlist file on the window;
  - **Import Playlist…** (the "+" next to Playlists in the sidebar): one big field that
    takes a link or any copied text, a "Choose File…" button, and on macOS a **Music**
    tab listing the Music app's playlists with checkboxes.
- **No waiting and no review step.** The playlist appears in the sidebar at once, with
  every track in order. Tracks found in the library play right away; the others are
  looked up on YouTube Music in the background ("Found 48 of 60…" on the playlist's page)
  and light up as they are found, while you listen. Confident matches are taken by
  themselves; a doubtful one gets a small "?" with three alternatives to pick from (if you
  care to); a track that isn't found stays grey with "Find…" (the search, filled in).
- **Importing the same link again** updates the playlist and keeps your picks.
- **Per service, what the user does:**
  - **Spotify**: Share → Copy link to playlist, then paste. For a private playlist or
    Liked Songs: in the Spotify app select all tracks (⌘A), copy (⌘C) and paste into
    Vitra. Spotify copies them as track links, and Vitra reads each one.
  - **Yandex Music**: Поделиться → Скопировать ссылку, then paste. «Мне нравится» and
    private playlists work with the cookies setting (the browser you're signed in with);
    without it Vitra says the playlist is closed and offers that one setting, or making
    the playlist public.
  - **Apple Music** (Mac): the **Music** tab of the import, tick the playlists, Import.
    macOS asks once for access to Media & Apple Music. Someone else's public playlist:
    paste its link.
  - **VK Music**: open the playlist in the browser, select all (⌘A), copy (⌘C) and paste.
    Vitra finds the tracks in the page's text. Or a file from an exporter.
  - **Deezer, YouTube Music, SoundCloud, Bandcamp**: paste the link.
  - **Anything else**: a CSV / M3U / text file from TuneMyMusic, Soundiiz, FreeYourMusic or
    Exportify, or copied text.

### What it is made of

- **yt-dlp** (already here): Yandex Music albums and old-style playlist links
  (`/users/<login>/playlists/<n>`; it doesn't know the new `/playlists/<uuid>` share
  links), YouTube Music search (`youtube:music:search_url`, the `#songs` results are the
  labels' audio tracks, not clips), YouTube, SoundCloud, Bandcamp.
- **curl** (already here, as for LRCLIB): the JSON that Spotify's embed pages
  (`open.spotify.com/embed/…`) and Apple Music's pages (`serialized-server-data`) carry,
  Yandex Music's new share links, Deezer's public API. These are unofficial for Spotify,
  Apple and Yandex: each reader is small, tested on recorded pages, and fails with a clear
  "this link can't be read right now, copy the tracks instead" message.
- **objc2-itunes-library** (same objc2 family as the rest): the Music app's library on
  macOS, read-only and local.
- Files: `csv` and `quick-xml` (small, widely used; justified in the PR) for CSV, XSPF and
  the Music app's XML export; M3U and text by hand.

### Parts

1. **Matching** (`crates/import`, pure): an imported track against the library (the search
   fold of title and main artist, duration ±3 s, ISRC from `TSRC` / `ISRC` tags), then
   against YouTube Music results (title / artist similarity, duration), with a confidence
   score. "Remastered 2011", "feat.", "(Live)", ё/е and Latin/Cyrillic spellings are
   tested on real titles.
2. **The imported playlist**: a migration for entries that aren't tracks yet (title,
   artists, duration, the match state and alternatives) beside the playlist's tracks; a
   background matcher with progress (`import://progress`), one YouTube Music search every
   few seconds; picks and "Find…" from the UI; importing the same source again updates it.
3. **Files and text**: CSV (the exporters' column names), M3U / M3U8, XSPF, the Music app's
   XML / text export, and the smart paste: one track per line ("Artist — Title",
   "Title – Artist", a track link), or blocks copied from a page (title, artist, 3:45).
4. **Links**: Spotify playlists, albums and tracks (embed JSON), Spotify track links in
   bulk, Yandex Music (yt-dlp, plus the new share links), Apple Music pages, Deezer.
5. **The Music app** (macOS): its playlists in the import sheet, with counts and artwork;
   the "Media & Apple Music" usage description in `Info.plist`.
6. **UI**: Import Playlist… with the field, the file button and the Music tab; the
   playlist page's progress, "?" and "Find…"; en + ru strings.

### Mac checks for stage 9

- Each way in with the owner's real playlists: a Spotify link and Liked Songs copied with
  ⌘A ⌘C, a Yandex link and «Мне нравится», a VK page copied as text, Music app
  playlists (the permission prompt), a TuneMyMusic / Exportify file.
- Order, names (Russian titles, ё), how many found in the library and on YouTube Music,
  wrong matches and picking another.
- A few hundred tracks matched in the background without YouTube blocking us, while
  playback goes on.

## Stage 10 — My Wave

An endless stream picked by mood, like Yandex Music's "Моя волна", from the library and,
when that runs out, from YouTube Music. Added by the owner after stage 6; not in the
original spec. Each part is its own commit (or a few), testable on its own.

### 10.1 Sound analysis (`crates/mood`)

- One FFmpeg run per track decodes it to mono at 22 050 Hz and prints per-window values
  with FFmpeg's own filters: `astats` (RMS level) and `aspectralstats` (spectral centroid and
  spectral flux). No new native dependency and no FFT of our own; the Rust side only reads
  numbers (`ametadata=print`) and does the statistics. Files are analyzed in three 30 s
  slices (around 20 %, 50 % and 80 % of the track) rather than whole, which keeps a track
  to a second or two of CPU.
- Features: **energy** (mean RMS) and its **spread** (how much the loudness moves),
  **tempo** (BPM from the autocorrelation of the spectral flux, i.e. the onset strength,
  folded into 70–180 with a confidence), **brightness** (mean spectral centroid). Optional,
  last: a **mode / "sadness"** estimate (major vs minor from a chroma profile; FFmpeg has
  no filter for that, so it needs raw PCM and a small pure-Rust FFT such as `rustfft`, a
  new dependency to justify in its PR).
- The mood vector is those features normalized against the library's own distribution
  (percentiles), so "energetic" means energetic for this library. A `version` number:
  improving the algorithm re-analyzes everything once.
- Pure parsing and statistics are unit-tested on recorded FFmpeg output and on generated
  tones and click tracks (known BPM, known loudness); one opt-in test runs a real FFmpeg.

### 10.2 Background analysis and storage

- A migration: `track_features` (track id, version, source `file` / `stream` / `live`, the
  file's size and mtime when analyzed, energy, spread, BPM + confidence, brightness,
  valence, analyzed at). A file is analyzed again only when its size or mtime (or the
  version) changes; tracks that fail are remembered so they aren't retried every launch.
- A worker thread analyzes one track at a time at low priority: FFmpeg with `-threads 1`,
  under `nice -n 15` on macOS / Linux (and the background QoS class on macOS) and
  `IDLE_PRIORITY_CLASS` on Windows. It pauses while a scan runs and with power saving on,
  and starts with the tracks most likely to be needed (recently played and favorites).
- Settings → My Wave: progress ("1 234 of 5 678 analyzed"), pause/resume, analyze links
  on/off; `wave://progress` events. Quitting stops FFmpeg at once.

### 10.3 Links without downloading

- From the stream: yt-dlp gives the audio URL and its HTTP headers (`-f bestaudio -J`);
  FFmpeg reads 60–90 s from the middle (`-ss` before `-i`, so only that range is
  fetched). Slowly, so YouTube doesn't block us: one link at a time with a randomized pause
  of tens of seconds between them, the cookies setting honored, and a long back-off after a
  403 / 429 / sign-in error (the existing `ytdlp::failure` classes).
- While listening: the `@vitralevel` chain that already runs for the backdrop pulse
  (`player/level.rs`) gets `aspectralstats` too, and the playing link's values are
  accumulated into a `live` analysis when enough of it has played (no extra traffic).
- The next 2–3 tracks of a wave are analyzed ahead of the background queue, so a pick
  never waits on the network.
- Offline (yt-dlp can't reach the site), the wave uses files and links that are already
  analyzed; nothing else is a candidate.

### 10.4 The wave (pure, in `crates/mood`)

- State: the **course** (a target point in mood space), the wave's history, likes and
  dislikes, a seeded RNG (deterministic tests).
- Each pick scores every analyzed candidate: closeness to the course (weighted per feature)
  plus artist / genre / year affinity with the wave so far, plus history (what is usually
  played after what: transitions from the play history), favorites and likes add, skips
  (in waves and outside them, decaying with time) and recently played tracks subtract.
- The next track is a weighted random choice among the best few (softmax with a
  temperature), never the same artist twice in a row and never a track already in this
  wave. The course drifts a little toward each track that is listened to and gets a small
  random walk, so the wave doesn't get stuck in one corner.
- Modes set the starting course: **Energetic** (high energy, tempo, brightness), **Calm**
  (low energy and tempo, little spread), **Sad** (slow, dark, minor where known), **Like
  the current track** (the playing track's vector). "Не нравится" skips and pushes the
  course away from that track; "Нравится" adds it to favorites and holds the course near
  it for a while.
- Tested with a synthetic library: modes pick the right region, no artist repeats, dislikes
  steer away, likes hold, the drift is bounded, 10 000 tracks pick in well under 10 ms.

### 10.5 Playback and UI

- The queue knows it is a wave (`QueueSource::Wave`): Rust keeps 2–3 tracks ahead (so
  gapless and preloading work as now) and tops it up as tracks play; playing something
  else ends the wave. Skips and likes reach the wave state; the session restores a wave.
- "Моя волна" button in the library (sidebar entry and the library header) and in Now
  Playing, with the mode chooser; during a wave, "Нравится" / "Не нравится" in Now Playing,
  the control bar and the mini player; the queue panel shows the wave's upcoming picks.
- Settings section as in 10.2; en + ru strings.

### 10.6 When the library runs out

- When a mode has too few good candidates (a small library, or everything was played
  recently), similar tracks come from the YouTube Music radio of a seed track:
  `https://music.youtube.com/watch?v=<id>&list=RDAMVM<id>` through `yt-dlp --flat-playlist`
  (the seed is the current link, else the track found by stage 9's YouTube Music search
  and matching).
- Those entries are analyzed (10.3) before they can be picked, play as links (joining the
  library under `library.addOpenedFiles`), and are rate-limited like everything else that
  talks to YouTube.

### Mac checks for stage 10

- Background analysis of the real library (FLAC, ALAC, MP3, M4A): playback never stutters,
  Activity Monitor shows FFmpeg at low priority with a small energy impact, also on
  battery; quitting during analysis leaves no FFmpeg behind; tracks in iCloud-offloaded
  folders are skipped rather than downloaded.
- Progress in Settings, pause/resume, re-analysis after a file's tags or audio change.
- Links analyzed over the real network (with Safari cookies if set), no blocks after an
  hour of waves; with Wi-Fi off the wave keeps playing analyzed tracks only.
- The modes feel right on the owner's music; "Нравится" / "Не нравится"; no artist twice
  in a row; the YouTube Music fallback with a small library.
- The wave from the Now Playing widget and media keys (next) after stage 8, and from the
  mini player.

## Stage 11 — Release

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
| Shipping `mpv.app` (dylibs) inside the bundle complicates signing/notarization             | Sign nested binaries in the release workflow (stage 11). Unsigned local builds work.                                                          |
| YouTube changes break yt-dlp                                                               | Updatable yt-dlp in app data + "Update yt-dlp" button; Deno for EJS challenges.                                                               |
| Installer size: static FFmpeg+ffprobe ≈ 330 MB (Win/Linux), Deno ≈ 100 MB, mpv ≈ 60–100 MB | See open question 1.                                                                                                                          |
| `backdrop-filter` with SVG filters only works in Chromium                                  | Feature detection + blur/highlight fallback (WebKit on macOS/Linux).                                                                          |
| Transparent windows on Linux need a compositor                                             | Detect and fall back to an opaque theme background.                                                                                           |
| A hard crash on Windows can orphan mpv (no fd-passing IPC there)                           | Assign mpv to a Job Object with kill-on-close (stage 11); macOS/Linux already tie mpv's life to the IPC socket.                               |
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
