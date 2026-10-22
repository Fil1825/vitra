<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/logo-horizontal-dark.svg">
    <img alt="Vitra" src="assets/brand/logo-horizontal-light.svg" width="360">
  </picture>
</p>

An open-source desktop music player with a **Liquid Glass** look — fully themeable, plays
virtually any audio format, streams audio from links (YouTube, SoundCloud and hundreds of other
sites) and shows synced lyrics. Runs on macOS, Windows and Linux.

> **Status:** early development (stage 5 of the [plan](docs/PLAN.md)): local playback, the
> music library, the play queue, playing and downloading links, and lyrics work; the mini
> player, theme editor and releases come later. Nothing to install yet.

## Links

Paste a link anywhere in the window (or press ⌘U / Ctrl+U): YouTube, YouTube Music,
SoundCloud, links to audio files and internet radio (`.m3u`, `.pls`, streams). Vitra plays the
audio only. Playlist links become playlists in the library. **Download** saves a link into the
library (original audio, MP3 or FLAC, with cover and tags) and keeps its play count, favorites
and playlist entries. Sites change often; Settings → Downloads → yt-dlp → **Update** gets the
latest yt-dlp.

## Lyrics

Vitra shows the lyrics in a song's tags (synced `SYLT`/LRC or plain) or in a `.lrc` file next
to it, following the song line by line, word by word when the lyrics have word timings. Click
a line to jump to it; text size and a sync offset are adjustable while it plays. The editor
(the pencil in the lyrics view, or **Edit Lyrics…** on a song) takes pasted, typed or imported
lyrics, syncs them as you tap Space at each line, and saves them into the file's tags (after
making a backup copy), a `.lrc` file, or Vitra's library only. Lyrics can also be looked up on
[LRCLIB](https://lrclib.net) — off by default; turning it on sends the song's title, artist,
album and length there.

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

## Disclaimer

Vitra plays and downloads audio from third-party sites through yt-dlp. You are responsible for
complying with the terms of service of the sites you use it with and with copyright law in your
country.

## License

[GPL-3.0-or-later](LICENSE). Vitra's look is inspired by Apple Music, but it contains no Apple
icons, logos, fonts or other assets.
