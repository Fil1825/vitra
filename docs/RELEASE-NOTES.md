# Vitra 0.1.0 — test release

An open-source desktop music player with a Liquid Glass interface, English and Russian,
custom themes and layouts, and a separate mini player.

- Local library, watched folders, albums, artists, playlists, and search.
- Audio playback and downloads from supported links through mpv, yt-dlp, FFmpeg, and Deno.
- Embedded and synchronized lyrics, LRC import/export, and a lyrics editor.
- Equalizer, loudness normalization, crossfade, media keys, and system controls.
- Playlist import and My Wave recommendations from the library and YouTube Music radio.
- System certificate support for HTTPS stream analysis, retry of failed analyses, and
  visible diagnostics for recurring decoder failures.

Discord and Last.fm integrations are not included. macOS packages require macOS 14+
(Apple Silicon) or 15+ (Intel); Linux Debian packages also install system mpv.

This is the first public test release (GitHub prerelease). Current macOS candidates use ad-hoc signatures
and are not notarized; Windows candidates are unsigned. Native UI and clean-machine
acceptance still need confirmation; official signing requires a future Developer ID account.
Third-party sites may change or require
sign-in. The user is responsible for complying with their terms and applicable copyright law.
