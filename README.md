<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/logo-horizontal-dark.svg">
    <img alt="Vitra" src="assets/brand/logo-horizontal-light.svg" width="360">
  </picture>
</p>

An open-source desktop music player with a **Liquid Glass** look — fully themeable, plays
virtually any audio format, streams audio from links (YouTube, SoundCloud and hundreds of other
sites) and shows synced lyrics. Runs on macOS, Windows and Linux.

English · [Русский](README.ru.md)

> **Status:** release preparation (stage 11 of the [plan](docs/PLAN.md)). My Wave, the library,
> playback, downloads, lyrics, themes, layouts, and playlist import are implemented. A stable
> public release has not been published yet. Discord and Last.fm are outside this release.

![My Wave in the English interface](docs/screenshots/my-wave-en.png)

Interface preview in Chromium, using the Aurora Night theme. Native window materials vary
by platform. [Russian appearance settings](docs/screenshots/settings-ru.png).

## Install a build candidate

Download installers from [GitHub Releases](https://github.com/Fil1825/vitra/releases).
The first release is marked **Pre-release** for testing. Choose `Vitra-macOS-Apple-Silicon.dmg`,
`Vitra-macOS-Intel.dmg`, `Vitra-Windows-x64.exe`, or `Vitra-Linux-x64.deb` for your system.
The workflow publishes only after all four platforms pass their checks. If no release is
listed yet, see [build progress](https://github.com/Fil1825/vitra/actions/workflows/release.yml).
See [release preparation](docs/RELEASING.md) for build and verification details.

- macOS: open the DMG and move Vitra to Applications. Requires macOS 14+ on Apple Silicon,
  or macOS 15+ on Intel. Current candidates use ad-hoc signatures and are not notarized.
- Windows 10/11 x64: run the NSIS installer. Current candidates are unsigned.
- Ubuntu 24.04 / compatible Debian x64: install the `.deb` with
  `sudo apt install ./Vitra-Linux-x64.deb`; mpv is installed as a package dependency.

The packages include yt-dlp, FFmpeg, ffprobe, and Deno; macOS and Windows also include mpv.
They do not require Node.js or Rust on the listener's machine. Official signing and
clean-machine acceptance are tracked in the release guide.

## My Wave

An endless queue picked by how music sounds: Your Taste, Energetic, Calm, Sad, or a wave
like a chosen track. Vitra measures local files and, when enabled, stretches of link audio.
Likes, dislikes, and skips shape later picks. Optional YouTube Music radio finds related
songs when the library runs short. Settings → My Wave controls analysis and online access,
and shows recurring stream-decoder problems. Analysis uses OS certificates for HTTPS.

## Links

Paste a link anywhere in the window (or press ⌘U / Ctrl+U): YouTube, YouTube Music,
SoundCloud, links to audio files and internet radio (`.m3u`, `.pls`, streams). Vitra plays the
audio only. Playlist links become playlists in the library. **Download** saves a link into the
library (original audio, MP3 or FLAC, with cover and tags) and keeps its play count, favorites
and playlist entries. Sites change often; Settings → Downloads → yt-dlp → **Update** gets the
latest yt-dlp.

Album links (YouTube Music albums, `OLAK5uy_…` playlists, Bandcamp albums) open as an album:
**Listen**, or **Download Album** into `Artist/Album/01 Title` with the album's tags and cover.
For age-restricted or sign-in-only videos, Settings → Downloads → **Cookies from browser**
lets yt-dlp use the sign-in of a browser on this computer.

Re-uploads often credit the channel instead of the artist and add "(Official Video)" to the
title: **Edit Tags…** on a link corrects its title, artists and album (or moves it into an
album you already have, which then lends it its cover). The corrections stay in Vitra's
library, survive opening the link again, and go into the file's tags when you download it.
**Restore Site Details** in the same window takes the site's details back.

## Importing playlists

Bring a playlist over from another service as you have it — no accounts or keys: paste its
link anywhere in the window (Spotify, Apple Music, Deezer, Yandex Music), drop a playlist file
on the window (M3U, XSPF, the Music app's XML export, CSV from Exportify, Soundiiz or
TuneMyMusic), or use **Import Playlist…** under the "+" next to Playlists, which also takes
copied tracks ("Artist — Title" per line, or a page's text such as a VK playlist selected and
copied in the browser) and, on a Mac, lists the **Music** app's playlists. Only the list comes
over: songs your library has play at once, the others are looked for on YouTube Music in the
background and light up as they're found. A match Vitra isn't sure of has a **?** with the
alternatives; one it didn't find stays grey — click it to search yourself. Importing the same
link again (or **Update from Source**) brings the playlist up to date and keeps your choices.
Spotify shows visitors the first 100 songs of a playlist: for the rest (and for Liked Songs),
select the songs in Spotify, copy them and paste them into the import. Yandex Music's «Мне
нравится» and private playlists use the browser chosen in Settings → Downloads → Cookies from
browser.

## Library

Vitra watches the folders you add and groups their songs by album and artist. Songs with
several artists (several `ARTIST` values, `ARTISTS`, or a credit like "A feat. B") are listed
under each of them; Settings → Library sets how credits are split and which names contain a
comma ("Tyler, The Creator"). Albums, artists and playlists can have a cover of your choice:
use the button on the cover, right-click, or drop an image file on it (files aren't changed).

## Lyrics

Vitra shows the lyrics in a song's tags (synced `SYLT`/LRC or plain) or in a `.lrc` file next
to it, following the song line by line, word by word when the lyrics have word timings. Click
a line to jump to it; text size and a sync offset are adjustable while it plays. The editor
(the pencil in the lyrics view, or **Edit Lyrics…** on a song) takes pasted, typed or imported
lyrics, syncs them as you tap Space at each line, and saves them into the file's tags (after
making a backup copy), a `.lrc` file, or Vitra's library only. Lyrics can also be looked up on
[LRCLIB](https://lrclib.net) — off by default; turning it on sends the song's title, artist,
album and length there. Don't want them? The crossed-out eye in the lyrics view hides lyrics
everywhere until the microphone in the control bar brings them back.

## Mini player

**⌥⌘M** (Ctrl+Alt+M) or the last button of the control bar swaps the main window for a small
glass player that stays on top: compact (artwork, controls, progress), a slim bar, or just the
playing line of the lyrics. Drag it anywhere (it snaps to the screen's edges) or by its edges
to resize it; right-click for its mode, size, background opacity, always on top and
click-through (hold ⌥ / Alt over it to use it then). Settings → Mini Player has the rest.

## Sound

Settings → Sound has a ten-band equalizer with the usual presets (Rock, Jazz, Bass Booster…)
and your own (shape the curve, then **Save as Preset**), **Normalization** that plays every
song about as loud as the next — by its ReplayGain tags, or by Vitra's own measurement for
songs without them, made once per song (per album in Album mode) — and **Crossfade**: songs
blend into each other over up to 12 seconds, except songs of one album, which follow each
other without a gap as recorded.

## In the system

Vitra shows what plays where the system shows it: Control Center and the lock screen on macOS,
the media overlay on Windows, the desktop's media controls on Linux (MPRIS), with the cover and
the progress. The keyboard's media keys and the buttons on your headphones control it while
another app is in front, also with only the mini player open. The icon in the menu bar (the
notification area on Windows) has the track, the transport buttons, the mini player and Quit.
Playback and the mini player can also get shortcuts that work in every app (Settings → Keyboard
Shortcuts, the second column).

## Look

The window's background comes from the playing song's cover: blurred, slowly breathing and
flowing, and gently pulsing with the music (Settings → Appearance → **Background moves with the
music**). It holds still when the system asks for reduced motion or in power-saving mode.

Covers can be square, rounded or round, and with **Vinyl** (Settings → Appearance, or the disc
button on the big cover) they become records that turn while the music plays. The text is set
in Vitra Sans, made of [Outfit](https://github.com/Outfitio/Outfit-Fonts) and
[Onest](https://github.com/googlefonts/onest) (both SIL Open Font License, shipped with the app).

## Themes and layout

Settings → Appearance has the themes: Light, Dark, Aurora Night and your own. **New Theme**
opens an editor over the window (colors, glass, fonts, corners, background picture, motion,
custom CSS) and the whole app shows every change as you make it. Themes travel as
`.vitratheme` files: drop one on the window to install it, or export yours from its menu.
Three examples are in [`themes/`](themes); [docs/THEMES.md](docs/THEMES.md) describes the
format.

The window is made of modules — sidebar, queue, lyrics, big artwork, player controls, level
meter — in columns on either side and bars above and below. Drag a column's edge or the line
between two modules to resize; the layout button in the title bar (or Settings → Layout)
lets you drag modules between places, hide them, and save layouts you like. The level meter
shows the music's spectrum as bars, mirrored bars or a wave (right-click it).

Every keyboard shortcut can be changed in Settings → Keyboard Shortcuts. Icons move a little
under the pointer, each in its own way (not with reduced motion).

## Building from source

Requirements: [Rust](https://rustup.rs) (stable), Node.js 22.12+, and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
npm ci
npm run fetch-sidecars   # downloads mpv, yt-dlp, FFmpeg and Deno for your platform
npm run tauri dev
```

On Linux, install mpv from your package manager (`sudo apt install mpv`); there are no official
Linux mpv binaries to download.

For a distributable installer use `npm run bundle`, which includes the fetched helper
programs and refuses incomplete downloads. A plain `npm run tauri build` is intended for
developers who provide those tools themselves. See [CONTRIBUTING.md](CONTRIBUTING.md),
[the release guide](docs/RELEASING.md), and [the TLS regression checks](docs/TLS.md).

## Disclaimer

Vitra plays and downloads audio from third-party sites through yt-dlp. You are responsible for
complying with the terms of service of the sites you use it with and with copyright law in your
country.

## License

[GPL-3.0-or-later](LICENSE). Vitra's look is inspired by Apple Music, but it contains no Apple
icons, logos, fonts or other assets. The bundled fonts are under the SIL Open Font License
(`src/assets/fonts/OFL-*.txt`, `themes/*/OFL.txt`).
