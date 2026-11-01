# yt-dlp fixtures

`ytdlp-SHA2-256SUMS` and `ytdlp-SHA2-256SUMS.sig` are the checksum file and detached
signature from the yt-dlp 2026.10.26 release (signed 2026-10-26). The signature test checks
them against the key embedded in `src/ytdlp_release_key.asc`. They are bytes from upstream,
not regenerated.

`generic-*.json` were recorded from yt-dlp's generic extractor against a local test page
(`tests/real_ytdlp.rs` builds the same pages).

`youtube-*.json` and `soundcloud-*.json` were written by hand in yt-dlp's `-J` format, because
the cloud environment that wrote them cannot reach those sites; the tests check them for exact
values. Real recordings go next to them, as `<name>-recorded.json`:

```sh
scripts/record-ytdlp-fixtures.sh
```

The script finds the fetched yt-dlp (on macOS the folder build, `yt-dlp/yt-dlp_macos`), keeps
only the fields the parser reads, and replaces a recording only when yt-dlp succeeded. The
parser tests check every recording they find for what doesn't depend on the site's data:

- `youtube-video-recorded.json`: a video; artist from the title or the channel.
- `youtube-music-recorded.json`: a track of an auto-generated "Artist - Topic" channel, which
  has music tags (track, artists, album); official videos don't (`VITRA_MUSIC_URL` picks
  another one).
- `youtube-radio-recorded.json`: YouTube Music's radio of that track (`RDAMVM<id>`), the
  list My Wave takes similar songs from.
- `soundcloud-track-recorded.json`, `soundcloud-set-recorded.json`.
- `youtube-album-recorded.json` (an `OLAK5uy_` playlist), `youtube-album-page-recorded.json`
  (its `music.youtube.com/browse/MPREb…` page) and `youtube-album-entry-recorded.json` (one of
  its tracks in full); `VITRA_ALBUM_URL` and `VITRA_ALBUM_PAGE_URL` pick another album.
